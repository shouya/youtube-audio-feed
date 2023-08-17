use std::str::FromStr;

use futures::{future::join, StreamExt};
use ytextract::{Channel, Client, Video};

use crate::{
  podcast::{AudioInfo, Episode, Podcast, Thumbnail},
  rss::RssChannel,
  Error, Result, INSTANCE_PUBLIC_URL,
};

use super::Harvestor;

pub struct RssYtextract {
  client: Client,
}

impl RssYtextract {
  pub fn new() -> Self {
    Self {
      client: Client::new(),
    }
  }

  pub async fn fetch(&self, channel_id: &str) -> Result<Channel> {
    let id = ytextract::channel::Id::from_str(channel_id)
      .map_err(|_| Error::InvalidId(channel_id.to_owned()))?;
    Ok(self.client.channel(id).await?)
  }

  pub async fn fetch_uploads(&self, channel: &Channel) -> Result<Vec<Video>> {
    let uploads: Vec<_> = channel
      .uploads()
      .await?
      .filter_map(|x| async move { x.ok() })
      .take(30)
      .map(|x| self.client.video(x.id()))
      .buffered(30)
      .collect()
      .await;

    uploads
      .into_iter()
      .map(|x| x.map_err(Error::from))
      .collect()
  }
}

#[async_trait::async_trait]
impl Harvestor for RssYtextract {
  async fn harvest(&self, channel_id: &str) -> Result<Podcast> {
    let (rss_channel, yt_channel) =
      join(RssChannel::fetch(channel_id), self.fetch(channel_id)).await;

    let rss_channel = rss_channel?;
    let yt_channel = yt_channel?;

    let yt_uploads = self.fetch_uploads(&yt_channel).await?;

    let podcast =
      make_podcast(channel_id, rss_channel, yt_channel, yt_uploads)?;
    Ok(podcast)
  }
}

fn make_episode(video: Video) -> Result<Episode> {
  let link = format!("https://www.youtube.com/watch?v={}", video.id());
  let audio_link = format!("{INSTANCE_PUBLIC_URL}/audio/{}", video.id());

  let date = video
    .date()
    .and_hms_opt(0, 0, 0)
    .expect("invalid date")
    .and_local_timezone(chrono::Utc)
    .single()
    .expect("invalid date")
    .to_rfc2822();

  let thumbnail = video
    .thumbnails()
    .iter()
    .max_by_key(|&x| x.width)
    .map(|x| Thumbnail {
      url: x.url.to_string(),
      width: x.width as u32,
      height: x.height as u32,
    })
    .unwrap_or_default();
  let audio_info = AudioInfo {
    url: audio_link,
    mime_type: "audio/mpeg".into(),
  };

  let episode = Episode {
    title: video.title().to_string(),
    link,
    description: video.description().to_string(),
    pub_date: date,
    author: "".into(),
    guid: video.id().to_string(),
    duration: video.duration().as_secs_f64() as u64,
    thumbnail,
    audio_info,
  };

  Ok(episode)
}

fn make_podcast(
  channel_id: &str,
  rss_channel: RssChannel,
  yt_channel: Channel,
  yt_videos: Vec<Video>,
) -> Result<Podcast> {
  let RssChannel {
    title,
    updated,
    description,
    language,
    author,
    ..
  } = rss_channel;

  let channel_url = format!("https://www.youtube.com/channel/{channel_id}");
  let logo_url = yt_channel
    .avatar()
    .max_by_key(|x| x.width)
    .map(|x| x.url.to_string())
    .unwrap_or_default();

  let episodes: Vec<_> = yt_videos
    .into_iter()
    .map(make_episode)
    .collect::<Result<_>>()?;

  let podcast = Podcast {
    title,
    description,
    last_build_date: updated,
    language,
    author,
    logo_url,
    categories: vec![],
    channel_url,
    episodes,
  };

  Ok(podcast)
}

#[cfg(test)]
mod test {
  use ytextract::Client;

  #[tokio::test]
  async fn test_harvest() {
    let client = Client::new();
    println!("{:?}", client.video("XpRyPxSLXsQ".parse().unwrap()).await);
  }
}
