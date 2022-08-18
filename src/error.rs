use axum::response::{IntoResponse, Response};
use reqwest::StatusCode;

pub enum Error {
  Server(anyhow::Error),
  #[allow(unused)]
  Client(anyhow::Error),
}

impl IntoResponse for Error {
  fn into_response(self) -> Response {
    match self {
      Error::Server(err) => {
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
      }
      Error::Client(err) => {
        (StatusCode::BAD_REQUEST, err.to_string()).into_response()
      }
    }
  }
}

impl<E> From<E> for Error
where
  E: Into<anyhow::Error>,
{
  fn from(err: E) -> Self {
    Error::Server(err.into())
  }
}
