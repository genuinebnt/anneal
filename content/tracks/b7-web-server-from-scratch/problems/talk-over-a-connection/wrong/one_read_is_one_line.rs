use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// Closing a socket that still holds unread input makes the system reset the connection, and a reset can destroy a
/// reply the client has not read. So: end our side, then read the rest away, for a bounded time and amount.
fn close_politely(mut stream: TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut sink = [0u8; 4096];
    let mut total = 0;
    while Instant::now() < deadline && total < 1024 * 1024 {
        match stream.read(&mut sink) {
            Ok(0) | Err(_) => return,
            Ok(n) => total += n,
        }
    }
}

fn serve_client(mut stream: TcpStream, max_line: usize, handler: &dyn Fn(&str) -> String) -> io::Result<()> {
    // received and not yet answered: never more than one line's worth plus one read
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            let mut line = &pending[..end];
            line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.len() > max_line {
                break;
            }
            let mut reply = match std::str::from_utf8(line) {
                Ok(text) => handler(text),
                Err(_) => "ERR not text".to_string(),
            };
            reply.push('\n');
            // `write` may send only part of the buffer; `write_all` goes on until all of it is sent
            stream.write_all(reply.as_bytes())?;
            pending.drain(..=end);
        }
        // what is left either has no end yet, or is a complete line that is too long
        let content = match pending.iter().position(|&b| b == b'\n') {
            Some(end) => end - usize::from(end > 0 && pending[end - 1] == b'\r'),
            None => pending.len() - usize::from(pending.ends_with(b"\r")),
        };
        if content > max_line {
            stream.write_all(b"ERR too long\n")?;
            close_politely(stream);
            return Ok(());
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                pending.extend_from_slice(&chunk[..n]);
                if !pending.ends_with(b"\n") {
                    pending.push(b'\n');
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

pub fn start(max_line: usize, handler: impl Fn(&str) -> String + Send + 'static) -> io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // neither a failed connection nor a panic is a reason to stop serving others
            let _ = catch_unwind(AssertUnwindSafe(|| serve_client(stream, max_line, &handler)));
        }
    });
    Ok(addr)
}
