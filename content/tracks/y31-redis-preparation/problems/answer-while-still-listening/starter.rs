use std::io;
use tokio::io::{AsyncRead, AsyncWrite};
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
