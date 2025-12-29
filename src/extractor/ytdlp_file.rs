use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::Regex;
use tokio::fs::File;
use tokio::process::Command;
use tracing::warn;

use crate::audio_store::{AudioFile, AudioStoreRef};
use crate::util::YTDLP_PROXY;
use crate::{Error, Result, YTDLP_MUTEX};

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

    match audio_file
      .get_or_download(|| async { download_file(&audio_file).await })
      .await
    {
      Ok(file) => serve_file(file).await,
      Err(e) => {
        warn!("error getting audio file {}: {}", video_id, e);

        // delete errored file
        drop(audio_file);
        self.audio_store.remove(video_id).await.unwrap();
        Err(e)
      }
    }
  }
}

async fn download_file(audio_file: &AudioFile) -> Result<()> {
  let audio_id = &audio_file.id;
  let temp_path = &audio_file.temp_path;
  let url = format!("https://youtube.com/watch?v={audio_id}");
  eprintln!("downloading audio file: {}", url);

  let mut cmd = Command::new("yt-dlp");

  cmd
    .arg("-f")
    .arg("ba[ext=m4a]")
    .arg("--no-progress")
    .arg("-o")
    .arg(temp_path)
    .arg("--no-mtime")
    .arg(url);

  if let Some(proxy) = &*YTDLP_PROXY {
    // used to remove cred info from proxy url before printing
    static AUTH_REGEX: LazyLock<Regex> =
      LazyLock::new(|| Regex::new(r"//[^:]+(:[^@]+)@").unwrap());
    eprintln!(
      "using proxy: {}",
      AUTH_REGEX.replace(proxy, "//<REDACTED>@")
    );
    cmd.arg("--proxy").arg(proxy);
  }

  let guard = YTDLP_MUTEX.acquire().await.unwrap();
  let child = cmd.stderr(std::process::Stdio::piped()).spawn()?;
  let output = child.wait_with_output().await?;
  drop(guard);
  detect_error(&output.stderr)?;

  std::fs::rename(temp_path, &audio_file.path).map_err(Error::IO)?;

  Ok(())
}

async fn serve_file(file: File) -> Result<Extraction> {
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
