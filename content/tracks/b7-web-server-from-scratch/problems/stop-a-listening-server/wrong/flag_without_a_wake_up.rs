use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// How long a read may block before the connection looks at the flag again.
const SLICE: Duration = Duration::from_millis(20);

type Answer = Arc<dyn Fn(&str) -> String + Send + Sync>;

pub struct Server {
    addr: SocketAddr,
    stopping: Arc<AtomicBool>,
    /// Returns the number of connections accepted. `None` once joined.
    acceptor: Option<JoinHandle<usize>>,
}

fn serve_client(mut stream: TcpStream, answer: &Answer, stopping: &AtomicBool) -> io::Result<()> {
    stream.set_read_timeout(Some(SLICE))?;
    let mut pending = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            if stopping.load(Ordering::SeqCst) {
                break;
            }
            let mut reply = answer(&String::from_utf8_lossy(&pending[..end]));
            reply.push('\n');
            stream.write_all(reply.as_bytes())?;
            pending.drain(..=end);
        }
        // looked at on every turn, so a client that never stops sending cannot hold the connection open
        if stopping.load(Ordering::SeqCst) {
            stream.shutdown(Shutdown::Write)?;
            // read away what is already here, so that closing does not reset the connection
            for _ in 0..64 {
                if !matches!(stream.read(&mut chunk), Ok(n) if n > 0) {
                    break;
                }
            }
            return Ok(());
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(e) => return Err(e),
        }
    }
}

fn accept(listener: TcpListener, answer: Answer, stopping: Arc<AtomicBool>) -> usize {
    let mut clients: Vec<JoinHandle<()>> = Vec::new();
    let mut accepted = 0;
    for stream in listener.incoming() {
        if stopping.load(Ordering::SeqCst) {
            break; // this is the connection `stop` made to wake us, or one that came too late
        }
        let Ok(stream) = stream else {
            continue;
        };
        accepted += 1;
        let (answer, stopping) = (answer.clone(), stopping.clone());
        clients.push(std::thread::spawn(move || {
            let _ = serve_client(stream, &answer, &stopping);
        }));
        // finished threads need no joining later
        clients.retain(|c| !c.is_finished());
    }
    // closed first, so that new connections are refused while we wait for the old ones
    drop(listener);
    for client in clients {
        let _ = client.join();
    }
    accepted
}

impl Server {
    pub fn start(answer: impl Fn(&str) -> String + Send + Sync + 'static) -> io::Result<Server> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let stopping = Arc::new(AtomicBool::new(false));
        let (answer, flag): (Answer, _) = (Arc::new(answer), stopping.clone());
        let acceptor = std::thread::spawn(move || accept(listener, answer, flag));
        Ok(Server { addr, stopping, acceptor: Some(acceptor) })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn stop(mut self) -> usize {
        self.finish()
    }

    fn finish(&mut self) -> usize {
        let Some(acceptor) = self.acceptor.take() else {
            return 0;
        };
        self.stopping.store(true, Ordering::SeqCst);
        // `accept` is blocked and only a connection wakes it
        acceptor.join().unwrap_or(0)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.finish();
    }
}
