use std::io::{BufReader, Cursor};

use anyhow::{Context, Result};

use atom_syndication::{Entry, Feed};
use rss::{
  extension::itunes::{
    ITunesChannelExtensionBuilder, ITunesItemExtensionBuilder,
  },
  Channel, EnclosureBuilder,
};
use tide::{Request, Response};

const GENERATOR_STR: &str = "youtube_audio_feed";

#[async_std::main]
async fn main() -> tide::Result<()> {
  tide::log::start();

  let mut app = tide::new();

  app.at("/channel/:channel_id").get(channel_podcast_xml);
  app.at("/audio/:video_id").get(get_audio);
  app.with(tide::log::LogMiddleware::new());

  app.listen("127.0.0.1:8080").await?;

  Ok(())
}

async fn channel_podcast_xml(req: Request<()>) -> tide::Result {
  let channel_id = req.param("channel_id")?;
  let feed = reqwest::get(format!(
    "https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}"
  ))
  .await?;

  let body = feed.text().await?;
  let youtube_feed = Feed::read_from(Cursor::new(body))?;

  let podcast_channel = map_feed(youtube_feed)?;
  let mut output = Vec::new();
  podcast_channel.pretty_write_to(&mut output, b' ', 2)?;

  let resp = Response::builder(200)
    .header("Content-Type", "application/rss+xml; charset=UTF-8")
    .body(output);

  Ok(resp.build())
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

  let media_content = &media_group
    .get("content")
    .with_context(|| "no media:content found")?
    .first()
    .with_context(|| "unreachable")?
    .attrs;

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

async fn get_audio(req: Request<()>) -> tide::Result {
  let video_id = req.param("video_id")?;

  let download = reqwest::Client::new()
    .post("https://invidious.namazso.eu/download")
    .form(&[
      ("id", video_id),
      ("title", "unknown"),
      ("dowload_widget", "{\"itag\":140,\"ext\":\"mp4\"}"),
    ])
    .send()
    .await?;

  if download.status().is_redirection() {
    let resp = Response::builder(download.status().as_u16())
      .header("Content-Type", "application/rss+xml; charset=UTF-8")
      .header("Location", "/foo")
      .build();

    return Ok(resp);
  }

  let resp = Response::builder(500)
    .header("Content-Type", "application/rss+xml; charset=UTF-8")
    .build();
  Ok(resp)
}
