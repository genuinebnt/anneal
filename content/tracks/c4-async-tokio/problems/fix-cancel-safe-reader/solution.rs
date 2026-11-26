use tokio::io::{self, AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

pub struct FrameReader<R> {
    reader: R,
    /// Bytes received and not yet returned. All progress lives here, so dropping `next` loses nothing.
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        FrameReader {
            reader,
            buf: Vec::new(),
        }
    }

    /// The next frame, or `None` at a clean end of stream.
    pub async fn next(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            if let Some(frame) = self.take_frame() {
                return Ok(Some(frame));
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

    fn take_frame(&mut self) -> Option<Vec<u8>> {
        let header = self.buf.get(..2)?;
        let end = 2 + u16::from_be_bytes([header[0], header[1]]) as usize;
        let payload = self.buf.get(2..end)?.to_vec();
        self.buf.drain(..end);
        Some(payload)
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
