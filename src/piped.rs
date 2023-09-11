#![allow(dead_code)]
use std::{convert::Infallible, sync::Mutex, time::Duration};

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Query};
use once_cell::sync::Lazy;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::{task::JoinSet, time::sleep};

const DEFAULT_PIPED_INSTANCE: &str = "https://pipedapi.aeong.one";

static GLOBAL_PIPED_INSTANCE: Lazy<Mutex<PipedInstance>> =
  Lazy::new(|| Mutex::new(PipedInstance::default()));

const PIPED_WIKI_URL: &str =
  "https://raw.githubusercontent.com/wiki/TeamPiped/Piped/Instances.md";

#[derive(Clone, Debug, Serialize)]
pub struct PipedInstance {
  api_url: String,
}

impl PipedInstance {
  fn new(domain: String) -> Self {
    Self { api_url: domain }
  }

  pub fn channel_url(&self, channel_id: &str) -> String {
    format!("{}/channel/{}", self.api_url, channel_id)
  }

  pub fn stream_url(&self, video_id: &str) -> String {
    format!("{}/streams/{}", self.api_url, video_id)
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

#[derive(Debug, Clone, Serialize)]
struct PipedInstanceStat {
  instance: PipedInstance,
  name: String,
  countries: Vec<String>,
  latency: Option<u64>,
}

pub struct PipedInstanceRepo {
  wiki_url: String,
}

impl Default for PipedInstanceRepo {
  fn default() -> Self {
    Self {
      wiki_url: PIPED_WIKI_URL.to_string(),
    }
  }
}

impl PipedInstanceRepo {
  pub async fn auto_update_global(self, interval: Duration) {
    loop {
      let instances = self.pull_latest().await;
      let instances = check_latency(&instances, Duration::from_secs(10)).await;

      if instances.is_empty() {
        println!("No piped instance available.");
        continue;
      }

      if let Some(instance) = instances.into_iter().next() {
        let mut global = GLOBAL_PIPED_INSTANCE.lock().unwrap();
        *global = instance.instance;
        println!("Global piped instance updated: {:?}", *global);
      };

      sleep(interval).await;
    }
  }

  async fn pull_latest(&self) -> Vec<PipedInstanceStat> {
    let markdown = reqwest::get(&self.wiki_url)
      .await
      .unwrap()
      .text()
      .await
      .unwrap();

    let mut instances: Vec<PipedInstanceStat> = vec![];
    const START_MARKER: &str = "--- | --- | --- | ---";
    let interesting_lines = markdown
      .lines()
      .skip_while(|line| !line.trim().starts_with(START_MARKER))
      .skip(1);

    for line in interesting_lines {
      let parts: Vec<String> = line.split('|').map(|x| x.to_string()).collect();
      if parts.len() != 5 {
        continue;
      }

      let name = parts[0].trim().to_string();
      let url = parts[1].trim().to_string();

      if !url.starts_with("https://") {
        continue;
      }

      let countries = parts[2]
        .trim()
        .split(',')
        .map(|x| x.trim().to_string())
        .map(|x| x.chars().map(from_flag_emoji).collect::<String>())
        .collect();
      let latency = None;

      instances.push(PipedInstanceStat {
        instance: PipedInstance::new(url),
        name,
        countries,
        latency,
      });
    }

    instances.shuffle(&mut rand::thread_rng());

    instances
  }
}

async fn check_latency(
  instances: &[PipedInstanceStat],
  timeout: Duration,
) -> Vec<PipedInstanceStat> {
  let mut tasks = JoinSet::new();
  for stat in instances {
    let mut stat = stat.clone();
    tasks.spawn(async move {
      let start = tokio::time::Instant::now();
      if let Ok(resp) = reqwest::get(&stat.instance.api_url).await {
        if resp.status().is_success() {
          let elapsed = start.elapsed().as_millis() as u64;
          stat.latency = Some(elapsed);
        }
      };
      stat
    });
  }

  let now = tokio::time::Instant::now();
  let mut output = vec![];

  while let Some(stat) = tasks.join_next().await {
    if let Ok(stat) = stat {
      output.push(stat);
    }
    if now.elapsed() >= timeout {
      break;
    }
  }

  output.sort_by_key(|x| x.latency.unwrap_or(u64::MAX));

  output
}

fn from_flag_emoji(flag_char: char) -> char {
  let code = u32::from(flag_char) - 0x1f1e6_u32 + u32::from('A');
  std::char::from_u32(code).unwrap_or(flag_char)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_piped_instance_repo() {
    let repo = PipedInstanceRepo {
      wiki_url: PIPED_WIKI_URL.to_string(),
    };

    let instances = repo.pull_latest().await;
    println!("{:#?}", &instances);
    assert!(!instances.is_empty());
  }
}
