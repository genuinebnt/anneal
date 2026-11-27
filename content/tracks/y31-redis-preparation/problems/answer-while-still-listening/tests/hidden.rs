use solution::*;


use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// The clock is paused, so a wait that nothing can end takes no real time.
#[allow(dead_code)]
fn run<F: std::future::Future<Output = ()>>(body: F) {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap();
    runtime.block_on(async {
        timeout(Duration::from_secs(600), body).await.expect("the test hung");
    });
}

/// Longer than anything correct needs while the clock is paused.
#[allow(dead_code)]
const SOON: Duration = Duration::from_secs(5);

#[allow(dead_code)]
struct Rig {
    client: DuplexStream,
    work: mpsc::Receiver<Batch>,
    notices: mpsc::Sender<String>,
    kill: CancellationToken,
    task: JoinHandle<Ending>,
}

/// A connection over an in-memory pipe that holds `pipe` bytes each way.
#[allow(dead_code)]
fn rig_with(pipe: usize, max_line: usize) -> Rig {
    let (client, server) = tokio::io::duplex(pipe);
    let (work_tx, work) = mpsc::channel(1);
    let (notices, notices_rx) = mpsc::channel(16);
    let kill = CancellationToken::new();
    let task = tokio::spawn(serve(server, work_tx, notices_rx, kill.clone(), max_line));
    Rig { client, work, notices, kill, task }
}
#[allow(dead_code)]
fn rig() -> Rig {
    rig_with(4096, 64)
}

#[allow(dead_code)]
impl Rig {
    async fn send(&mut self, text: &str) {
        timeout(SOON, self.client.write_all(text.as_bytes())).await.expect("the server stopped reading").expect("write");
    }
    /// The next batch the connection gives the worker.
    async fn batch(&mut self) -> Batch {
        timeout(SOON, self.work.recv()).await.expect("no batch arrived").expect("the connection dropped the work channel")
    }
    /// Answers the next batch with each line in upper case, and returns the lines.
    async fn answer(&mut self) -> Vec<String> {
        let batch = self.batch().await;
        let replies = batch.lines.iter().map(|l| l.to_uppercase()).collect();
        let _ = batch.reply.send(replies);
        batch.lines
    }
    /// Reads exactly this text from the server.
    async fn expect(&mut self, text: &str) {
        let mut got = vec![0; text.len()];
        let read = timeout(SOON, self.client.read_exact(&mut got)).await;
        assert!(matches!(read, Ok(Ok(_))), "expected {text:?}, but the stream gave only part of it or ended");
        assert_eq!(String::from_utf8_lossy(&got), text);
    }
    /// Checks that nothing arrives from the server while nothing else happens.
    async fn silence(&mut self) {
        let mut byte = [0];
        let read = timeout(SOON, self.client.read(&mut byte)).await;
        assert!(read.is_err(), "expected nothing, got {read:?} {byte:?}");
    }
    async fn ending(&mut self) -> Ending {
        timeout(SOON, &mut self.task).await.expect("the connection did not end").expect("it panicked")
    }
}

