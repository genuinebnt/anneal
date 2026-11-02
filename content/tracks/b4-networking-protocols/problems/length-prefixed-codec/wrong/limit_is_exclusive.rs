/// The stream can never be decoded again: a frame announced more than the allowed size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge { len: usize, max: usize },
}

/// Appends `[len: u32 big-endian][payload]` to `out`.
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}

/// Turns arbitrary chunks of a byte stream (as `read` returns them) back into whole frames.
pub struct Decoder {
    buf: Vec<u8>,
    max: usize,
}

impl Decoder {
    pub fn new(max_frame: usize) -> Self {
        Decoder { buf: Vec::new(), max: max_frame }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// The next complete frame's payload, `Ok(None)` if more bytes are needed, or an error for an oversized length.
    /// The length is checked as soon as its four bytes are here, before any payload is buffered.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes(self.buf[..4].try_into().unwrap()) as usize;
        if len >= self.max {
            return Err(FrameError::TooLarge { len, max: self.max });
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let frame = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Ok(Some(frame))
    }

    /// Bytes held that do not yet form a frame.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}
