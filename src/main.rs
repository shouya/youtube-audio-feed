use std::{
  net::SocketAddr,
  sync::{Arc, LazyLock},
};

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
pub use util::YTDLP_MUTEX;

use crate::audio_store::AudioStore;

pub static INSTANCE_PUBLIC_URL: LazyLock<String> = LazyLock::new(|| {
  std::env::var("INSTANCE_PUBLIC_URL")
    .unwrap_or_else(|_| "http://localhost:8080".to_owned())
});

pub static GENERATOR_STR: LazyLock<String> = LazyLock::new(|| {
  std::env::var("GENERATOR_STR").unwrap_or_else(|_| {
    "https://github.com/shouya/youtube_audio_feed".to_owned()
  })
});

pub static AUDIO_STORE_PATH: LazyLock<String> = LazyLock::new(|| {
  std::env::var("AUDIO_STORE_PATH")
    .unwrap_or_else(|_| "/tmp/audio-store".to_owned())
});

pub static BIND_ADDRESS: LazyLock<SocketAddr> = LazyLock::new(|| {
  std::env::var("BIND_ADDRESS")
    .map(|addr| addr.parse().expect("Invalid BIND_ADDRESS"))
    .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 8080)))
});

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
  tracing_subscriber::fmt::init();

  #[cfg(unix)]
  {
    tokio::spawn(async {
      signal_handler().await.expect("Signal handler failed");
    });
  }

  let audio_store = AudioStore::new(AUDIO_STORE_PATH.as_str());
  let audio_store_ref = audio_store.spawn();

  let app = Router::new()
    .route("/", get(homepage))
    .route("/health", get(health))
    .route("/get-podcast", get(feed::channel_podcast_url))
    .route("/channel/:channel_id", get(feed::channel_podcast_xml))
    .route("/audio/:video_id", get(audio::get_audio))
    .layer(Extension(Arc::new(audio_store_ref)));

  info!("Listening on {}", *BIND_ADDRESS);
  info!("Public URL: {}", &*INSTANCE_PUBLIC_URL);

  tokio::task::spawn(async move { piped::PipedInstanceRepo::run().await });

  axum::Server::bind(&BIND_ADDRESS)
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
