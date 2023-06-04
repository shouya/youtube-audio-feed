use crate::error::Error;
use crate::piped::PipedInstance;
use async_trait::async_trait;

#[derive(Default, Debug)]
pub struct Extraction {
  pub url: String,
  pub headers: Vec<(String, String)>,
}

#[async_trait]
pub trait YoutubeAudioExtractor {
  async fn extract(&self, video_id: &str) -> Result<Extraction, Error>;
}

pub struct Piped<'a>(pub &'a PipedInstance);

#[async_trait]
impl<'a> YoutubeAudioExtractor for Piped<'a> {
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
impl YoutubeAudioExtractor for Rustube {
  async fn extract(&self, video_id: &str) -> Result<Extraction, Error> {
    use rustube::{Id, Video};

    let id = Id::from_str(video_id)?.as_owned();
    let video = Video::from_id(id).await?;
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
