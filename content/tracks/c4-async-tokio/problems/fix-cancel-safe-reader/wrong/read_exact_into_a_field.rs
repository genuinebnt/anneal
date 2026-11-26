use tokio::io::{self, AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

pub struct FrameReader<R> {
    reader: R,
    header: [u8; 2],
    have_header: bool,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        FrameReader {
            reader,
            header: [0; 2],
            have_header: false,
        }
    }

    /// Remembers a finished header, but `read_exact` still loses its own progress when dropped.
    pub async fn next(&mut self) -> io::Result<Option<Vec<u8>>> {
        if !self.have_header {
            if self.reader.read(&mut self.header[..1]).await? == 0 {
                return Ok(None);
            }
            self.reader.read_exact(&mut self.header[1..]).await?;
            self.have_header = true;
        }
        let mut payload = vec![0u8; u16::from_be_bytes(self.header) as usize];
        self.reader.read_exact(&mut payload).await?;
        self.have_header = false;
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
