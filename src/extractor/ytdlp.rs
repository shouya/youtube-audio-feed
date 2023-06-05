use std::collections::HashMap;

use async_trait::async_trait;

use crate::{Error, Result};

use super::{Extraction, Extractor};

// run yt-dlp command line to get video playback url.
// requires yt-dlp executable to be in PATH.
pub struct Ytdlp;

#[async_trait]
impl Extractor for Ytdlp {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
    use serde::Deserialize;
    use tokio::process::Command;

    #[derive(Deserialize)]
    struct YtdlpOutput {
      formats: Vec<Format>,
    }

    #[derive(Deserialize)]
    struct Format {
      quality: Option<f32>,
      resolution: String,
      audio_ext: String,
      #[serde(default)]
      fragments: Vec<Fragment>,
      http_headers: HashMap<String, String>,
    }

    #[derive(Deserialize)]
    struct Fragment {
      url: String,
    }

    let url = format!("https://youtube.com/watch?v={video_id}");
    let output = Command::new("yt-dlp").arg("-j").arg(url).output().await?;
    let output =
      String::from_utf8(output.stdout).map_err(|_| Error::Extraction)?;
    let output: YtdlpOutput = serde_json::from_str(&output).map_err(|e| {
      println!("{:?}", e);
      Error::Extraction
    })?;
    let format = output
      .formats
      .into_iter()
      .filter(|format| format.fragments.len() == 1)
      .filter(|format| {
        format.resolution == "audio only" && format.audio_ext == "m4a"
      })
      .max_by_key(|format| (format.quality.unwrap_or(0.0) as i64))
      .ok_or_else(|| Error::Extraction)?;

    let url = format.fragments[0].url.clone();
    let headers = format.http_headers.into_iter().collect();

    Ok(Extraction { url, headers })
  }
}
