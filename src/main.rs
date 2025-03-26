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
use tracing::info;
pub use util::W;

use crate::audio_store::AudioStore;

#[cfg(not(debug_assertions))]
pub const INSTANCE_PUBLIC_URL: &str = "https://youtube-audio-feed.fly.dev";

#[cfg(debug_assertions)]
pub const INSTANCE_PUBLIC_URL: &str = "http://192.168.86.40:8080";

pub const GENERATOR_STR: &str = "https://github.com/shouya/youtube_audio_feed";

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
  tracing_subscriber::fmt::init();

  #[cfg(unix)]
  {
    tokio::spawn(async {
      signal_handler().await.expect("Signal handler failed");
    });
  }

  let audio_store = if cfg!(not(debug_assertions)) {
    AudioStore::new("/data/audio-store/")
  } else {
    AudioStore::new("/tmp/audio-store/")
  };
  let audio_store_ref = audio_store.spawn();

  let app = Router::new()
    .route("/", get(homepage))
    .route("/health", get(health))
    .route("/get-podcast", get(feed::channel_podcast_url))
    .route("/channel/:channel_id", get(feed::channel_podcast_xml))
    .route("/audio/:video_id", get(audio::get_audio))
    .layer(Extension(Arc::new(audio_store_ref)));

  info!("Listening on {}", INSTANCE_PUBLIC_URL);

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

#[cfg(unix)]
async fn signal_handler() -> Result<()> {
  use tokio::signal::unix::{signal, SignalKind};

  let mut sigint = signal(SignalKind::interrupt())?;
  let mut sigterm = signal(SignalKind::terminate())?;

  tokio::select! {
    _ = sigint.recv() => {
      info!("Received SIGINT, shutting down...");
    }
    _ = sigterm.recv() => {
      info!("Received SIGTERM, shutting down...");
    }
  };

  std::process::exit(0)
}
