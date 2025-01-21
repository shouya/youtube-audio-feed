mod rss_piped;
mod rss_ytextract;
mod ytdlp;

use async_trait::async_trait;

#[allow(unused)]
pub use rss_piped::RssPiped;
pub use ytdlp::Ytdlp;

use crate::{podcast::Podcast, Result};

#[async_trait]
pub trait Harvestor {
  async fn harvest(&self, channel_id: &str) -> Result<Podcast>;
}
