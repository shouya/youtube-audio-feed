use atom_syndication::Entry;
use axum::response::{IntoResponse, Response};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
  #[error("failed parsing url: {0}")]
  RequestUpstream(#[from] reqwest::Error),
  #[error("atom feed format error: {0}")]
  AtomFormat(#[from] atom_syndication::Error),
  #[error("rss feed format error: {0}")]
  RSSFormat(#[from] rss::Error),
  #[error("invalid feed: {} ({1})", .0.id())]
  InvalidFeedEntry(Entry, &'static str),
  #[error("parse html error: {0}")]
  ParseHTML(#[from] tl::ParseError),
  #[error("parse url error: {0}")]
  ParseURL(#[from] http_types::url::ParseError),
  #[error("info not found in html: {0}")]
  InvalidHTML(&'static str),
  #[error("unsupported url: {0} ({1})")]
  UnsupportedURL(String, &'static str),
  #[error("http error: {0}")]
  HTTP(#[from] http::Error),
  #[error("io error: {0}")]
  IO(#[from] std::io::Error),
  #[error("request invidious error: {0}")]
  Invidious(&'static str),
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    use http::StatusCode;
    use Error::*;

    let code = match &self {
      RequestUpstream(e) => e.status().unwrap_or(StatusCode::BAD_REQUEST),
      AtomFormat(_) => StatusCode::BAD_GATEWAY,
      RSSFormat(_) => StatusCode::BAD_GATEWAY,
      ParseHTML(_) => StatusCode::BAD_GATEWAY,
      ParseURL(_) => StatusCode::BAD_GATEWAY,
      InvalidHTML(_) => StatusCode::BAD_GATEWAY,
      UnsupportedURL(_, _) => StatusCode::BAD_REQUEST,
      HTTP(_) => StatusCode::BAD_GATEWAY,
      _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (code, self.to_string()).into_response()
  }
}
