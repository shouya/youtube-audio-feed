use std::str::FromStr;

use axum::body::StreamBody;
use axum::extract::Query;
use axum::{
  body, extract::Path, headers::HeaderMap, http::Response,
  response::IntoResponse,
};
use http::HeaderName;
use reqwest::header;
use serde::Deserialize;

use crate::extractor::{self, Extraction, Extractor};
use crate::piped::PipedInstance;
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
pub struct ExtractorQuery {
  #[serde(default)]
  pub extractor: String,
}

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
  Query(extractor): Query<ExtractorQuery>,
  piped: PipedInstance,
  headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let extractor: Box<dyn Extractor + Send + Sync> =
    match extractor.extractor.as_str() {
      "piped" => Box::new(extractor::Piped(&piped)),
      "rustube" => Box::new(extractor::Rustube),
      "yt-dlp" => Box::new(extractor::Ytdlp),
      _ => Box::new(extractor::Ytdlp),
    };

  let extraction = extractor.extract(&video_id).await?;

  // Ok(Redirect::temporary(&playable_link))

  proxy_play_link(extraction, headers).await
}

async fn proxy_play_link(
  extraction: Extraction,
  mut headers: HeaderMap,
) -> Result<impl IntoResponse> {
  headers.remove(header::HOST);
  for (key, value) in extraction.headers.iter() {
    let key = HeaderName::from_str(key).map_err(|_e| Error::Extraction)?;
    let value =
      header::HeaderValue::from_str(value).map_err(|_e| Error::Extraction)?;
    headers.insert(key, value);
  }

  let resp = reqwest::Client::new()
    .get(extraction.url)
    .headers(headers)
    .send()
    .await?;

  Ok(StreamResponse(resp))
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
