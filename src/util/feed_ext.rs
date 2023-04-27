use atom_syndication::{extension::Extension, Entry};

use crate::{podcast::Thumbnail, Error, Result, W};

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
}
