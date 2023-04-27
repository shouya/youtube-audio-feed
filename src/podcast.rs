use crate::GENERATOR_STR;

#[derive(Debug, Default)]
pub struct Podcast {
  pub title: String,
  pub description: String,
  pub last_build_date: String,
  pub language: String,
  pub author: String,
  pub logo_url: String,
  pub categories: Vec<String>,
  pub channel_url: String,
  pub episodes: Vec<Episode>,
}

impl From<Podcast> for rss::Channel {
  fn from(podcast: Podcast) -> Self {
    let itunes_categoris = podcast.categories.into_iter().map(|c| {
      rss::extension::itunes::ITunesCategoryBuilder::default()
        .text(c)
        .build()
    });

    let itunes_ext =
      rss::extension::itunes::ITunesChannelExtensionBuilder::default()
        .author(Some(podcast.author))
        .image(Some(podcast.logo_url))
        .categories(itunes_categoris.collect::<Vec<_>>())
        .build();

    let mut channel = rss::ChannelBuilder::default()
      .title(podcast.title)
      .description(podcast.description)
      .link(podcast.channel_url)
      .last_build_date(Some(podcast.last_build_date))
      .language((!podcast.language.is_empty()).then_some(podcast.language))
      .itunes_ext(Some(itunes_ext))
      .generator(Some(GENERATOR_STR.to_owned()))
      .build();

    for episode in podcast.episodes {
      channel.items.push(episode.into());
    }

    channel
  }
}

#[derive(Debug, Default)]
pub struct Thumbnail {
  pub url: String,
  pub width: u32,
  pub height: u32,
}

#[derive(Debug, Default)]
pub struct Episode {
  pub title: String,
  pub link: String,
  pub description: String,
  pub pub_date: String,
  pub author: String,
  pub guid: String,
  pub duration: u64,
  pub thumbnail: Thumbnail,
  pub audio_url: String,
}

impl From<Episode> for rss::Item {
  fn from(episode: Episode) -> Self {
    // rewrite above map_entry function with From trait
    let description_html = format!(
      "<img loading=\"lazy\" class=\"size-thumbnail\"\
            src=\"{}\" width=\"{}\" height=\"{}\"/>\n\
       <p>{}</p>\n\
       <p><a href=\"{}\">{}</a></p>\n",
      episode.thumbnail.url,
      episode.thumbnail.width,
      episode.thumbnail.height,
      episode.description,
      episode.link,
      episode.link,
    );

    let enclosure = rss::EnclosureBuilder::default()
      .url(episode.audio_url)
      .mime_type("audio/mpeg".to_owned())
      .build();

    let mut itunes =
      rss::extension::itunes::ITunesItemExtensionBuilder::default()
        .summary(Some(episode.description))
        .author(Some(episode.author))
        .image(Some(episode.thumbnail.url))
        .duration(
          (episode.duration > 0).then(|| seconds_to_duration(episode.duration)),
        )
        .build();

    rss::Item {
      title: Some(episode.title),
      link: Some(episode.link),
      pub_date: Some(episode.pub_date),
      guid: Some(rss::Guid {
        value: episode.guid,
        permalink: true,
      }),
      description: Some(description_html),
      itunes_ext: Some(itunes),
      enclosure: Some(enclosure),
      ..Default::default()
    }
  }
}

fn seconds_to_duration(secs: u64) -> String {
  let hours = secs / 3600;
  let minutes = (secs % 3600) / 60;
  let seconds = secs % 60;

  if hours > 0 {
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
  } else {
    format!("{:02}:{:02}", minutes, seconds)
  }
}
