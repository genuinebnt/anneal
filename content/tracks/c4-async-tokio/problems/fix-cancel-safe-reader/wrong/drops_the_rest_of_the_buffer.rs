use tokio::io::{self, AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

pub struct FrameReader<R> {
    reader: R,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        FrameReader {
            reader,
            buf: Vec::new(),
        }
    }

    pub async fn next(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            if self.buf.len() >= 2 {
                let end = 2 + u16::from_be_bytes([self.buf[0], self.buf[1]]) as usize;
                if self.buf.len() >= end {
                    let payload = self.buf[2..end].to_vec();
                    self.buf.clear();
                    return Ok(Some(payload));
                }
            }
            self.buf.reserve(1024);
            if self.reader.read_buf(&mut self.buf).await? == 0 {
                return if self.buf.is_empty() {
                    Ok(None)
                } else {
                    Err(io::ErrorKind::UnexpectedEof.into())
                };
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Tick,
    Frame(Vec<u8>),
}

pub async fn pump<R: AsyncRead + Unpin>(
    reader: R,
    mut ticks: mpsc::Receiver<()>,
    out: &mut Vec<Event>,
) -> io::Result<()> {
    let mut frames = FrameReader::new(reader);
    loop {
        tokio::select! {
            Some(()) = ticks.recv() => out.push(Event::Tick),
            frame = frames.next() => match frame? {
                Some(payload) => out.push(Event::Frame(payload)),
                None => return Ok(()),
            },
        }
    }
}
