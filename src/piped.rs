use std::{
  convert::Infallible,
  sync::{Arc, Mutex, RwLock},
  time::Duration,
};

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Query};
use once_cell::sync::Lazy;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::{
  sync::mpsc::{channel, Receiver, Sender},
  task::JoinSet,
};
use tracing::{info, warn};

const DEFAULT_PIPED_INSTANCE: &str = "https://pipedapi.leptons.xyz";

const PIPED_WIKI_URL: &str =
  "https://raw.githubusercontent.com/TeamPiped/documentation/refs/heads/main/content/docs/public-instances/index.md";

const PIPED_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

const PIPED_TEST_CHANNEL: &str = "UC1yNl2E66ZzKApQdRuTQ4tw";

#[allow(unused)]
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
    PipedInstanceRepo::instance()
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
        .unwrap_or_default();

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
  current_instance: Mutex<PipedInstance>,
  update_signal: Sender<()>,
  update_receiver: RwLock<Option<Receiver<()>>>,
  interval: Duration,
}

impl Default for PipedInstanceRepo {
  fn default() -> Self {
    Self::new(Duration::from_secs(60 * 60))
  }
}

static GLOBAL_REPO: Lazy<PipedInstanceRepo> = Lazy::new(Default::default);

impl PipedInstanceRepo {
  pub fn global() -> &'static Self {
    &GLOBAL_REPO
  }

  pub fn instance() -> PipedInstance {
    GLOBAL_REPO.current_instance.lock().unwrap().clone()
  }

  pub fn notify_update<E: std::error::Error>(e: E) -> E {
    eprintln!("Failed requesting piped: {e:?}, refreshing");
    GLOBAL_REPO.update_signal.try_send(()).ok();
    e
  }

  fn new(interval: Duration) -> Self {
    let (update_signal, update_receiver) = channel(1);
    let update_receiver = RwLock::new(Some(update_receiver));

    Self {
      wiki_url: PIPED_WIKI_URL.to_string(),
      current_instance: Mutex::new(PipedInstance::new(
        DEFAULT_PIPED_INSTANCE.to_string(),
      )),
      update_signal,
      update_receiver,
      interval,
    }
  }

  pub async fn run() {
    let this = Self::global();

    let mut receiver = this
      .update_receiver
      .write()
      .expect("piped instance repo not run as singleton")
      .take()
      .unwrap();

    loop {
      let Ok(instances) = this.pull_latest().await else {
        warn!(
          "Failed pulling latest piped instances. Retrying after {:?}",
          this.interval
        );
        tokio::time::sleep(this.interval).await;
        continue;
      };
      let instances = check_latency(&instances).await;

      if instances.is_empty() {
        warn!(
          "No available piped instances found. Retrying after {:?}",
          this.interval
        );
        tokio::time::sleep(this.interval).await;
        continue;
      }

      if let Some(instance) = instances.into_iter().next() {
        let mut global = this.current_instance.lock().unwrap();
        *global = instance.instance;
        info!("Selected new piped instance: {:?}", *global);
      };

      tokio::select! {
        () = tokio::time::sleep(this.interval) => {},
        _ = receiver.recv() => {},
      }
    }
  }

  async fn pull_latest(&self) -> anyhow::Result<Vec<PipedInstanceStat>> {
    let markdown = reqwest::get(&self.wiki_url).await?.text().await?;

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

    Ok(instances)
  }
}

async fn check_latency(
  instances: &[PipedInstanceStat],
) -> Vec<PipedInstanceStat> {
  let client = Arc::new(
    reqwest::Client::builder()
      .timeout(PIPED_PROBE_TIMEOUT)
      .build()
      .unwrap(),
  );

  let mut tasks = JoinSet::new();
  for stat in instances {
    let mut stat = stat.clone();
    let client = client.clone();
    tasks.spawn(async move {
      let start = tokio::time::Instant::now();
      let url = stat.instance.channel_url(PIPED_TEST_CHANNEL);
      match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
          let elapsed = start.elapsed().as_millis() as u64;
          stat.latency = Some(elapsed);
        }
        _ => {}
      }
      stat
    });
  }

  let now = tokio::time::Instant::now();
  let mut output = vec![];

  while let Some(stat) = tasks.join_next().await {
    if let Ok(stat) = stat {
      output.push(stat);
    }

    if now.elapsed() >= PIPED_PROBE_TIMEOUT {
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
    let repo = PipedInstanceRepo::global();

    let instances = repo.pull_latest().await?;
    println!("{:#?}", &instances);
    assert!(!instances.is_empty());
  }
}
