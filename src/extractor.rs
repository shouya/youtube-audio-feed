use std::collections::HashMap;

use crate::error::Error;
use crate::piped::PipedInstance;
use async_trait::async_trait;

#[derive(Default, Debug)]
pub struct Extraction {
  pub url: String,
  pub headers: Vec<(String, String)>,
}

#[async_trait]
pub trait Extractor {
  async fn extract(&self, video_id: &str) -> Result<Extraction, Error>;
}

pub struct Piped<'a>(pub &'a PipedInstance);

#[async_trait]
impl<'a> Extractor for Piped<'a> {
  async fn extract(&self, video_id: &str) -> Result<Extraction, Error> {
    use serde_query::{DeserializeQuery, Query};

    #[derive(DeserializeQuery)]
    struct PipedStreamResp {
      #[query(".audioStreams.[0].url")]
      url: String,
    }

    let piped_url = self.0.stream_url(video_id);
    let resp: PipedStreamResp = reqwest::Client::new()
      .get(piped_url)
      .header("User-Agent", "Mozilla/5.0")
      .send()
      .await?
      .json::<Query<PipedStreamResp>>()
      .await?
      .into();

    Ok(Extraction {
      url: resp.url,
      ..Default::default()
    })
  }
}

pub struct Rustube;

#[async_trait]
impl Extractor for Rustube {
  async fn extract(&self, video_id: &str) -> Result<Extraction, Error> {
    use rustube::{Id, VideoFetcher};

    let id = Id::from_str(video_id)?.as_owned();
    let video = VideoFetcher::from_id(id)?.fetch().await?.descramble()?;
    let stream = video
      .best_audio()
      .ok_or_else(|| Error::AudioStream(video_id.into()))?;
    let url = stream.signature_cipher.url.to_string();
    let url = format!("{}&range=0-999999999999", url);

    Ok(Extraction {
      url,
      ..Default::default()
    })
  }
}

// run yt-dlp command line to get video playback url.
// requires yt-dlp executable to be in PATH.
pub struct Ytdlp;

#[async_trait]
impl Extractor for Ytdlp {
  async fn extract(&self, video_id: &str) -> Result<Extraction, Error> {
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
