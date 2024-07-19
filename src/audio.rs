use std::io::SeekFrom;
use std::str::FromStr;
use std::sync::Arc;

use axum::body::StreamBody;
use axum::Extension;
use axum::{
  body, extract::Path, headers::HeaderMap, http::Response,
  response::IntoResponse,
};
use futures::stream::BoxStream;
use futures::TryStreamExt as _;
use http::{HeaderName, HeaderValue};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header;
use tokio::fs::File;
use tokio::io::AsyncSeekExt as _;
use tokio_util::io::ReaderStream;

use crate::audio_store::AudioStoreRef;
use crate::extractor::{self, Extraction, Extractor};
use crate::piped::PipedInstance;
use crate::util::{race_ordered_first_ok, ByteStream};
use crate::{Error, Result};

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
  piped: PipedInstance,
  req_headers: HeaderMap,
  Extension(audio_store): Extension<Arc<AudioStoreRef>>,
) -> Result<impl IntoResponse> {
  let piped_extractor = extractor::Piped(&piped);
  let ytdlp_stream = extractor::YtdlpStream::new(audio_store);
  let extractions: Vec<_> = vec![
    ytdlp_stream.extract(&video_id),
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
    Extraction::Stream {
      stream,
      mime_type,
      filesize,
    } => proxy_stream(stream, mime_type, filesize, req_headers)
      .await
      .map(|x| x.into_response()),
    Extraction::File { file, mime_type } => {
      serve_file(file, mime_type, req_headers)
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
  filesize: Option<u64>,
  req_headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let range = req_headers
    .get(header::RANGE)
    .and_then(|range| range.to_str().ok());
  let range = parse_range(range);

  let stream = ByteStream::new(stream);

  let stream = match range {
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

  let mut headers = HeaderMap::new();
  headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime_type)?);

  if let Some(content_range) = to_content_range(range) {
    headers.insert(
      header::CONTENT_RANGE,
      HeaderValue::from_str(&content_range)?,
    );
  }

  if let Some(filesize) = filesize {
    headers.insert(
      header::CONTENT_LENGTH,
      HeaderValue::from_str(&filesize.to_string())?,
    );
  }

  Ok((headers, stream))
}

fn to_content_range(range: (Option<usize>, Option<usize>)) -> Option<String> {
  match range {
    (Some(start), Some(end)) => Some(format!("bytes {}-{}/*", start, end)),
    (Some(start), None) => Some(format!("bytes {}-*/?", start)),
    (None, Some(end)) => Some(format!("bytes */{}", end + 1)),
    (_, _) => None,
  }
}

async fn serve_file(
  mut file: File,
  mime_type: String,
  req_headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let range = req_headers
    .get(header::RANGE)
    .and_then(|range| range.to_str().ok());
  let (from, to) = parse_range(range);

  let metadata = file.metadata().await.expect("metadata");
  let len = metadata.len() as usize;

  let from = from.unwrap_or(0).min(len as usize);
  let to = to.unwrap_or(len - 1).min(len - 1).max(from);
  let content_len = to - from + 1;
  file.seek(SeekFrom::Start(from as u64)).await?;

  let stream = ByteStream::new(ReaderStream::new(file).map_err(|e| e.into()))
    .limit_bytes(content_len);

  let mut headers = HeaderMap::new();
  headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime_type)?);
  headers.insert(
    header::CONTENT_LENGTH,
    HeaderValue::from_str(&content_len.to_string())?,
  );
  headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
  headers.insert(
    header::CONTENT_RANGE,
    HeaderValue::from_str(&format!("bytes {}-{}/{}", from, to, len))?,
  );

  let status = if from != 0 || to != len - 1 {
    http::StatusCode::PARTIAL_CONTENT
  } else {
    http::StatusCode::OK
  };

  Ok((status, headers, StreamBody::new(stream)))
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
