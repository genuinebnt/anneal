use solution::*;

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{self, AsyncRead, AsyncWriteExt, ReadBuf};

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
fn encode(frames: &[Vec<u8>]) -> Vec<u8> {
    frames.iter().flat_map(|f| (f.len() as u16).to_be_bytes().into_iter().chain(f.iter().copied())).collect()
}

#[allow(dead_code)]
fn slow_sender(pieces: Vec<(Vec<u8>, u64)>) -> io::DuplexStream {
    let (mut tx, rx) = io::duplex(32);
    tokio::spawn(async move {
        for (part, pause) in pieces {
            tokio::time::sleep(Duration::from_millis(pause)).await;
            if tx.write_all(&part).await.is_err() {
                return;
            }
        }
    });
    rx
}

#[allow(dead_code)]
async fn read_with_interruptions<R: AsyncRead + Unpin>(r: &mut FrameReader<R>, every: u64) -> Result<Vec<Vec<u8>>, io::ErrorKind> {
    let mut out = Vec::new();
    for _ in 0..200_000 {
        match tokio::time::timeout(Duration::from_millis(every), r.next()).await {
            Err(_) => continue,
            Ok(Ok(Some(f))) => out.push(f),
            Ok(Ok(None)) => return Ok(out),
            Ok(Err(e)) => return Err(e.kind()),
        }
    }
    panic!("the stream never ended")
}

/// Hands out some data, then fails with a real error.
struct Broken(Vec<u8>);

impl AsyncRead for Broken {
    fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.0.is_empty() {
            return Poll::Ready(Err(io::ErrorKind::ConnectionReset.into()));
        }
        let n = self.0.len().min(buf.remaining());
        buf.put_slice(&self.0[..n]);
        self.0.drain(..n);
        Poll::Ready(Ok(()))
    }
}

#[test]
fn empty_stream() {
    run(async {
        let mut r = FrameReader::new(&[][..]);
        check!("nothing at all", r.next().await.unwrap(), None);
    });
}

#[test]
fn many_frames_in_one_read_are_all_returned() {
    run(async {
        let want: Vec<Vec<u8>> = (0..200u32).map(|i| i.to_le_bytes().to_vec()).collect();
        let bytes = encode(&want);
        let mut r = FrameReader::new(&bytes[..]);
        check!("200 frames available at once", read_with_interruptions(&mut r, 5).await, Ok(want));
    });
}

#[test]
fn largest_frame() {
    run(async {
        let want = vec![vec![9u8; 65_535], vec![1]];
        let stream = slow_sender(encode(&want).chunks(1000).map(|c| (c.to_vec(), 4)).collect());
        let mut r = FrameReader::new(stream);
        check!("a 65535-byte frame, then one byte", read_with_interruptions(&mut r, 2).await.map(|v| v.iter().map(Vec::len).collect::<Vec<_>>()), Ok(vec![65_535, 1]));
    });
}

#[test]
fn end_inside_the_header() {
    run(async {
        let mut r = FrameReader::new(&[0u8][..]);
        check!("one header byte", r.next().await.map_err(|e| e.kind()), Err(io::ErrorKind::UnexpectedEof));
    });
}

#[test]
fn end_after_header() {
    run(async {
        let mut r = FrameReader::new(&[0u8, 3][..]);
        check!("a header and no payload", r.next().await.map_err(|e| e.kind()), Err(io::ErrorKind::UnexpectedEof));
    });
}

#[test]
fn truncated_after_interruptions() {
    run(async {
        let mut bytes = encode(&[b"ok".to_vec(), b"cut".to_vec()]);
        bytes.pop();
        let stream = slow_sender(bytes.chunks(1).map(|c| (c.to_vec(), 10)).collect());
        let mut r = FrameReader::new(stream);
        check!("last byte missing, slow and interrupted", read_with_interruptions(&mut r, 3).await, Err(io::ErrorKind::UnexpectedEof));
    });
}

#[test]
fn other_errors_pass_through() {
    run(async {
        let mut r = FrameReader::new(Broken(vec![0, 1, 42, 0]));
        check!("first frame", r.next().await.map_err(|e| e.kind()), Ok(Some(vec![42])));
        check!("then the stream's own error", r.next().await.map_err(|e| e.kind()), Err(io::ErrorKind::ConnectionReset));
    });
}

#[test]
fn dropped_before_any_byte() {
    run(async {
        let stream = slow_sender(vec![(encode(&[b"late".to_vec()]), 500)]);
        let mut r = FrameReader::new(stream);
        check!("many dropped calls before the first byte", read_with_interruptions(&mut r, 1).await, Ok(vec![b"late".to_vec()]));
    });
}

#[test]
fn random_chunking_and_interruptions() {
    let mut rng = anneal_prelude::Rng::new(8103);
    for _ in 0..200 {
        let count = rng.below(6);
        let want: Vec<Vec<u8>> = (0..count)
            .map(|_| {
                let len = rng.below(12);
                rng.vec(len, 0, 255)
            })
            .collect();
        let mut bytes = &encode(&want)[..];
        let mut pieces = Vec::new();
        while !bytes.is_empty() {
            let n = (1 + rng.below(9)).min(bytes.len());
            pieces.push((bytes[..n].to_vec(), rng.int(0, 6) as u64));
            bytes = &bytes[n..];
        }
        let every = rng.int(1, 5) as u64;
        let shown = format!("frames {want:?}, next dropped every {every} ms, pieces {pieces:?}");
        let got = run(async {
            let mut r = FrameReader::new(slow_sender(pieces));
            read_with_interruptions(&mut r, every).await
        });
        check!(shown, got, Ok(want));
    }
}
