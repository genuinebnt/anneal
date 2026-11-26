use std::io::{self, ErrorKind, Read, Write};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// How long a connection may wait for the first byte of its next line.
    pub idle: Duration,
    /// How long a line may take, from its first byte to its end.
    pub line: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    ClientLeft,
    Idle,
    TooSlow,
}

fn send<S: Write>(stream: &mut S, parts: &[&[u8]]) -> io::Result<()> {
    for part in parts {
        stream.write_all(part)?;
    }
    stream.flush()
}

pub fn serve<S: Read + Write>(stream: &mut S, now: impl Fn() -> Duration, limits: Limits) -> io::Result<Ending> {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 256];
    // two different moments: when the connection became idle, and when the unfinished line began
    let mut idle_since = now();
    let mut line_began: Option<Duration> = None;
    let mut last_arrival = idle_since;
    loop {
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            send(stream, &[b"ok ", &pending[..=end]])?;
            pending.drain(..=end);
            idle_since = now();
            // whatever follows a finished line arrived with the latest read
            line_began = (!pending.is_empty()).then_some(last_arrival);
        }
        // checked on every turn: a client that keeps sending never lets a read come back empty
        let time = now();
        match line_began {
            Some(began) if time.saturating_sub(began) >= limits.line => {
                send(stream, &[b"err too slow\n"])?;
                return Ok(Ending::TooSlow);
            }
            None if time.saturating_sub(idle_since) >= limits.idle => return Ok(Ending::Idle),
            _ => {}
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(Ending::ClientLeft),
            Ok(n) => {
                last_arrival = now();
                line_began.get_or_insert(last_arrival);
                pending.extend_from_slice(&chunk[..n]);
            }
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(e) => return Err(e),
        }
    }
}
