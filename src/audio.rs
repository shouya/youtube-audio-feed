use anyhow::{Context, Result};
use axum::{
  body::{self, HttpBody},
  extract::Path,
  http::Response,
  response::IntoResponse,
};
use reqwest::header;

use crate::{Error, INVIDIOUS_INSTANCE};

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
) -> Result<impl IntoResponse, Error> {
  let download = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()?
    .post(format!("{INVIDIOUS_INSTANCE}/download"))
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
        format!("{INVIDIOUS_INSTANCE}{target_path}"),
      )
      .body(body::Empty::new().boxed())?;

    return Ok(resp);
  }

  let resp = Response::builder()
    .status(download.status())
    .header(header::CONTENT_TYPE, "application/rss+xml; charset=UTF-8")
    .body(download.text().await?.boxed())?;

  Ok(resp)
}
