use std::sync::LazyLock;

use futures::Future;

mod byte_stream;
mod feed_ext;

pub use byte_stream::ByteStream;
use tokio::sync::Semaphore;

#[derive(Default)]
pub struct W<T>(pub T);

// ensure only a limited set of ytdlp processes at a time
pub static YTDLP_MUTEX: LazyLock<Semaphore> = LazyLock::new(|| {
  let concurrency = std::env::var("YTDLP_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .unwrap_or(1);
  Semaphore::new(concurrency)
});

// Races multiple futures concurrently and returns the first future that resolves to an `Ok` result,
// while preserving the order of the input futures.
//
// The provided futures are executed in parallel, and as soon as a future returns an `Ok` result,
// it is returned as the final result, canceling the execution of the remaining futures.
// If none of the futures return `Ok`, the last encountered error is returned.
//
// # Arguments
//
// * `futs`: A vector of futures implementing `Future<Output = Result<A, E>>`.
//           The order of the futures in the vector determines their execution order.
//
// # Returns
//
// * `Ok(A)`: If any of the futures return `Ok`, the first encountered `Ok` result is returned.
// * `Err(E)`: If none of the futures return `Ok`, the last encountered `Err` result is returned.
//
// # Panics
//
// This function will panic if the input list of futures is empty.
//
pub async fn race_ordered_first_ok<A, E>(
  futs: Vec<impl Future<Output = Result<A, E>>>,
) -> Result<A, E> {
  use futures::stream::StreamExt;

  let mut futs = futures::stream::iter(futs).buffered(10);

  let mut last_err = None;
  while let Some(res) = futs.next().await {
    match res {
      Ok(res) => return Ok(res),
      Err(e) => {
        last_err = Some(e);
      }
    }
  }

  Err(last_err.unwrap())
}

#[cfg(test)]
mod test {
  use std::time::Duration;

  async fn sleep_and_return<A>(dur: Duration, res: A) -> A {
    tokio::time::sleep(dur).await;
    res
  }

  #[tokio::test]
  async fn test_race_ordered_first_ok() {
    let futs = vec![
      // failed one should not be returned
      sleep_and_return(Duration::from_millis(300), Err(1)),
      // the first successful one should be returned
      sleep_and_return(Duration::from_millis(400), Ok(2)),
      // a fast successful one at a later position shouldn't be returned
      sleep_and_return(Duration::from_millis(100), Ok(3)),
    ];

    let now = std::time::Instant::now();
    let res = super::race_ordered_first_ok(futs).await;

    assert_eq!(res, Ok(2));

    // the parallelism is working.
    assert!(now.elapsed() > Duration::from_millis(400));
    assert!(now.elapsed() < Duration::from_millis(500));
  }
}

// read ytdlp_proxy from environment variable (YTDLP_PROXY) and return it.
static YTDLP_PROXY: LazyLock<Option<String>> =
  LazyLock::new(|| std::env::var("YTDLP_PROXY").ok());

pub fn ytdlp_proxy() -> Option<&'static str> {
  YTDLP_PROXY.as_deref()
}
