use async_trait::async_trait;
use futures::StreamExt;
use tokio::io::BufReader;
use tokio_util::io::ReaderStream;

use crate::Result;

use super::{Extraction, Extractor};

// run yt-dlp command line to get audio stream directly.
// requires yt-dlp executable to be in PATH.
pub struct YtdlpStream;

#[async_trait]
impl Extractor for YtdlpStream {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
    use tokio::process::Command;

    let url = format!("https://youtube.com/watch?v={video_id}");

    // yt-dlp -f 'ba[ext=m4a]' -o - 'https://youtube.com/watch?v=XXXXXXX'
    let child = Command::new("yt-dlp")
      .arg("-f")
      .arg("ba[ext=m4a]")
      .arg("-o")
      .arg("-")
      .arg(url)
      .spawn()?;

    let stream = ReaderStream::new(BufReader::new(child.stdout.unwrap()))
      .map(|res| res.map_err(|e| e.into()));
    let stream = stream.boxed();

    let mime_type = String::from("audio/mp4");

    Ok(Extraction::Stream { mime_type, stream })
  }
}