#[allow(dead_code)]
fn words(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_line_a_byte_at_a_time() {
    run(async {
        let mut r = rig();
        for byte in "hello".chars() {
            r.send(&byte.to_string()).await;
            tokio::task::yield_now().await;
        }
        r.silence().await;
        check!("no batch without the newline", r.work.try_recv().is_err(), true);
        r.send("\n").await;
        check!("batch", r.answer().await, words(&["hello"]));
    });
}

#[test]
fn the_client_leaving_when_idle() {
    run(async {
        let mut r = rig();
        r.send("unfinished").await;
        r.client.shutdown().await.unwrap();
        check!("ending", r.ending().await, Ending::ClientLeft);
        check!("no batch", r.work.try_recv().is_err(), true);
    });
}

#[test]
fn the_longest_line_and_one_longer() {
    run(async {
        let mut r = rig_with(4096, 8);
        r.send("12345678\n").await;
        check!("8 is fine", r.answer().await, words(&["12345678"]));
        r.expect("12345678\n").await;
        r.send("123456789\n").await;
        r.expect("ERR too long\n").await;
        check!("ending", r.ending().await, Ending::TooLong);
        check!("it never reached the worker", r.work.try_recv().is_err(), true);
    });
}

#[test]
fn too_long_is_found_without_a_newline() {
    run(async {
        let mut r = rig_with(4096, 8);
        r.send("123456789").await;
        r.expect("ERR too long\n").await;
        let mut rest = Vec::new();
        timeout(SOON, r.client.read_to_end(&mut rest)).await.expect("the stream ends").unwrap();
        check!("nothing more", rest, Vec::<u8>::new());
        check!("ending", r.ending().await, Ending::TooLong);
    });
}

#[test]
fn a_busy_worker_stops_the_reading_after_a_while() {
    run(async {
        let mut r = rig_with(64, 100);
        r.send("first\n").await;
        let batch = r.batch().await;
        // the pipe holds 64 bytes; the connection may buffer a few kilobytes at most
        let flood = "x\n".repeat(50_000);
        let blocked = timeout(SOON, r.client.write_all(flood.as_bytes())).await.is_err();
        check!("100000 bytes could not all be written while the worker was busy", blocked, true);
        drop(batch);
        check!("a dropped reply ends the connection", r.ending().await, Ending::WorkerGone);
    });
}

#[test]
fn lines_sent_while_busy_are_not_lost() {
    run(async {
        let mut r = rig_with(16, 32);
        r.send("0\n").await;
        let batch = r.batch().await;
        let (mut from, mut to) = tokio::io::split(std::mem::replace(&mut r.client, tokio::io::duplex(1).0));
        let writer = tokio::spawn(async move {
            for i in 1..300 {
                to.write_all(format!("{i}\n").as_bytes()).await.unwrap();
            }
            to
        });
        let reader = tokio::spawn(async move {
            let mut got = String::new();
            let mut buf = [0; 64];
            while got.matches('\n').count() < 300 {
                let n = from.read(&mut buf).await.unwrap();
                assert!(n > 0, "the stream ended after {got:?}");
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            got
        });
        tokio::time::sleep(SOON).await;
        let mut seen = batch.lines.clone();
        batch.reply.send(batch.lines.clone()).unwrap();
        while seen.len() < 300 {
            let batch = r.batch().await;
            seen.extend(batch.lines.clone());
            batch.reply.send(batch.lines.clone()).unwrap();
        }
        let want: Vec<String> = (0..300).map(|i| i.to_string()).collect();
        check!("every line, in order", seen, want.clone());
        let got = timeout(SOON, reader).await.expect("every reply").unwrap();
        check!("every reply, in order", got, want.join("\n") + "\n");
        drop(writer.await.unwrap());
    });
}

#[test]
fn empty_replies_are_allowed() {
    run(async {
        let mut r = rig();
        r.send("quiet\n").await;
        r.batch().await.reply.send(vec![]).unwrap();
        r.send("loud\n").await;
        r.batch().await.reply.send(words(&["a", "b", "c"])).unwrap();
        r.expect("a\nb\nc\n").await;
    });
}

#[test]
fn a_closed_notice_channel_changes_nothing() {
    run(async {
        let mut r = rig();
        let (spare, _) = mpsc::channel(1);
        drop(std::mem::replace(&mut r.notices, spare));
        r.send("a\n").await;
        r.answer().await;
        r.expect("A\n").await;
        r.send("b\n").await;
        r.answer().await;
        r.expect("B\n").await;
    });
}

#[test]
fn a_worker_that_is_gone() {
    run(async {
        let mut r = rig();
        r.work.close();
        r.send("a\n").await;
        check!("ending", r.ending().await, Ending::WorkerGone);
    });
}

#[test]
fn kill_while_waiting_for_the_worker() {
    run(async {
        let mut r = rig();
        r.send("a\n").await;
        let _batch = r.batch().await;
        r.kill.cancel();
        check!("ending", r.ending().await, Ending::Killed);
    });
}

#[test]
fn kill_while_the_work_channel_is_full() {
    run(async {
        let (client, server) = tokio::io::duplex(1024);
        let (work, _held) = mpsc::channel(1);
        let (reply, _keep) = tokio::sync::oneshot::channel();
        work.try_send(Batch { lines: vec![], reply }).ok().expect("room for one");
        let (_notices, notices_rx) = mpsc::channel(1);
        let kill = CancellationToken::new();
        let task = tokio::spawn(serve(server, work, notices_rx, kill.clone(), 16));
        let mut client = client;
        client.write_all(b"a\n").await.unwrap();
        tokio::time::sleep(SOON).await;
        kill.cancel();
        let ending = timeout(SOON, task).await.expect("kill must work while a send is stuck").unwrap();
        check!("ending", ending, Ending::Killed);
    });
}

#[test]
fn kill_while_a_write_is_stuck() {
    run(async {
        // the client reads nothing, and the pipe holds 8 bytes
        let mut r = rig_with(8, 64);
        r.send("a\n").await;
        r.batch().await.reply.send(vec!["long reply ".repeat(50)]).unwrap();
        tokio::time::sleep(SOON).await;
        r.kill.cancel();
        check!("ending", r.ending().await, Ending::Killed);
    });
}

#[test]
fn kill_while_a_notice_is_stuck() {
    run(async {
        let mut r = rig_with(8, 64);
        r.notices.send("n".repeat(100)).await.unwrap();
        tokio::time::sleep(SOON).await;
        r.kill.cancel();
        check!("ending", r.ending().await, Ending::Killed);
    });
}

/// A stream whose reads or writes fail with a given error.
#[allow(dead_code)]
struct Faulty {
    input: Vec<u8>,
    read_error: Option<io::ErrorKind>,
    write_error: Option<io::ErrorKind>,
}
impl AsyncRead for Faulty {
    fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.input.is_empty() {
            return match self.read_error {
                Some(kind) => Poll::Ready(Err(kind.into())),
                None => Poll::Pending,
            };
        }
        let n = self.input.len().min(buf.remaining());
        let taken: Vec<u8> = self.input.drain(..n).collect();
        buf.put_slice(&taken);
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for Faulty {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.write_error {
            Some(kind) => Poll::Ready(Err(kind.into())),
            None => Poll::Ready(Ok(buf.len())),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[test]
fn stream_errors_are_reported() {
    run(async {
        let reset = io::ErrorKind::ConnectionReset;
        let broken = io::ErrorKind::BrokenPipe;
        for (stream, want) in [
            (Faulty { input: vec![], read_error: Some(reset), write_error: None }, reset),
            (Faulty { input: b"a\n".to_vec(), read_error: None, write_error: Some(broken) }, broken),
        ] {
            let (work, mut jobs) = mpsc::channel::<Batch>(1);
            let worker = tokio::spawn(async move {
                while let Some(b) = jobs.recv().await {
                    let _ = b.reply.send(b.lines.clone());
                }
            });
            let (_notices, notices_rx) = mpsc::channel(1);
            let ending = timeout(SOON, serve(stream, work, notices_rx, CancellationToken::new(), 16)).await.expect("it ends");
            check!("ending", ending, Ending::Io(want));
            worker.abort();
        }
    });
}

#[test]
fn many_notices_do_not_disturb_a_slow_line() {
    run(async {
        let mut r = rig();
        let text = "abcdefghijklmnop";
        for (i, c) in text.chars().enumerate() {
            r.send(&c.to_string()).await;
            r.notices.send(format!("n{i}")).await.unwrap();
            r.expect(&format!("n{i}\n")).await;
        }
        r.send("\n").await;
        check!("the line is whole", r.answer().await, words(&[text]));
    });
}
