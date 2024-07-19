use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command;

use crate::audio_store::{AudioFile, AudioStoreRef};
use crate::{Error, Result};

use super::{Extraction, Extractor};

// run yt-dlp command line to get audio stream directly.
// requires yt-dlp executable to be in PATH.
pub struct YtdlpFile {
  audio_store: Arc<AudioStoreRef>,
}

impl YtdlpFile {
  pub fn new(audio_store: Arc<AudioStoreRef>) -> Self {
    Self { audio_store }
  }
}

#[async_trait]
impl Extractor for YtdlpFile {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
    let audio_file = self
      .audio_store
      .get_or_allocate(video_id.to_string())
      .await?;

    if audio_file.is_finished().await {
      serve_file(&audio_file).await
    } else {
      download_file(&audio_file).await?;
      serve_file(&audio_file).await
    }
  }
}

async fn download_file(audio_file: &AudioFile) -> Result<()> {
  let audio_id = audio_file.id();
  let file_path = audio_file.path();
  let url = format!("https://youtube.com/watch?v={audio_id}");

  let child = Command::new("yt-dlp")
    .arg("-f")
    .arg("ba[ext=m4a]")
    .arg("--no-progress")
    .arg("-o")
    .arg(file_path)
    .arg(url)
    .stderr(std::process::Stdio::piped())
    .spawn()?;

  let output = child.wait_with_output().await?;
  detect_error(&output.stderr)?;
  audio_file.mark_finished().await?;

  Ok(())
}

async fn serve_file(audio_file: &AudioFile) -> Result<Extraction> {
  let file = audio_file.open().await?;
  let mime_type = "audio/mp4".to_string();
  Ok(Extraction::File { file, mime_type })
}

fn detect_error(bytes: &[u8]) -> Result<()> {
  let s = String::from_utf8_lossy(bytes);
  if s.contains("ERROR:") {
    Err(Error::AudioStream(s.to_string()))
  } else {
    Ok(())
  }
}
