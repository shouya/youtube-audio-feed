use anyhow::Result;

use axum::{
  headers::ContentType, response::IntoResponse, routing::get, Router,
  TypedHeader,
};

mod audio;
mod error;
mod feed;

pub use error::Error;

pub const INSTANCE_PUBLIC_URL: &str = "https://youtube-audio-feed.fly.dev";
pub const GENERATOR_STR: &str = "youtube_audio_feed";
pub const INVIDIOUS_INSTANCE: &str = "https://invidious.namazso.eu";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  let app = Router::new()
    .route("/", get(homepage))
    .route("/health", get(health))
    .route("/get-podcast", get(feed::channel_podcast_url))
    .route("/channel/:channel_id", get(feed::channel_podcast_xml))
    .route("/audio/:video_id", get(audio::get_audio));

  axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
    .serve(app.into_make_service())
    .await
    .map_err(|e| e.into())
}

pub const HOMEPAGE_HTML: &str = include_str!("../html/homepage.html");

async fn homepage() -> impl IntoResponse {
  (
    TypedHeader::<ContentType>(ContentType::html()),
    HOMEPAGE_HTML,
  )
}

async fn health() -> impl IntoResponse {
  "ok".to_owned()
}
