use anyhow::{ensure, Context, Result};
use axum::body::StreamBody;
use axum::{
  body, extract::Path, headers::HeaderMap, http::Response,
  response::IntoResponse,
};
use reqwest::header;

use crate::{Error, INVIDIOUS_INSTANCE};

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, Error> {
  let playable_link = get_playable_link(&video_id).await?;
  proxy_play_link(&playable_link, headers).await
}

async fn proxy_play_link(
  url: &str,
  mut headers: HeaderMap,
) -> Result<impl IntoResponse, Error> {
  headers.remove(header::HOST);

  let resp = reqwest::Client::new()
    .get(url)
    .headers(headers)
    .send()
    .await?;

  Ok(StreamResponse(resp))
}

async fn get_playable_link(video_id: &str) -> Result<String> {
  let download = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()?
    .post(format!("{INVIDIOUS_INSTANCE}/download"))
    .form(&[
      ("id", video_id),
      ("title", "foobar"),
      ("download_widget", "{\"itag\":140,\"ext\":\"mp4\"}"),
    ])
    .send()
    .await?;

  ensure!(
    download.status().is_redirection(),
    "Invidious returns other than redirection"
  );

  let target_path = download
    .headers()
    .get(header::LOCATION)
    .with_context(|| "Target location not found")?
    .to_str()?;

  return Ok(format!("{INVIDIOUS_INSTANCE}{target_path}"));
}

struct StreamResponse(reqwest::Response);

impl IntoResponse for StreamResponse {
  fn into_response(self) -> axum::response::Response {
    let mut builder = Response::builder().status(self.0.status());

    for (header, value) in self.0.headers().iter() {
      builder = builder.header(header, value);
    }

    let stream = StreamBody::new(self.0.bytes_stream());

    builder.body(body::boxed(stream)).expect("create body")
  }
}
