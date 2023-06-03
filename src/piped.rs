use std::{convert::Infallible, sync::Mutex};

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Query};
use once_cell::sync::Lazy;
use serde::Deserialize;

const DEFAULT_PIPED_INSTANCE: &str = "pipedapi.kavin.rocks";

static GLOBAL_PIPED_INSTANCE: Lazy<Mutex<PipedInstance>> =
  Lazy::new(|| Mutex::new(PipedInstance::default()));

#[derive(Clone, Debug)]
pub struct PipedInstance {
  domain: String,
}

impl PipedInstance {
  fn new(domain: String) -> Self {
    Self { domain }
  }

  pub fn channel_url(&self, channel_id: &str) -> String {
    format!("https://{}/channel/{}", self.domain, channel_id)
  }

  pub fn stream_url(&self, video_id: &str) -> String {
    format!("https://{}/streams/{}", self.domain, video_id)
  }
}

impl Default for PipedInstance {
  fn default() -> Self {
    Self::new(DEFAULT_PIPED_INSTANCE.to_string())
  }
}

#[derive(Deserialize)]
struct PipedInstanceQuery {
  piped_instance: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for PipedInstance
where
  S: Send + Sync,
{
  type Rejection = Infallible;

  async fn from_request_parts(
    parts: &mut http::request::Parts,
    state: &S,
  ) -> Result<Self, Self::Rejection> {
    // extract &piped_instance=<value> from URL or use global instance.
    let instance =
      Query::<PipedInstanceQuery>::from_request_parts(parts, state)
        .await
        .map(|query| PipedInstance::new(query.0.piped_instance))
        .unwrap_or_else(|_| GLOBAL_PIPED_INSTANCE.lock().unwrap().clone());

    Ok(instance)
  }
}
