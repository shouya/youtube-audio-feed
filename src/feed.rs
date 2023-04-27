use std::io::Cursor;

use atom_syndication::{Entry, Feed};
use axum::{
  body::{self, Bytes},
  extract::{Path, Query},
  headers::ContentType,
  http::Response,
  response::IntoResponse,
  TypedHeader,
};
use http_types::Url;
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{header, StatusCode};
use rss::{
  extension::itunes::{
    ITunesCategory, ITunesChannelExtensionBuilder, ITunesItemExtensionBuilder,
  },
  Channel, EnclosureBuilder,
};
use serde::Deserialize;

use crate::{
  Error, Result, GENERATOR_STR, INSTANCE_PUBLIC_URL, PIPED_INSTANCE,
};

pub async fn channel_podcast_xml(
  Path(channel_id): Path<String>,
) -> Result<impl IntoResponse> {
  let (feed, extra_info, piped_channel) = tokio::try_join!(
    get_feed(&channel_id),
    get_extra_info(&channel_id),
    get_piped_channel(&channel_id)
  )?;

  dbg!(piped_channel);

  let podcast_channel = convert_feed(feed, extra_info)?;
  let mut output = Vec::new();
  podcast_channel.pretty_write_to(&mut output, b' ', 2)?;

  let resp = Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, "application/rss+xml; charset=UTF-8")
    .body(body::Full::new(Bytes::from(output)))?;

  Ok(resp)
}

fn convert_feed(feed: Feed, extra: ExtraInfo) -> Result<Channel> {
  let mut channel = Channel::default();

  channel.set_title(&*feed.title);
  channel.set_description(&*feed.title);

  channel.set_last_build_date(Some(feed.updated.to_rfc2822()));
  channel.set_language(feed.lang);
  channel.set_generator(Some(GENERATOR_STR.into()));

  let mut itunes = ITunesChannelExtensionBuilder::default();

  itunes
    .author(Some(feed.authors.iter().map(|x| &x.name).join(", ")))
    .image(Some(extra.logo_url))
    .categories(
      extra
        .tags
        .into_iter()
        .map(|tag| ITunesCategory {
          text: tag,
          subcategory: None,
        })
        .collect::<Vec<_>>(),
    );

  channel.set_itunes_ext(itunes.build());

  let items = feed
    .entries
    .into_iter()
    .map(map_entry)
    .collect::<Result<Vec<_>>>()?;

  channel.set_items(items);

  Ok(channel)
}

