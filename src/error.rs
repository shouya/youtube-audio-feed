use axum::response::{IntoResponse, Response};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("failed parsing url")]
  RequestUpstreamError(#[from] reqwest::Error),
  #[error("failed fetching feed")]
  FailedFetchingFeed,
  #[error("failed converting feed")]
  FailedConvertingFeed(anyhow::Error),
  #[error(transparent)]
  Unknown(#[from] anyhow::Error),
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    use http::StatusCode;
    use Error::*;

    let code = match self {
      RequestUpstreamError(_) => StatusCode::BAD_REQUEST,
      FailedFetchingFeed => StatusCode::INTERNAL_SERVER_ERROR,
      FailedConvertingFeed(_) => StatusCode::INTERNAL_SERVER_ERROR,
      Unknown(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    code.into_response()
  }
}
