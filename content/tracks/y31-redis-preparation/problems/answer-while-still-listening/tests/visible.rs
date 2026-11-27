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
fn lines_that_are_there_go_in_one_batch() {
    run(async {
        let mut r = rig();
        r.send("one\ntwo\nthr").await;
        check!("batch", r.answer().await, words(&["one", "two"]));
        r.expect("ONE\nTWO\n").await;
        r.send("ee\n\n").await;
        check!("the rest, and an empty line", r.answer().await, words(&["three", ""]));
        r.expect("THREE\n\n").await;
    });
}

#[test]
fn one_batch_at_a_time() {
    run(async {
        let mut r = rig();
        r.send("a\n").await;
        let first = r.batch().await;
        r.send("b\n").await;
        r.send("c\n").await;
        tokio::time::sleep(SOON).await;
        check!("no second batch yet", r.work.try_recv().is_err(), true);
        first.reply.send(words(&["1"])).unwrap();
        r.expect("1\n").await;
        check!("then both", r.answer().await, words(&["b", "c"]));
    });
}

#[test]
fn notices_arrive_on_their_own_and_never_inside_a_batchs_replies() {
    run(async {
        let mut r = rig();
        r.notices.send("ping".into()).await.unwrap();
        r.expect("ping\n").await;
        r.send("a\nb\n").await;
        let batch = r.batch().await;
        r.notices.send("later".into()).await.unwrap();
        r.silence().await;
        batch.reply.send(words(&["A", "B"])).unwrap();
        r.expect("A\nB\nlater\n").await;
    });
}

#[test]
fn leaving_while_the_worker_is_busy() {
    run(async {
        let mut r = rig();
        r.send("slow\n").await;
        let batch = r.batch().await;
        r.client.shutdown().await.unwrap();
        check!("noticed at once", r.ending().await, Ending::ClientLeft);
        check!("the worker finds nobody waiting", batch.reply.send(vec![]).is_err(), true);
    });
}

#[test]
fn a_line_that_is_too_long() {
    run(async {
        let mut r = rig_with(4096, 4);
        r.send("ok\n1234\n12345").await;
        check!("the good lines", r.answer().await, words(&["ok", "1234"]));
        r.expect("OK\n1234\nERR too long\n").await;
        check!("ending", r.ending().await, Ending::TooLong);
    });
}

#[test]
fn kill() {
    run(async {
        let mut r = rig();
        r.kill.cancel();
        check!("ending", r.ending().await, Ending::Killed);
    });
}
