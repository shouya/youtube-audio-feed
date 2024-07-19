use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Instant,
};

use kameo::{actor::ActorRef, messages, Actor};
use lru_time_cache::{Entry, LruCache};
use tokio::fs::File;

use crate::{Error, Result};

pub struct AudioFile {
  id: String,
  path: PathBuf,
  temp_path: PathBuf,
  #[allow(dead_code)]
  created_at: Instant,
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
  ) -> Result<(Arc<AudioFile>, bool)> {
    if let (Some(file), evicted) = self.files.notify_get(&audio_id) {
      drop(evicted);
      return Ok((file.clone(), false));
    }

    let file = AudioFile::new(&self.base_dir, &audio_id);
    let value = Arc::new(file);
    self.files.insert(audio_id, value.clone());
    Ok((value, true))
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
  ) -> Result<(Arc<AudioFile>, bool)> {
    Ok(self.0.ask(GetOrAllocate { audio_id }).send().await.unwrap())
  }

  pub async fn remove(&self, audio_id: String) -> Result<()> {
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
      created_at: Instant::now(),
    }
  }

  pub async fn open(&self) -> Result<File> {
    File::open(&self.path).await.map_err(Error::IO)
  }

  pub fn id(&self) -> &str {
    &self.id
  }

  pub fn ready(&self) -> bool {
    self.path.exists()
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn temp_path(&self) -> &Path {
    &self.temp_path
  }
}
