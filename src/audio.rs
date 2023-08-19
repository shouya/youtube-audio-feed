use std::str::FromStr;

use axum::body::StreamBody;
use axum::{
  body, extract::Path, headers::HeaderMap, http::Response,
  response::IntoResponse,
};
use futures::stream::BoxStream;
use http::HeaderName;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header;

use crate::extractor::{self, Extraction, Extractor};
use crate::piped::PipedInstance;
use crate::util::{race_ordered_first_ok, ByteStream};
use crate::{Error, Result};

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
  piped: PipedInstance,
  req_headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let piped_extractor = extractor::Piped(&piped);
  let extractions: Vec<_> = vec![
    extractor::Ytdlp.extract(&video_id),
    extractor::Rustube.extract(&video_id),
    piped_extractor.extract(&video_id),
  ];

  let extraction = race_ordered_first_ok(extractions).await?;

  match extraction {
    Extraction::Proxy { url, headers } => {
      // Ok(Redirect::temporary(&playable_link))

      proxy_play_link(url, headers, req_headers)
        .await
        .map(|x| x.into_response())
    }
    Extraction::Stream { stream, mime_type } => {
      proxy_stream(stream, mime_type, req_headers)
        .await
        .map(|x| x.into_response())
    }
  }
}

fn parse_range(range: Option<&str>) -> (Option<usize>, Option<usize>) {
  static REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"bytes=(\d+)?-(\d+)?").unwrap());

  let Some(range) = range else {
    return (None, None);
  };

  let Some(captures) = REGEX.captures(range) else {
    return (None, None);
  };

  let start = captures
    .get(1)
    .and_then(|m| m.as_str().parse::<usize>().ok());
  let end = captures
    .get(2)
    .and_then(|m| m.as_str().parse::<usize>().ok());
  (start, end)
}

async fn proxy_stream(
  stream: BoxStream<'static, Result<bytes::Bytes>>,
  mime_type: String,
  req_headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let range = req_headers
    .get(header::RANGE)
    .and_then(|range| range.to_str().ok());

  let stream = ByteStream::new(stream);

  let stream = match parse_range(range) {
    (Some(start), Some(end)) => {
      let stream = stream.skip_bytes(start).limit_bytes(end - start + 1);
      StreamBody::new(stream)
    }
    (Some(start), None) => {
      let stream = stream.skip_bytes(start);
      StreamBody::new(stream)
    }
    (None, Some(end)) => {
      let stream = stream.limit_bytes(end + 1);
      StreamBody::new(stream)
    }
    (_, _) => StreamBody::new(stream),
  };

  Ok(stream)
}

async fn proxy_play_link(
  url: String,
  headers: Vec<(String, String)>,
  mut req_headers: HeaderMap,
) -> Result<impl IntoResponse> {
  req_headers.remove(header::HOST);
  for (key, value) in headers.iter() {
    let key = HeaderName::from_str(key).map_err(|_e| Error::Extraction)?;
    let value =
      header::HeaderValue::from_str(value).map_err(|_e| Error::Extraction)?;
    req_headers.insert(key, value);
  }

  let resp = reqwest::Client::new()
    .get(url)
    .headers(req_headers)
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
