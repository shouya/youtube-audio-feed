use std::io::Cursor;

use anyhow::{Context, Result};

use atom_syndication::{Entry, Feed};
use axum::{
  body::{self, Bytes, HttpBody},
  extract::Path,
  http::status::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
  Router,
};

use reqwest::{header, redirect::Policy};
use rss::{
  extension::itunes::{
    ITunesChannelExtensionBuilder, ITunesItemExtensionBuilder,
  },
  Channel, EnclosureBuilder,
};

const GENERATOR_STR: &str = "youtube_audio_feed";

enum Error {
  Server(anyhow::Error),
  #[allow(unused)]
  Client(anyhow::Error),
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    match self {
      Error::Server(err) => {
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
      }
      Error::Client(err) => {
        (StatusCode::BAD_REQUEST, err.to_string()).into_response()
      }
    }
  }
}

impl<E> From<E> for Error
where
  E: Into<anyhow::Error>,
{
  fn from(err: E) -> Self {
    Error::Server(err.into())
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let app = Router::new()
    .route("/channel/:channel_id", get(channel_podcast_xml))
    .route("/audio/:video_id", get(get_audio));

  axum::Server::bind(&"127.0.0.1:8080".parse().unwrap())
    .serve(app.into_make_service())
    .await
    .map_err(|e| e.into())
}

async fn channel_podcast_xml(
  Path(channel_id): Path<String>,
) -> Result<impl IntoResponse, Error> {
  let feed = reqwest::get(format!(
    "https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}"
  ))
  .await?;

  let body = feed.text().await?;
  let youtube_feed = Feed::read_from(Cursor::new(body))?;

  let podcast_channel = map_feed(youtube_feed)?;
  let mut output = Vec::new();
  podcast_channel.pretty_write_to(&mut output, b' ', 2)?;

  let resp = Response::builder()
    .status(StatusCode::INTERNAL_SERVER_ERROR)
    .header("Content-Type", "application/rss+xml; charset=UTF-8")
    .body(body::Full::new(Bytes::from(output)))?;

  Ok(resp)
}

#[axum::debug_handler]
async fn get_audio(
  Path(video_id): Path<String>,
) -> Result<impl IntoResponse, Error> {
  let download = reqwest::Client::builder()
    .redirect(Policy::none())
    .build()?
    .post("https://invidious.namazso.eu/download")
    .form(&[
      ("id", video_id.as_str()),
      ("title", "foobar"),
      ("download_widget", "{\"itag\":140,\"ext\":\"mp4\"}"),
    ])
    .send()
    .await?;

  if download.status().is_redirection() {
    let target_path = download
      .headers()
      .get(header::LOCATION)
      .with_context(|| "Target location not found")?
      .to_str()?;

    let resp = Response::builder()
      .status(301)
      .header(header::CONTENT_TYPE, "application/rss+xml; charset=UTF-8")
      .header(
        header::LOCATION,
        format!("{}{}", "https://invidious.namazso.eu", target_path),
      )
      .body(body::Empty::new().boxed())?;

    return Ok(resp);
  }

  let resp = Response::builder()
    .status(500)
    .header("Content-Type", "application/rss+xml; charset=UTF-8")
    .body(format!("{:#?}", download).boxed())?;

  Ok(resp)
}

fn map_feed(feed: Feed) -> anyhow::Result<Channel> {
  let mut channel = Channel::default();

  channel.set_title(&*feed.title);
  channel.set_description(&*feed.title);

  channel.set_last_build_date(Some(feed.updated.to_rfc2822()));
  channel.set_language(feed.lang);
  channel.set_generator(Some(GENERATOR_STR.into()));

  let mut itunes = ITunesChannelExtensionBuilder::default();

  itunes.author(Some(
    feed
      .authors
      .iter()
      .map(|x| x.name.as_ref())
      .collect::<Vec<_>>()
      .join(", "),
  ));

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
  let media_group = &entry
    .extensions
    .get("media")
    .with_context(|| "no media extension found")?
    .get("group")
    .with_context(|| "no media:group extension found")?
    .first()
    .with_context(|| "unreachable")?
    .children;

  let media_description = &media_group
    .get("description")
    .with_context(|| "no media:description found")?
    .first()
    .with_context(|| "unreachable")?
    .value
    .as_ref()
    .with_context(|| "media:description empty")?;

  let media_thumbnail = &media_group
    .get("thumbnail")
    .with_context(|| "no media:thumbnail found")?
    .first()
    .with_context(|| "unreachable")?
    .attrs;

  let video_url = entry
    .links
    .first()
    .cloned()
    .map(|x| x.href)
    .with_context(|| "video url not found")?;
  let audio_url = translate_video_to_audio_url(video_url.as_ref());

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

fn translate_video_to_audio_url(url: &str) -> String {
  url.into()
}
