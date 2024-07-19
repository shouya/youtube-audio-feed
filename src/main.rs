use std::sync::Arc;

use axum::{
  headers::ContentType, response::IntoResponse, routing::get, Extension,
  Router, TypedHeader,
};

mod audio;
mod audio_store;
mod error;
mod extractor;
mod feed;
mod harvestor;
mod piped;
mod podcast;
mod rss;
mod util;

pub use error::{Error, Result};
pub use util::W;

use crate::audio_store::AudioStore;

#[cfg(not(debug_assertions))]
pub const INSTANCE_PUBLIC_URL: &str = "https://youtube-audio-feed.fly.dev";

#[cfg(debug_assertions)]
pub const INSTANCE_PUBLIC_URL: &str = "http://192.168.86.40:8080";

pub const GENERATOR_STR: &str = "https://github.com/shouya/youtube_audio_feed";

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
  let audio_store = AudioStore::new("/tmp/audio-store/");
  let audio_store_ref = audio_store.spawn();

  let app = Router::new()
    .route("/", get(homepage))
    .route("/health", get(health))
    .route("/get-podcast", get(feed::channel_podcast_url))
    .route("/channel/:channel_id", get(feed::channel_podcast_xml))
    .route("/audio/:video_id", get(audio::get_audio))
    .layer(Extension(Arc::new(audio_store_ref)));

  ctrlc::set_handler(move || std::process::exit(0))
    .expect("Error setting SIGINT/SIGTERM handler");

  println!("Listening on {}", INSTANCE_PUBLIC_URL);

  tokio::task::spawn(async move { piped::PipedInstanceRepo::run().await });

  axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
    .serve(app.into_make_service())
    .await
    .expect("Failed to start server");

  Ok(())
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
