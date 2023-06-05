mod piped;
mod rustube;
mod ytdlp;

use async_trait::async_trait;

use crate::Result;

pub use self::rustube::Rustube;
pub use piped::Piped;
pub use ytdlp::Ytdlp;

#[derive(Default, Debug)]
pub struct Extraction {
  pub url: String,
  pub headers: Vec<(String, String)>,
}

#[async_trait]
pub trait Extractor {
  async fn extract(&self, video_id: &str) -> Result<Extraction>;
}
