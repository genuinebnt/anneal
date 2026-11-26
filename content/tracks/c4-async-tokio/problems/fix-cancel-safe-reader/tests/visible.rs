use solution::*;

use std::time::Duration;
use tokio::io::{self, AsyncWriteExt};
use tokio::sync::mpsc;

/// A runtime with a paused clock: sleeps take no real time and happen in a fixed order.
#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
fn encode(frames: &[Vec<u8>]) -> Vec<u8> {
    frames.iter().flat_map(|f| (f.len() as u16).to_be_bytes().into_iter().chain(f.iter().copied())).collect()
}

/// Writes `bytes` in pieces of `piece` bytes, 10 ms apart, then closes the stream.
#[allow(dead_code)]
fn slow_sender(bytes: Vec<u8>, piece: usize) -> io::DuplexStream {
    let (mut tx, rx) = io::duplex(64);
    tokio::spawn(async move {
        for part in bytes.chunks(piece) {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if tx.write_all(part).await.is_err() {
                return;
            }
        }
    });
    rx
}

/// Reads every frame, dropping the `next` future whenever 3 ms pass first.
#[allow(dead_code)]
async fn read_with_interruptions<R: tokio::io::AsyncRead + Unpin>(r: &mut FrameReader<R>) -> Result<Vec<Vec<u8>>, io::ErrorKind> {
    let mut out = Vec::new();
    for _ in 0..100_000 {
        match tokio::time::timeout(Duration::from_millis(3), r.next()).await {
            Err(_) => continue,
            Ok(Ok(Some(f))) => out.push(f),
            Ok(Ok(None)) => return Ok(out),
            Ok(Err(e)) => return Err(e.kind()),
        }
    }
    panic!("the stream never ended")
}

#[test]
fn whole_frames_without_interruption() {
    run(async {
        let bytes = encode(&[b"hello".to_vec(), Vec::new(), b"!".to_vec()]);
        let mut r = FrameReader::new(&bytes[..]);
        check!("first", r.next().await.unwrap(), Some(b"hello".to_vec()));
        check!("empty payload", r.next().await.unwrap(), Some(Vec::new()));
        check!("third", r.next().await.unwrap(), Some(b"!".to_vec()));
        check!("end", r.next().await.unwrap(), None);
        check!("end again", r.next().await.unwrap(), None);
    });
}

#[test]
fn payload_arrives_in_pieces_with_interruptions() {
    run(async {
        let want = vec![b"abcdefgh".to_vec(), b"ijk".to_vec()];
        let mut r = FrameReader::new(slow_sender(encode(&want), 3));
        check!("3 bytes every 10 ms, next dropped every 3 ms", read_with_interruptions(&mut r).await, Ok(want));
    });
}

#[test]
fn header_split_in_two() {
    run(async {
        let want = vec![vec![7u8; 300]];
        let mut r = FrameReader::new(slow_sender(encode(&want), 1));
        check!("one byte at a time", read_with_interruptions(&mut r).await, Ok(want));
    });
}

#[test]
fn truncated_stream_is_an_error() {
    run(async {
        let mut r = FrameReader::new(&[0u8, 5, 1, 2][..]);
        check!("2 of 5 payload bytes", r.next().await.map_err(|e| e.kind()), Err(io::ErrorKind::UnexpectedEof));
    });
}

#[test]
fn pump_with_ticks_delivers_every_frame() {
    run(async {
        let want = vec![b"first frame".to_vec(), b"second".to_vec(), b"third one".to_vec()];
        let stream = slow_sender(encode(&want), 4);
        let (tick, ticks) = mpsc::channel(1);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(7)).await;
                if tick.send(()).await.is_err() {
                    return;
                }
            }
        });
        let mut out = Vec::new();
        let result = tokio::time::timeout(Duration::from_secs(60), pump(stream, ticks, &mut out)).await.expect("pump never returned");
        let frames: Vec<Vec<u8>> = out.iter().filter_map(|e| if let Event::Frame(f) = e { Some(f.clone()) } else { None }).collect();
        check!("pump result", result.map_err(|e| e.kind()), Ok(()));
        check!("frames seen by pump", frames, want);
        assert!(out.contains(&Event::Tick), "ticks must still be handled");
    });
}
