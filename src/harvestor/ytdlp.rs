use std::sync::{Arc, LazyLock, RwLock};

use crate::{
  podcast::{AudioInfo, Episode, Podcast},
  Result, INSTANCE_PUBLIC_URL,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::process::Command;

use super::Harvestor;

// run yt-dlp command line to get audio stream directly.
// requires yt-dlp executable to be in PATH.
pub struct Ytdlp;

impl Ytdlp {
  pub fn new() -> Self {
    Self
  }
}

#[derive(Debug, serde::Deserialize)]
struct Channel {
  channel: String,
  channel_url: String,
  description: Option<String>,
  tags: Vec<String>,
  entries: Vec<Option<Entry>>,
  thumbnails: Vec<Thumbnail>,
  uploader_id: String,
}
impl Channel {
  fn squarest_thumbnail(&self) -> Option<Thumbnail> {
    self
      .thumbnails
      .iter()
      .filter(|t| t.width > 0 && t.height > 0)
      // first by aspect ratio, then by largest width
      .min_by_key(|t| ((t.width - t.height).abs(), -t.width))
      .cloned()
  }

  fn last_episode_date(&self) -> String {
    self
      .entries
      .iter()
      .filter_map(|e| e.as_ref())
      .map(|ep| GLOBAL_EPISODE_DATE_REGISTRY.get(&ep.id))
      .max()
      .unwrap_or_default()
      .to_rfc2822()
  }
}

#[derive(Debug, serde::Deserialize)]
struct Entry {
  id: String,
  title: String,
  url: String,
  description: Option<String>,
  thumbnails: Vec<Thumbnail>,
  duration: f32,
}

#[derive(Debug, serde::Deserialize, Clone, Default)]
struct Thumbnail {
  url: String,
  #[serde(default)]
  width: i32,
  #[serde(default)]
  height: i32,
}

#[async_trait]
impl Harvestor for Ytdlp {
  async fn harvest(&self, channel_id: &str) -> Result<Podcast> {
    let url = format!("https://youtube.com/channel/{}/videos", channel_id);
    let mut cmd = Command::new("yt-dlp");
    cmd
      // don't fetch video pages
      .arg("--flat-playlist")
      // emit the output as a single json object instead of jsonl
      .arg("--dump-single-json")
      // only fetch the latest 20 videos
      .arg("--playlist-end")
      .arg("20")
      .arg(url);
    let stdout = cmd.output().await?.stdout;
    let channel: Channel = serde_json::from_slice(&stdout)?;

    Ok(channel.into())
  }
}

impl From<Channel> for Podcast {
  fn from(c: Channel) -> Self {
    let logo_url = c.squarest_thumbnail().map(|t| t.url).unwrap_or_default();
    let last_episode_date = c.last_episode_date();
    let episodes = c.entries.into_iter().flatten().map(Into::into).collect();

    Self {
      title: c.channel,
      description: c.description.unwrap_or_default(),
      last_build_date: last_episode_date,
      language: String::from("en"),
      author: c.uploader_id,
      categories: c.tags,
      channel_url: c.channel_url,
      episodes,
      logo_url,
    }
  }
}

impl From<Entry> for Episode {
  fn from(e: Entry) -> Self {
    let thumbnail = e
      .thumbnails
      .into_iter()
      .max_by_key(|t| t.width)
      .unwrap_or_default()
      .into();

    let audio_info = AudioInfo {
      url: format!("{}/audio/{}", &*INSTANCE_PUBLIC_URL, e.id),
      mime_type: "audio/mpeg".to_string(),
    };
    let pub_date = GLOBAL_EPISODE_DATE_REGISTRY.get(&e.id).to_rfc2822();

    Self {
      title: e.title,
      link: e.url,
      description: e.description.unwrap_or_default(),
      author: "".to_string(),
      duration: e.duration as u64,
      guid: e.id,
      thumbnail,
      pub_date,
      audio_info,
    }
  }
}

impl From<Thumbnail> for crate::podcast::Thumbnail {
  fn from(t: Thumbnail) -> Self {
    Self {
      url: t.url,
      width: t.width as u32,
      height: t.height as u32,
    }
  }
}

// ytdlp's `--flat-list` option doesn't return the publish date of the
// videos. So we need a different way to keep track of the date of each
// video without much overhead as to fetch the video page
#[derive(Debug, Default, Clone)]
struct EpisodeDateRegistry {
  dict: Arc<RwLock<std::collections::HashMap<String, DateTime<Utc>>>>,
}

static GLOBAL_EPISODE_DATE_REGISTRY: LazyLock<EpisodeDateRegistry> =
  LazyLock::new(Default::default);

impl EpisodeDateRegistry {
  fn get(&self, id: &str) -> DateTime<Utc> {
    if let Some(date) = self.dict.read().unwrap().get(id).cloned() {
      return date;
    }

    let date = Utc::now();
    self.dict.write().unwrap().insert(id.to_string(), date);
    date
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_episode_date_registry() {
    let registry = EpisodeDateRegistry::default();
    let date = registry.get("test");
    assert_eq!(registry.get("test"), date);
  }

  #[tokio::test]
  async fn test_channel() {
    let harvestor = Ytdlp::new();
    let podcast = harvestor.harvest("UC1yNl2E66ZzKApQdRuTQ4tw").await;
    dbg!(&podcast);
    assert!(podcast.is_ok());

    let podcast = podcast.unwrap();
    assert_eq!(podcast.title, "Sabine Hossenfelder");
  }
}
