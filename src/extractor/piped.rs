use async_trait::async_trait;

use crate::{piped::PipedInstance, Result};

use super::{Extraction, Extractor};

pub struct Piped<'a>(pub &'a PipedInstance);

#[async_trait]
impl Extractor for Piped<'_> {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
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

    Ok(Extraction::Proxy {
      url: resp.url,
      headers: vec![],
    })
  }
}
