from y17_parts.common import broken

TYPES = r'''/// Complete lines for the worker, and where its replies go.
pub struct Batch {
    pub lines: Vec<String>,
    pub reply: oneshot::Sender<Vec<String>>,
}

/// Why a connection ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ending {
    ClientLeft,
    TooLong,
    Killed,
    WorkerGone,
    Io(io::ErrorKind),
}
'''

SOLUTION = r'''use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

''' + TYPES + r'''
/// Takes every complete line off the front of `input`.
fn take_lines(input: &mut Vec<u8>, max_line: usize) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut too_long = false;
    while let Some(end) = input[start..].iter().position(|&b| b == b'\n') {
        if end > max_line {
            too_long = true;
            break;
        }
        lines.push(String::from_utf8_lossy(&input[start..start + end]).into_owned());
        start += end + 1;
    }
    input.drain(..start);
    too_long |= input.len() > max_line && !input[..=max_line].contains(&b'\n');
    (lines, too_long)
}

/// Serves one connection until it ends.
pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    work: mpsc::Sender<Batch>,
    notices: mpsc::Receiver<String>,
    kill: CancellationToken,
    max_line: usize,
) -> Ending {
    // cancelling drops whatever the connection was waiting for, a stuck write included
    tokio::select! {
        biased;
        _ = kill.cancelled() => Ending::Killed,
        ending = connection(stream, work, notices, max_line) => ending,
    }
}

async fn connection<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    work: mpsc::Sender<Batch>,
    mut notices: mpsc::Receiver<String>,
    max_line: usize,
) -> Ending {
    let (mut read, mut write) = tokio::io::split(stream);
    let mut input: Vec<u8> = Vec::new();
    let mut more_notices = true;
    loop {
        let (lines, too_long) = take_lines(&mut input, max_line);
        if !lines.is_empty() {
            let (reply, mut answer) = oneshot::channel();
            if work.send(Batch { lines, reply }).await.is_err() {
                return Ending::WorkerGone;
            }
            let replies = loop {
                input.reserve(1024);
                // keep reading while the worker is busy, but only so far
                let room = input.len() <= max_line;
                tokio::select! {
                    answered = &mut answer => match answered {
                        Ok(replies) => break replies,
                        Err(_) => return Ending::WorkerGone,
                    },
                    read = read.read_buf(&mut input), if room => match read {
                        Ok(0) => return Ending::ClientLeft,
                        Ok(_) => {}
                        Err(e) => return Ending::Io(e.kind()),
                    },
                }
            };
            let text: String = replies.iter().map(|r| format!("{r}\n")).collect();
            if let Err(e) = write.write_all(text.as_bytes()).await {
                return Ending::Io(e.kind());
            }
            if let Err(e) = write.flush().await {
                return Ending::Io(e.kind());
            }
            // what came in meanwhile is the next batch, after any notices that are waiting
            while let Ok(notice) = notices.try_recv() {
                if let Err(e) = write.write_all(format!("{notice}\n").as_bytes()).await {
                    return Ending::Io(e.kind());
                }
            }
            if !too_long {
                continue;
            }
        }
        if too_long {
            let _ = write.write_all(b"ERR too long\n").await;
            let _ = write.shutdown().await;
            return Ending::TooLong;
        }
        input.reserve(1024);
        tokio::select! {
            read = read.read_buf(&mut input) => match read {
                Ok(0) => return Ending::ClientLeft,
                Ok(_) => {}
                Err(e) => return Ending::Io(e.kind()),
            },
            notice = notices.recv(), if more_notices => match notice {
                Some(notice) => {
                    if let Err(e) = write.write_all(format!("{notice}\n").as_bytes()).await {
                        return Ending::Io(e.kind());
                    }
                }
                None => more_notices = false,
            },
        }
    }
}
'''

STARTER = r'''use std::io;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

''' + TYPES + r'''
/// Serves one connection until it ends.
pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    work: mpsc::Sender<Batch>,
    notices: mpsc::Receiver<String>,
    kill: CancellationToken,
    max_line: usize,
) -> Ending {
    todo!()
}
'''

HELPERS = r'''
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
'''

VISIBLE = [
    HELPERS,
    r'''
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
''',
]

HIDDEN = [
    HELPERS,
    r'''
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
''',
]

