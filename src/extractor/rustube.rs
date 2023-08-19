use async_trait::async_trait;

use crate::{Error, Result};

use super::{Extraction, Extractor};

pub struct Rustube;

#[async_trait]
impl Extractor for Rustube {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
    use rustube::{Id, VideoFetcher};

    let id = Id::from_str(video_id)?.as_owned();
    let video = VideoFetcher::from_id(id)?.fetch().await?.descramble()?;
    let stream = video
      .best_audio()
      .ok_or_else(|| Error::AudioStream(video_id.into()))?;
    let url = stream.signature_cipher.url.to_string();
    let url = format!("{}&range=0-999999999999", url);

    Ok(Extraction::Proxy {
      url,
      headers: vec![],
    })
  }
}
