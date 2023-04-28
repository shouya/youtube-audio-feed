use crate::INSTANCE_PUBLIC_URL;
use atom_syndication::{extension::Extension, Entry};
use http_types::Url;
use serde_query::{DeserializeQuery, Query};

use crate::{
  podcast::{AudioInfo, Thumbnail},
  Error, Result, PIPED_INSTANCE, W,
};

impl W<&Entry> {
  fn media_group_children(&self, name: &str) -> Result<&Vec<Extension>> {
    let children = self
      .0
      .extensions
      .get("media")
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "not media extension",
      ))?
      .get("group")
      .ok_or(Error::InvalidFeedEntry(self.0.clone(), "not media group"))?
      .first()
      .expect("unreachable")
      .children
      .get(name)
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "no media group children",
      ))?;

    Ok(children)
  }

  fn media_group_child(&self, name: &str) -> Result<&Extension> {
    self
      .media_group_children(name)?
      .first()
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "no media group children",
      ))
  }

  pub fn description(&self) -> Result<String> {
    self
      .media_group_child("description")?
      .value
      .as_ref()
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "invalid description attribute",
      ))
      .map(|x| x.to_string())
  }

  pub fn thumbnail(&self) -> Result<Thumbnail> {
    let attrs = &self.media_group_child("thumbnail")?.attrs;
    let thumbnail = Thumbnail {
      url: attrs["url"].clone(),
      width: attrs["width"].parse().unwrap_or_default(),
      height: attrs["height"].parse().unwrap_or_default(),
    };

    Ok(thumbnail)
  }

  pub fn link(&self) -> Result<String> {
    self
      .0
      .links
      .first()
      .cloned()
      .map(|x| x.href)
      .ok_or(Error::InvalidFeedEntry(self.0.clone(), "no link found"))
  }

  pub fn video_id(&self) -> Result<String> {
    self
      .0
      .extensions
      .get("yt")
      .ok_or(Error::InvalidFeedEntry(self.0.clone(), "not yt extension"))?
      .get("videoId")
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "no yt:videoId found",
      ))?
      .first()
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "no yt:videoId value",
      ))?
      .value()
      .ok_or(Error::InvalidFeedEntry(
        self.0.clone(),
        "no yt:videoId value",
      ))
      .map(|x| x.to_string())
  }

  pub fn audio_info(&self) -> Result<AudioInfo> {
    let url = translate_video_to_audio_url(&self.link()?)?;
    let mime_type = "audio/mpeg".to_string();
    Ok(AudioInfo { url, mime_type })
  }

  pub async fn piped_audio_info(&self) -> Result<AudioInfo> {
    #[derive(Debug, DeserializeQuery)]
    struct PipedStreamResp {
      #[query(".audioStreams.[0].url")]
      url: String,
      #[query(".audioStreams.[0].mimeType")]
      mime_type: String,
    }

    let video_id = self.video_id()?;
    let piped_url = format!("{PIPED_INSTANCE}/streams/{video_id}");
    let resp: PipedStreamResp = reqwest::Client::new()
      .get(piped_url)
      .header("User-Agent", "Mozilla/5.0")
      .send()
      .await?
      .json::<Query<PipedStreamResp>>()
      .await?
      .into();

    Ok(AudioInfo {
      url: resp.url,
      mime_type: resp.mime_type,
    })
  }
}

fn translate_video_to_audio_url(uri_str: &str) -> Result<String> {
  let uri: Url = uri_str.parse()?;
  let video_id = uri
    .query_pairs()
    .find_map(|(k, v)| (k == "v").then_some(v))
    .ok_or_else(|| {
      Error::UnsupportedURL(uri_str.into(), "v parameter not found")
    })?;

  Ok(format!("{INSTANCE_PUBLIC_URL}/audio/{video_id}"))
}
