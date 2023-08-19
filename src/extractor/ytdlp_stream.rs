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
    let mut child = Command::new("yt-dlp")
      .arg("-f")
      .arg("ba[ext=m4a]")
      .arg("-o")
      .arg("-")
      .arg(url)
      .stdout(std::process::Stdio::piped())
      .spawn()?;

    let stdout = child.stdout.take().expect("stdout not opened");
    let stream = ReaderStream::new(BufReader::new(stdout))
      .map(|res| res.map_err(|e| e.into()));
    let stream = stream.boxed();

    // wait for child to finish in another task
    tokio::spawn(async move { child.wait().await.unwrap() });

    let mime_type = String::from("audio/mp4");
    Ok(Extraction::Stream { mime_type, stream })
  }
}
