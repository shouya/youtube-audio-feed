use axum::{
  body::{self, Bytes},
  extract::{Path, Query},
  headers::ContentType,
  http::Response,
  response::IntoResponse,
  TypedHeader,
};
use http_types::Url;
use reqwest::{header, StatusCode};

use crate::{
  harvestor, harvestor::Harvestor, piped::PipedInstance, Error, Result,
  INSTANCE_PUBLIC_URL,
};

pub async fn channel_podcast_xml(
  Path(channel_id): Path<String>,
  piped: PipedInstance,
) -> Result<impl IntoResponse> {
  let podcast = harvestor::RssPiped::new(piped).harvest(&channel_id).await?;
  let podcast_channel: rss::Channel = podcast.into();

  let mut output = Vec::new();
  podcast_channel.pretty_write_to(&mut output, b' ', 2)?;

  let resp = Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, "application/rss+xml; charset=UTF-8")
    .body(body::Full::new(Bytes::from(output)))?;

  Ok(resp)
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
