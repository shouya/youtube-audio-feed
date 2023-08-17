use std::str::FromStr;

use axum::body::StreamBody;
use axum::{
  body, extract::Path, headers::HeaderMap, http::Response,
  response::IntoResponse,
};
use http::HeaderName;
use reqwest::header;

use crate::extractor::{self, Extraction, Extractor};
use crate::piped::PipedInstance;
use crate::util::race_ordered_first_ok;
use crate::{Error, Result};

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
  piped: PipedInstance,
  headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let piped_extractor = extractor::Piped(&piped);
  let extractions: Vec<_> = vec![
    extractor::Ytdlp.extract(&video_id),
    extractor::Rustube.extract(&video_id),
    piped_extractor.extract(&video_id),
  ];

  let extraction = race_ordered_first_ok(extractions).await?;

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
    .header("Range", "bytes=0-")
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
