use std::{
  future::Future,
  path::{Path, PathBuf},
  sync::Arc,
  time::Instant,
};

use kameo::{actor::ActorRef, messages, Actor};
use lru_time_cache::LruCache;
use tokio::{fs::File, sync::Mutex};
use tracing::warn;

use crate::{Error, Result};

pub enum AudioFileState {
  New,
  Ready,
}

pub struct AudioFile {
  pub id: String,
  pub path: PathBuf,
  pub temp_path: PathBuf,
  pub state: Mutex<AudioFileState>,
  #[allow(dead_code)]
  pub created_at: Instant,
}

impl Drop for AudioFile {
  fn drop(&mut self) {
    // delete the file on drop
    if !self.path.exists() {
      return;
    }

    std::fs::remove_file(&self.temp_path).ok();

    if let Err(e) = std::fs::remove_file(&self.path) {
      eprintln!("failed to delete file: {}", e);
    } else {
      eprintln!("deleted file: {}", self.path.display());
    }
  }
}

#[derive(Actor)]
pub struct AudioStore {
  base_dir: PathBuf,
  files: LruCache<String, Arc<AudioFile>>,
}

pub struct AudioStoreRef(ActorRef<AudioStore>);

#[messages]
impl AudioStore {
  #[message]
  async fn get_or_allocate(
    &mut self,
    audio_id: String,
  ) -> Result<Arc<AudioFile>> {
    if let Some(file) = self.files.get(&audio_id) {
      return Ok(file.clone());
    }

    let file = AudioFile::new(&self.base_dir, &audio_id);
    let value = Arc::new(file);
    self.files.insert(audio_id, value.clone());
    Ok(value)
  }

  #[message]
  async fn remove(&mut self, audio_id: String) {
    self.files.remove(&audio_id);
  }
}

impl AudioStore {
  pub fn new(base_dir: impl AsRef<Path>) -> Self {
    let files = LruCache::with_expiry_duration_and_capacity(
      // expire after 10 minutes
      std::time::Duration::from_secs(10 * 60),
      // store up to 30 files
      30,
    );

    // delete all existing files
    std::fs::remove_dir_all(&base_dir).ok();
    std::fs::create_dir_all(&base_dir).unwrap();

    Self {
      base_dir: base_dir.as_ref().to_owned(),
      files,
    }
  }

  pub fn spawn(self) -> AudioStoreRef {
    AudioStoreRef(kameo::spawn(self))
  }
}

impl AudioStoreRef {
  pub async fn get_or_allocate(
    &self,
    audio_id: String,
  ) -> Result<Arc<AudioFile>> {
    Ok(self.0.ask(GetOrAllocate { audio_id }).send().await.unwrap())
  }

  pub async fn remove(&self, audio_id: &str) -> Result<()> {
    let audio_id = audio_id.to_string();
    self.0.ask(Remove { audio_id }).send().await.unwrap();
    Ok(())
  }
}

impl AudioFile {
  fn new(base_dir: &Path, audio_id: &str) -> Self {
    let file_path = base_dir.join(audio_id).with_extension("m4a");
    let temp_path = base_dir.join(audio_id).with_extension("temp.m4a");
    Self {
      id: audio_id.to_string(),
      path: file_path,
      temp_path,
      state: Mutex::new(AudioFileState::New),
      created_at: Instant::now(),
    }
  }

  pub async fn open(&self) -> Result<File> {
    File::open(&self.path).await.map_err(Error::IO)
  }

  pub async fn get_or_download<F, Fut>(&self, dl: F) -> Result<File>
  where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<()>>,
  {
    let mut guard = self.state.lock().await;
    if let AudioFileState::Ready = &*guard {
      return self.open().await;
    };


    dl().await?;

    if !self.path.exists() {
      warn!("audio file not found after download: {}", self.path.display());
      return Err(Error::AudioStream(self.id.clone()));
    }

    *guard = AudioFileState::Ready;
    self.open().await
  }
}
