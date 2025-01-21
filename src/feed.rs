use axum::{
  body::{self, Bytes},
  extract::{Path, Query},
  headers::ContentType,
  http::Response,
  response::IntoResponse,
  TypedHeader,
};
use futures::future::select_ok;
use http_types::Url;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{header, StatusCode};

use crate::{
  harvestor, harvestor::Harvestor, piped::PipedInstance, Error, Result,
  INSTANCE_PUBLIC_URL,
};

pub async fn channel_podcast_xml(
  Path(channel_id): Path<String>,
  _piped: PipedInstance,
  req_headers: header::HeaderMap,
) -> Result<impl IntoResponse> {
  let user_agent = req_headers
    .get(header::USER_AGENT)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("unknown");

  eprintln!(
    "client requesting podcast: {} (user-agent: {})",
    channel_id, user_agent
  );

  let (podcast, _) =
    select_ok([harvestor::Ytdlp::new().harvest(&channel_id)]).await?;

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
  let channel_ref = extract_youtube_channel_ref(&req.url)?;
  let channel_id = find_youtube_channel_id(channel_ref).await?;
  let podcast_url = format!("{INSTANCE_PUBLIC_URL}/channel/{channel_id}");
  let content_type = TypedHeader(ContentType::text());

  Ok((content_type, podcast_url))
}

async fn find_youtube_channel_id(channel_ref: ChannelRef) -> Result<String> {
  let url = match channel_ref {
    ChannelRef::Name(name) => {
      format!("https://www.youtube.com/c/{name}")
    }
    ChannelRef::Handle(handle) => {
      format!("https://www.youtube.com/@{handle}")
    }
    ChannelRef::Id(id) => return Ok(id),
  };

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

enum ChannelRef {
  Id(String),
  // https://www.youtube.com/c/SciShow
  Name(String),
  // https://www.youtube.com/@ComplexityExplorer
  Handle(String),
}

fn extract_youtube_channel_ref(url: &str) -> Result<ChannelRef> {
  static ID_REGEX: Lazy<Regex> =
    Lazy::new(|| regex::Regex::new(r"^channel/([a-zA-Z0-9_]+)").unwrap());
  static NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| regex::Regex::new(r"^c/([a-zA-Z0-9_]+)").unwrap());
  static HANDLE_REGEX: Lazy<Regex> =
    Lazy::new(|| regex::Regex::new(r"^@([a-zA-Z0-9_]+)$").unwrap());

  let url: Url = url.parse()?;

  match url.host_str() {
    Some("www.youtube.com") | Some("m.youtube.com") => (),
    _ => return Err(Error::UnsupportedURL(url.into(), "not youtube domain")),
  }

  let path = url.path().strip_prefix('/').unwrap_or("");

  if let Some(captures) = ID_REGEX.captures(path) {
    let id = captures.get(1).unwrap().as_str().to_string();
    return Ok(ChannelRef::Id(id));
  }

  if let Some(captures) = NAME_REGEX.captures(path) {
    let name = captures.get(1).unwrap().as_str().to_string();
    return Ok(ChannelRef::Name(name));
  }

  if let Some(captures) = HANDLE_REGEX.captures(path) {
    let handle = captures.get(1).unwrap().as_str().to_string();
    return Ok(ChannelRef::Handle(handle));
  }

  Err(Error::UnsupportedURL(url.into(), "invalid youtube url"))
}
