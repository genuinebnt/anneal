use tokio::io::{self, AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

pub struct FrameReader<R> {
    reader: R,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        FrameReader { reader }
    }

    /// The next frame, or `None` at a clean end of stream.
    pub async fn next(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut first = [0u8; 1];
        if self.reader.read(&mut first).await? == 0 {
            return Ok(None);
        }
        let mut second = [0u8; 1];
        self.reader.read_exact(&mut second).await?;
        let len = u16::from_be_bytes([first[0], second[0]]) as usize;
        let mut payload = vec![0u8; len];
        self.reader.read_exact(&mut payload).await?;
        Ok(Some(payload))
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