fn map_entry(entry: Entry) -> Result<rss::Item> {
  // used for reporting error
  let media_group = &entry
    .extensions
    .get("media")
    .ok_or(Error::InvalidFeedEntry(
      entry.clone(),
      "not media extension",
    ))?
    .get("group")
    .ok_or(Error::InvalidFeedEntry(entry.clone(), "not media group"))?
    .first()
    .expect("unreachable")
    .children;

  let media_description = &media_group
    .get("description")
    .ok_or(Error::InvalidFeedEntry(
      entry.clone(),
      "no media:description found",
    ))?
    .first()
    .expect("unreachable")
    .value
    .as_ref()
    .ok_or(Error::InvalidFeedEntry(
      entry.clone(),
      "no media:description value",
    ))?;

  let media_thumbnail = &media_group
    .get("thumbnail")
    .ok_or(Error::InvalidFeedEntry(
      entry.clone(),
      "no media:thumbnail found",
    ))?
    .first()
    .expect("unreachable")
    .attrs;

  let video_url = entry
    .links
    .first()
    .cloned()
    .map(|x| x.href)
    .ok_or(Error::InvalidFeedEntry(entry.clone(), "no link found"))?;
  let audio_url = translate_video_to_audio_url(video_url.as_ref())?;

  let description_html = format!(
    "<img loading=\"lazy\" class=\"size-thumbnail\"\
          src=\"{}\" width=\"{}\" height=\"{}\"/>\n\
     <p>{}</p>\n\
     <p><a href=\"{}\">{}</a></p>\n",
    media_thumbnail["url"],
    media_thumbnail["width"],
    media_thumbnail["height"],
    media_description,
    video_url,
    video_url,
  );

  let enclosure = EnclosureBuilder::default()
    .url(audio_url)
    .mime_type("audio/mpeg".to_owned())
    .build();

  let itunes = ITunesItemExtensionBuilder::default()
    .summary(Some((*media_description).clone()))
    .author(entry.authors.first().map(|x| x.name.clone()))
    .image(Some(media_thumbnail["url"].clone()))
    .build();

  let item = rss::Item {
    title: Some(entry.title.as_str().to_owned()),
    link: Some(video_url),
    pub_date: Some(entry.updated.to_rfc2822()),
    guid: Some(rss::Guid {
      value: entry.id.clone(),
      permalink: true,
    }),
    description: Some(description_html),
    itunes_ext: Some(itunes),
    enclosure: Some(enclosure),
    ..Default::default()
  };

  Ok(item)
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

async fn get_feed(channel_id: &str) -> Result<Feed> {
  let feed = reqwest::get(format!(
    "https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}"
  ))
  .await?;

  let body = feed.text().await?;
  Ok(Feed::read_from(Cursor::new(body))?)
}

async fn get_extra_info(channel_id: &str) -> Result<ExtraInfo> {
  let url = format!("https://www.youtube.com/channel/{channel_id}");
  let resp = reqwest::get(url).await?;

  let resp_body = resp.text().await?;
  let dom = tl::parse(&resp_body, tl::ParserOptions::default())?;

  let logo_url = get_logo(&dom)?;
  let tags = get_tags(&dom)?;

  Ok(ExtraInfo { logo_url, tags })
}

fn get_logo(dom: &tl::VDom<'_>) -> Result<String> {
  let thumbnail_node = dom
    .query_selector("link[rel=image_src][href]")
    .expect("selector is hard-coded, thus must be valid")
    .next()
    .ok_or(Error::InvalidHTML("link[rel=image_src]"))?;

  let thumbnail_url = thumbnail_node
    .get(dom.parser())
    .expect("queried node must be within dom")
    .as_tag()
    .ok_or(Error::InvalidHTML("thumbnail"))?
    .attributes()
    .get("href")
    .expect("href must exists")
    .ok_or(Error::InvalidHTML("link[href]"))?
    .as_utf8_str();

  let large_thumbnail_url =
    GOOGLE_IMG_WIDTH_REGEX.replace(&thumbnail_url, "=s1400");

  Ok(large_thumbnail_url.to_string())
}

static GOOGLE_IMG_WIDTH_REGEX: Lazy<Regex> =
  Lazy::new(|| Regex::new("=s\\d+").unwrap());

fn get_tags(dom: &tl::VDom<'_>) -> Result<Vec<String>> {
  let mut tags = Vec::new();
  let node_iter = dom
    // a clumpsy and inaccurate way to query for
    // [property=og:video:tag] because tl doesn't support it.
    .query_selector(
      "meta[property^=og][property*=video][property$=tag][content]",
    )
    .expect("selector should be valid");

  for node in node_iter {
    let tag = node
      .get(dom.parser())
      .expect("queried node must be within dom")
      .as_tag()
      .ok_or(Error::InvalidHTML("meta[property=og:video:tag]"))?
      .attributes()
      .get("content")
      .expect("content must exists")
      .ok_or(Error::InvalidHTML("meta[property=og:video:tag] content"))?
      .as_utf8_str()
      .into_owned();

    tags.push(tag);
  }

  Ok(tags)
}

struct ExtraInfo {
  logo_url: String,
  tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipedChannel {
  related_streams: Vec<PipedStream>,
}
#[derive(Debug, Deserialize)]
struct PipedStream {
  #[serde(rename = "url")]
  video_id: String,
  duration: u64,
}

async fn get_piped_channel(channel_id: &str) -> Result<PipedChannel> {
  let url = format!("{PIPED_INSTANCE}/channel/{channel_id}");
  let mut channel = reqwest::Client::new()
    .get(&url)
    .header("User-Agent", "Mozilla/5.0")
    .send()
    .await?
    .json::<PipedChannel>()
    .await?;

  for stream in &mut channel.related_streams {
    stream.video_id = stream
      .video_id
      .strip_prefix("/watch?v=")
      .unwrap()
      .to_string();
  }

  Ok(channel)
}

#[derive(serde::Deserialize)]
pub struct GetPodcastReq {
  url: String,
}

pub async fn channel_podcast_url(
  Query(req): Query<GetPodcastReq>,
) -> Result<impl IntoResponse> {
  let channel_id = match extract_youtube_channel_name(&req.url)? {
    ChannelIdentifier::Id(id) => id,
    ChannelIdentifier::Name(name) => find_youtube_channel_id(&name).await?,
  };
  let podcast_url = format!("{INSTANCE_PUBLIC_URL}/channel/{channel_id}");
  let content_type = TypedHeader(ContentType::text());

  Ok((content_type, podcast_url))
}

async fn find_youtube_channel_id(channel_name: &str) -> Result<String> {
  let url = format!("https://www.youtube.com/c/{channel_name}");
  let resp = reqwest::get(url).await?;

  let resp_body = resp.text().await?;
  let dom = tl::parse(&resp_body, tl::ParserOptions::default())?;

  let link_node = dom
    .query_selector("link[rel=canonical][href]")
    .expect("selector is hard-coded, thus must be valid")
    .next()
    .ok_or(Error::InvalidHTML("link[rel=canonical]"))?;

  let link_url = link_node
    .get(dom.parser())
    .expect("queried node must be within dom")
    .as_tag()
    .ok_or(Error::InvalidHTML("link[rel=canonical]"))?
    .attributes()
    .get("href")
    .expect("href must exists")
    .ok_or(Error::InvalidHTML("link[rel=canonical]"))?
    .as_utf8_str();

  let channel_id = link_url
    .strip_prefix("https://www.youtube.com/channel/")
    .ok_or(Error::InvalidHTML("link[rel=canonical] url prefix"))?;

  Ok(channel_id.to_string())
}

enum ChannelIdentifier {
  Id(String),
  Name(String),
}

fn extract_youtube_channel_name(url: &str) -> Result<ChannelIdentifier> {
  let url: Url = url.parse()?;

  match url.host_str() {
    Some("www.youtube.com") | Some("m.youtube.com") => (),
    _ => return Err(Error::UnsupportedURL(url.into(), "not youtube")),
  }

  if let Some(segments) = url.path_segments() {
    let segs: Vec<_> = segments.take(3).collect();
    if segs.len() < 2 {
      return Err(Error::UnsupportedURL(url.into(), "video path not found"));
    }

    let id = match segs[0] {
      "c" => ChannelIdentifier::Name(segs[1].to_string()),
      "channel" => ChannelIdentifier::Id(segs[1].to_string()),
      _ => {
        return Err(Error::UnsupportedURL(url.into(), "video path not found"))
      }
    };

    return Ok(id);
  }

  Err(Error::UnsupportedURL(url.into(), "invalid youtube url"))
}
