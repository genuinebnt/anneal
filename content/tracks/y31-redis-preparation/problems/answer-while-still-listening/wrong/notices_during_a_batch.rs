use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Complete lines for the worker, and where its replies go.
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
                    Some(notice) = notices.recv() => {
                        if let Err(e) = write.write_all(format!("{notice}\n").as_bytes()).await {
                            return Ending::Io(e.kind());
                        }
                    }
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
