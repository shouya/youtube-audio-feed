mod rss_piped;
mod rss_ytextract;

use async_trait::async_trait;

pub use rss_piped::RssPiped;
pub use rss_ytextract::RssYtextract;

use crate::{podcast::Podcast, Result};

#[async_trait]
pub trait Harvestor {
  async fn harvest(&self, channel_id: &str) -> Result<Podcast>;
}
