use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use tokio::io::{AsyncWriteExt as _, BufReader};
use tokio::process::Command;
use tokio_util::io::ReaderStream;

use crate::{Error, Result};

use super::{Extraction, Extractor};

// run yt-dlp command line to get audio stream directly.
// requires yt-dlp executable to be in PATH.
pub struct YtdlpStream;

#[async_trait]
impl Extractor for YtdlpStream {
  async fn extract(&self, video_id: &str) -> Result<Extraction> {
    let url = format!("https://youtube.com/watch?v={video_id}");

    let info_json = get_info_json(&url).await?;
    stream(&info_json).await
  }
}

fn detect_error(bytes: Bytes) -> Result<Bytes> {
  let s = String::from_utf8_lossy(&bytes);
  if s.contains("ERROR:") {
    Err(Error::AudioStream(s.to_string()))
  } else {
    // do not actually output anything
    Ok(Bytes::new())
  }
}

#[derive(serde::Deserialize)]
struct InfoJson {
  filesize: Option<u64>,
}

async fn get_info_json(url: &str) -> Result<String> {
  let output = Command::new("yt-dlp")
    .arg("-j")
    .arg("-f")
    .arg("ba[ext=m4a]")
    .arg(url)
    .output()
    .await?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!(
      "yt-dlp exited with code ({:?}): {}",
      output.status.code(),
      stderr
    );
    return Err(Error::AudioStream(message));
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  Ok(stdout.to_string())
}

async fn stream(info_json: &str) -> Result<Extraction> {
  let info: InfoJson = serde_json::from_str(info_json)?;

  let mut child = Command::new("yt-dlp")
    .arg("--load-info-json")
    .arg("-")
    .arg("-f")
    .arg("ba[ext=m4a]")
    .arg("-o")
    .arg("-")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()?;

  let mut stdin = child.stdin.take().expect("stdin not opened");
  let stdout = child.stdout.take().expect("stdout not opened");
  let stderr = child.stderr.take().expect("stderr not opened");

  stdin.write_all(info_json.as_bytes()).await?;
  drop(stdin);

  let stdout_stream = ReaderStream::new(BufReader::new(stdout))
    .map(|res| res.map_err(|e| e.into()));
  let stderr_stream = ReaderStream::new(BufReader::new(stderr))
    .map(|res| res.map_err(|e| e.into()))
    .map(|res| res.and_then(detect_error));
  let stream = futures::stream::select(stdout_stream, stderr_stream).boxed();

  // wait for child to finish in another task
  tokio::spawn(async move { child.wait().await.unwrap() });

  let mime_type = String::from("audio/mp4");
  let filesize = info.filesize;

  Ok(Extraction::Stream {
    mime_type,
    stream,
    filesize,
  })
}
