/// The stream can never be decoded again: a frame announced more than the allowed size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge { len: usize, max: usize },
}

/// Appends `[len: u32 big-endian][payload]` to `out`.
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) {
    todo!()
}

/// Turns arbitrary chunks of a byte stream (as `read` returns them) back into whole frames.
pub struct Decoder {
    // TODO: the bytes received so far, and the largest frame you accept
}

impl Decoder {
    pub fn new(max_frame: usize) -> Self {
        todo!()
    }

    pub fn feed(&mut self, data: &[u8]) {
        todo!()
    }

    /// The next complete frame's payload, `Ok(None)` if more bytes are needed, or an error for an oversized length.
    /// The length is checked as soon as its four bytes are here, before any payload is buffered.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        todo!()
    }

    /// Bytes held that do not yet form a frame.
    pub fn buffered(&self) -> usize {
        todo!()
    }
}
