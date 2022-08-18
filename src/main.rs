use anyhow::Result;

use axum::{routing::get, Router, response::IntoResponse};

mod audio;
mod error;
mod feed;

pub use error::Error;

pub const INSTANCE_PUBLIC_URL: &str = "https://youtube-audio-feed.foobar.com";
pub const GENERATOR_STR: &str = "youtube_audio_feed";
pub const INVIDIOUS_INSTANCE: &str = "https://invidious.namazso.eu";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let app = Router::new()
    .route("/", get(homepage))
    .route("/health", get(health))
    .route("/channel/:channel_id", get(feed::channel_podcast_xml))
    .route("/audio/:video_id", get(audio::get_audio));

  axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
    .serve(app.into_make_service())
    .await
    .map_err(|e| e.into())
}

async fn homepage() -> impl IntoResponse {
  "Hello!".to_owned()
}

async fn health() -> impl IntoResponse {
  "ok".to_owned()
}
