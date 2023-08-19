use std::{
  pin::Pin,
  task::{Context, Poll},
};

use bytes::Bytes;
use futures::Stream;

use crate::Result;

pub struct ByteStream<T> {
  stream: T,
  skip_bytes: usize,
  // inclusive
  limit_bytes: Option<usize>,
}

impl<T> ByteStream<T> {
  pub fn new(stream: T) -> Self {
    ByteStream {
      stream,
      skip_bytes: 0,
      limit_bytes: None,
    }
  }

  pub fn skip_bytes(self, bytes: usize) -> Self {
    ByteStream {
      skip_bytes: bytes,
      ..self
    }
  }

  pub fn limit_bytes(self, bytes: usize) -> Self {
    ByteStream {
      limit_bytes: Some(bytes),
      ..self
    }
  }
}

impl<T> Stream for ByteStream<T>
where
  T: Stream<Item = Result<Bytes>> + Unpin,
{
  type Item = Result<Bytes>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    let this = &mut *self;
    let poll = Pin::new(&mut this.stream).poll_next(cx);
    match poll {
      Poll::Ready(Some(Ok(bytes))) => {
        if bytes.len() <= this.skip_bytes {
          this.skip_bytes -= bytes.len();
          cx.waker().wake_by_ref();
          return Poll::Pending;
        }

        let bytes = bytes.slice(this.skip_bytes..);
        this.skip_bytes = 0;

        match this.limit_bytes {
          None => Poll::Ready(Some(Ok(bytes))),
          Some(0) => Poll::Ready(None),
          Some(limit_bytes) => {
            if bytes.len() > limit_bytes {
              let bytes = bytes.slice(..limit_bytes);
              this.limit_bytes = Some(0);
              Poll::Ready(Some(Ok(bytes)))
            } else {
              this.limit_bytes = Some(limit_bytes - bytes.len());
              Poll::Ready(Some(Ok(bytes)))
            }
          }
        }
      }
      Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err))),
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
    }
  }
}

#[cfg(test)]
mod test {
  use super::*;
  use futures::executor::block_on_stream;

  #[test]
  fn test_byte_stream() {
    assert_bytes(&[b"hello", b"world"], &[b"hello", b"world"], 0, None);
    assert_bytes(&[b"hello", b"world"], &[b"llo", b"world"], 2, None);
    assert_bytes(&[b"hello", b"world"], &[b"hello", b"w"], 0, Some(6));
    assert_bytes(&[b"hello", b"world"], &[b"llo", b"wor"], 2, Some(6));

    assert_bytes(&[b"hello", b"world"], &[b"world"], 5, None);
    assert_bytes(&[b"hello", b"world"], &[b"orld"], 6, None);
    assert_bytes(&[b"hello", b"world"], &[b"hel"], 0, Some(3));
    assert_bytes(&[b"hello", b"world"], &[b"hello"], 0, Some(5));
    assert_bytes(&[b"hello", b"world"], &[], 0, Some(0));

    assert_bytes(&[b"hello", b"world"], &[], 100, None);
    assert_bytes(&[b"hello", b"world"], &[b"hello", b"world"], 0, Some(100));
    assert_bytes(&[b"hello", b"world"], &[], 100, Some(100));

    assert_bytes(&[], &[], 0, None);
    assert_bytes(&[], &[], 100, None);
    assert_bytes(&[], &[], 0, Some(100));
    assert_bytes(&[], &[], 100, Some(100));
  }

  fn assert_bytes(
    stream: &[&[u8]],
    expect: &[&[u8]],
    skip: usize,
    limit: Option<usize>,
  ) {
    let stream = futures::stream::iter(
      stream
        .iter()
        .map(|bytes| Ok(Bytes::copy_from_slice(*bytes))),
    );

    let stream = ByteStream::new(stream).skip_bytes(skip);
    let stream = match limit {
      Some(limit) => stream.limit_bytes(limit),
      None => stream,
    };

    let bytes = block_on_stream(stream)
      .map(Result::unwrap)
      .collect::<Vec<_>>();

    let expect = expect
      .iter()
      .map(|bytes| Bytes::copy_from_slice(*bytes))
      .collect::<Vec<_>>();

    assert_eq!(bytes, expect);
  }
}
