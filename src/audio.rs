use axum::body::StreamBody;
use axum::response::Redirect;
use axum::{
  body, extract::Path, headers::HeaderMap, http::Response,
  response::IntoResponse,
};
use reqwest::header;
use serde_query::{DeserializeQuery, Query};

use crate::piped::PipedInstance;
use crate::Result;

#[axum::debug_handler]
pub async fn get_audio(
  Path(video_id): Path<String>,
  piped: PipedInstance,
  _headers: HeaderMap,
) -> Result<impl IntoResponse> {
  let playable_link = get_playable_link(&video_id, &piped).await?;
  Ok(Redirect::temporary(&playable_link))

  // proxy_play_link(&playable_link, headers).await
}

async fn proxy_play_link(
  url: &str,
  mut headers: HeaderMap,
) -> Result<impl IntoResponse> {
  headers.remove(header::HOST);

  let resp = reqwest::Client::new()
    .get(url)
    .headers(headers)
    .send()
    .await?;

  Ok(StreamResponse(resp))
}

// https://docs.piped.video/docs/api-documentation/
#[derive(DeserializeQuery)]
struct PipedStreamResp {
  #[query(".audioStreams.[0].url")]
  url: String,
}

async fn get_playable_link(
  video_id: &str,
  piped: &PipedInstance,
) -> Result<String> {
  let piped_url = piped.stream_url(video_id);
  let resp: PipedStreamResp = reqwest::Client::new()
    .get(piped_url)
    .header("User-Agent", "Mozilla/5.0")
    .send()
    .await?
    .json::<Query<PipedStreamResp>>()
    .await?
    .into();

  Ok(resp.url)
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
