mod piped;
mod rustube;
mod ytdlp;

use async_trait::async_trait;
use axum::body::Bytes;
use futures::stream::BoxStream;

use crate::Result;

pub use self::rustube::Rustube;
pub use piped::Piped;
pub use ytdlp::Ytdlp;

pub enum Extraction {
  Proxy {
    url: String,
    headers: Vec<(String, String)>,
  },
  Stream {
    stream: BoxStream<'static, Result<Bytes>>,
    mime_type: String,
  },
}

#[async_trait]
pub trait Extractor {
  async fn extract(&self, video_id: &str) -> Result<Extraction>;
}
