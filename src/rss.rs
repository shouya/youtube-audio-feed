use std::io::Cursor;

use atom_syndication::{Entry, Feed};
use itertools::Itertools;

use crate::{podcast::Thumbnail, Result, W};

pub struct RssChannel {
  pub title: String,
  pub updated: String,
  pub description: String,
  pub language: String,
  pub author: String,
  pub episodes: Vec<RssEpisode>,
}

pub struct RssEpisode {
  pub video_id: String,
  pub title: String,
  pub description: String,
  pub thumbnail: Thumbnail,
  pub author: String,
}

impl RssEpisode {
  fn from_entry(entry: Entry) -> Result<Self> {
    let description = W(&entry).description()?;
    let thumbnail = W(&entry).thumbnail()?;
    let video_id = W(&entry).video_id()?;
    let author = entry
      .authors
      .first()
      .map(|x| x.name.clone())
      .unwrap_or_default();
    let title = entry.title.to_string();

    Ok(RssEpisode {
      title,
      description,
      thumbnail,
      video_id,
      author,
    })
  }
}

impl RssChannel {
  fn from_feed(feed: Feed) -> Result<Self> {
    let title = feed.title.to_string();
    let updated = feed.updated.to_rfc2822();
    let description = feed.subtitle.unwrap_or_default().to_string();
    let language = feed.lang.unwrap_or_default();
    let author = feed.authors.iter().map(|x| &x.name).join(", ").to_string();

    let episodes = feed
      .entries
      .into_iter()
      .map(RssEpisode::from_entry)
      .collect::<Result<Vec<_>>>()?;

    Ok(RssChannel {
      title,
      updated,
      description,
      language,
      author,
      episodes,
    })
  }

  async fn fetch_feed(channel_id: &str) -> Result<Feed> {
    let feed = reqwest::get(format!(
      "https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}"
    ))
    .await?;

    let body = feed.text().await?;
    Ok(Feed::read_from(Cursor::new(body))?)
  }

  pub async fn fetch(channel_id: &str) -> Result<Self> {
    let feed = Self::fetch_feed(channel_id).await?;
    Self::from_feed(feed)
  }
}
