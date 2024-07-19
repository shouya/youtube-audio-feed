use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Instant,
};

use bytes::Bytes;
use futures::{Stream, StreamExt as _};
use kameo::{actor::ActorRef, messages, Actor};
use lru_time_cache::{Entry, LruCache};
use tokio::{fs::File, io::BufReader, sync::RwLock};
use tokio_util::io::ReaderStream;

use crate::{Error, Result};

enum AudioStatus {
  Pending,
  Finished {
    #[allow(unused)]
    file_size: u64,
    #[allow(unused)]
    finished_at: Instant,
  },
}

pub struct AudioFile {
  id: String,
  path: PathBuf,
  #[allow(unused)]
  created_at: Instant,
  status: RwLock<AudioStatus>,
}

impl Drop for AudioFile {
  fn drop(&mut self) {
    // delete the file on drop
    if !self.path.exists() {
      return;
    }

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
    match self.files.entry(audio_id.clone()) {
      Entry::Occupied(entry) => {
        let entry = entry.into_mut();
        while !entry.is_finished().await {
          tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        Ok(entry.clone())
      }
      Entry::Vacant(entry) => {
        let file = Arc::new(AudioFile::new(&self.base_dir, &audio_id));
        entry.insert(file.clone());
        Ok(file)
      }
    }
  }
}

impl AudioStore {
  pub fn new(base_dir: impl AsRef<Path>) -> Self {
    let files = LruCache::with_expiry_duration_and_capacity(
      std::time::Duration::from_secs(60 * 60),
      100,
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
}

impl AudioFile {
  fn new(base_dir: &Path, audio_id: &str) -> Self {
    let file_path = base_dir.join(audio_id).with_extension("m4a");
    Self {
      id: audio_id.to_string(),
      path: file_path,
      created_at: Instant::now(),
      status: RwLock::new(AudioStatus::Pending),
    }
  }

  pub async fn mark_finished(&self) -> Result<()> {
    let mut write = self.status.write().await;
    *write = AudioStatus::Finished {
      file_size: self.path.metadata()?.len(),
      finished_at: Instant::now(),
    };
    Ok(())
  }

  pub async fn is_finished(&self) -> bool {
    let read = self.status.read().await;
    matches!(&*read, AudioStatus::Finished { .. })
  }

  #[allow(unused)]
  pub async fn size(&self) -> Option<u64> {
    let read = self.status.read().await;

    match &*read {
      AudioStatus::Finished { file_size, .. } => Some(*file_size),
      _ => None,
    }
  }

  pub async fn open(&self) -> Result<File> {
    File::open(&self.path).await.map_err(Error::IO)
  }

  #[allow(unused)]
  pub async fn read(&self) -> Result<impl Stream<Item = Result<Bytes>>> {
    while !self.is_finished().await {
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let file = File::open(&self.path).await?;
    let stream = ReaderStream::new(BufReader::new(file));

    Ok(stream.map(|r| r.map(Bytes::from).map_err(Error::IO)))
  }

  pub fn id(&self) -> &str {
    &self.id
  }

  pub fn path(&self) -> &Path {
    &self.path
  }
}
