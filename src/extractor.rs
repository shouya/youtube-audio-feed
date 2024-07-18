mod piped;
mod rustube;
mod ytdlp;
mod ytdlp_stream;

use async_trait::async_trait;
use axum::body::Bytes;
use futures::stream::BoxStream;

use crate::Result;

pub use self::rustube::Rustube;
pub use piped::Piped;
pub use ytdlp_stream::YtdlpStream;

pub enum Extraction {
  Proxy {
    url: String,
    headers: Vec<(String, String)>,
  },
  Stream {
    stream: BoxStream<'static, Result<Bytes>>,
    filesize: Option<u64>,
    mime_type: String,
  },
}

#[async_trait]
pub trait Extractor {
  async fn extract(&self, video_id: &str) -> Result<Extraction>;
}