STATEMENT = r'''
A server connection reads commands from a client, hands them to a worker, and writes back the answers. The server also has news to send at any moment. The hard part is what happens **while** the worker is busy: clients leave, flood, stop reading, or must be cut off.

Write `serve(stream, work, notices, kill, max_line)`. It serves one connection and returns why it ended.

Lines end with `\n`, which is not part of the line. The tests send UTF-8; use `String::from_utf8_lossy`. `max_line` is at least 1.

**Batches.** Every complete line that has arrived goes to the worker in **one** `Batch`, in order. At most **one batch** is outstanding. When its replies come, write each followed by `\n`.

A batch of lines may get any number of replies. Lines that arrived meanwhile form the next batch. Send a batch as soon as you can; never wait for more input first.

**While a batch is outstanding:**

- Keep reading, so that a client that leaves is noticed **at once**. Then return `ClientLeft`, without waiting for the worker.
- But do not buffer without end: once you hold more than `max_line` bytes of unsent lines, stop reading until the replies are written. (You may hold a few kilobytes more, from the last read.)

**Notices.** Each string from `notices` is written followed by `\n`, as soon as no batch is outstanding. A batch's replies are written together, with no notice between them.

A notice that comes during a batch goes after its replies. When the notice channel closes, carry on without it.

**Long lines.** A line longer than `max_line` bytes ends the connection, **whether its `\n` has arrived or not**. First deliver the replies to the complete lines before it. Then write `ERR too long\n` and return `TooLong`.

**Ending.**

| Return | When |
|---|---|
| `ClientLeft` | a read returns end of stream (an unfinished line is dropped) |
| `Killed` | `kill` is cancelled: return **at once**, whatever you are waiting for, even a write to a client that is not reading |
| `WorkerGone` | the `work` channel is closed, or a batch's reply sender is dropped |
| `Io(kind)` | a read or write fails |

Waiting for input must not lose bytes when something else happens first. The tests use in-memory streams on a runtime whose clock is paused, so no part of your answer may depend on timeouts.
'''

PROBLEM = dict(
    slug="answer-while-still-listening", title="Answer while still listening", level="hard", stage="network",
    tags=["tokio select", "cancel safety", "backpressure", "pipelining", "cancellation"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    crates=["tokio", "tokio-util"],
    teaches=[
        "While one thing is pending, a connection still has other things to watch; `select!` waits on all of them.",
        "A branch precondition on the read is backpressure: stop reading and the sender must wait.",
        "Wrapping the whole connection in one `select!` with the kill token cancels any stuck step.",
    ],
    follow_up="1) Which futures in your loop are dropped by `select!`, and why is each safe to drop? 2) With `biased;`, what could starve? 3) How would you give the worker a deadline without losing the connection's state?",
    hints=[
        ("approach", "Write the idle state as one wait on two things: bytes or a notice. Write the busy state as another: the reply, or more bytes if there is room. Then think about where cancellation can sit so that it covers every await at once."),
        ("rust", "`read_buf` is cancel-safe: dropped, it has read nothing. `tokio::io::split` lets you write while a read branch exists. A `select!` branch takes `, if condition`. `&mut receiver` lets you poll one oneshot again and again."),
        ("edge", "Check line lengths both for complete lines and for the unfinished rest. A `recv()` on a closed channel completes at once, every time: switch that branch off."),
    ],
    notes=("The connection alternates between two waits: idle, on input or a notice; busy, on the reply or, while there is room, more input. The whole loop runs inside a `select!` with the kill token, so cancelling drops whatever await is in progress.",
           "O(bytes)", "O(max_line + one read)"),
    wrong={
        "waits_for_the_reply_without_reading": broken(SOLUTION, ("                let room = input.len() <= max_line;", "                let room = false;")),
        "reads_without_bound_while_busy": broken(SOLUTION, ("                let room = input.len() <= max_line;", "                let room = true;")),
        "notices_during_a_batch": broken(SOLUTION, (
            "                    read = read.read_buf(&mut input), if room => match read {",
            "                    Some(notice) = notices.recv() => {\n                        if let Err(e) = write.write_all(format!(\"{notice}\\n\").as_bytes()).await {\n                            return Ending::Io(e.kind());\n                        }\n                    }\n                    read = read.read_buf(&mut input), if room => match read {")),
        "kill_checked_only_while_idle": broken(SOLUTION, (
            "    tokio::select! {\n        biased;\n        _ = kill.cancelled() => Ending::Killed,\n        ending = connection(stream, work, notices, max_line) => ending,\n    }",
            "    connection(stream, work, notices, max_line, kill).await"),
            ("    mut notices: mpsc::Receiver<String>,\n    max_line: usize,\n) -> Ending {\n    let (mut read",
             "    mut notices: mpsc::Receiver<String>,\n    max_line: usize,\n    kill: CancellationToken,\n) -> Ending {\n    let (mut read"),
            ("        input.reserve(1024);\n        tokio::select! {\n            read = read.read_buf(&mut input) => match read {",
             "        input.reserve(1024);\n        tokio::select! {\n            _ = kill.cancelled() => return Ending::Killed,\n            read = read.read_buf(&mut input) => match read {")),
        "long_lines_only_without_newline": broken(SOLUTION, (
            "        if end > max_line {\n            too_long = true;\n            break;\n        }\n", "")),
        "one_line_per_batch": broken(SOLUTION, (
            "        lines.push(String::from_utf8_lossy(&input[start..start + end]).into_owned());\n        start += end + 1;\n    }",
            "        lines.push(String::from_utf8_lossy(&input[start..start + end]).into_owned());\n        start += end + 1;\n        break;\n    }")),
    },
)
