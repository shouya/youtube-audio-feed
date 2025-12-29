use std::collections::HashMap;

use async_trait::async_trait;

use crate::{Error, Result, YTDLP_MUTEX};

use super::{Extraction, Extractor};

// run yt-dlp command line to get audio playback url.
// requires yt-dlp executable to be in PATH.
#[allow(unused)]
pub struct YtdlpProxy;

#[async_trait]
impl Extractor for YtdlpProxy {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
    use serde::Deserialize;
    use tokio::process::Command;

    #[derive(Deserialize, Debug)]
    struct YtdlpOutput {
      formats: Vec<Format>,
    }

    #[derive(Deserialize, Debug)]
    struct Format {
      #[allow(unused)]
      format: String,
      quality: Option<f32>,
      resolution: String,
      audio_ext: String,
      url: String,
      http_headers: HashMap<String, String>,
    }

    let url = format!("https://youtube.com/watch?v={video_id}");
    let guard = YTDLP_MUTEX.acquire().await;
    let output = Command::new("yt-dlp").arg("-j").arg(url).output().await?;
    drop(guard);
    let output =
      String::from_utf8(output.stdout).map_err(|_| Error::Extraction)?;
    let output: YtdlpOutput =
      serde_json::from_str(&output).map_err(|_| Error::Extraction)?;

    let format = output
      .formats
      .into_iter()
      .filter(|format| {
        format.resolution == "audio only" && format.audio_ext == "m4a"
      })
      .max_by_key(|format| format.quality.unwrap_or(0.0) as i64)
      .ok_or_else(|| Error::Extraction)?;

    let url = format.url.clone();
    let headers = format.http_headers.into_iter().collect();

    Ok(Extraction::Proxy { url, headers })
  }
}
