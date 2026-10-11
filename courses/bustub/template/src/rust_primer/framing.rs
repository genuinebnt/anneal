//! Frames from a stream of bytes that arrives in pieces: a 2-byte big-endian length, then that many bytes.

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A frame announced more payload bytes than the decoder accepts.
    TooLong { len: usize },
}

pub struct FrameDecoder {
    _frames: (),
}

impl FrameDecoder {
    /// A decoder with nothing buffered. A frame longer than `max_frame` bytes is refused.
    pub fn new(max_frame: usize) -> FrameDecoder {
        todo!("r-c4: a decoder with nothing buffered")
    }

    /// Adds the next piece of the stream; returns the frames (payloads) that are complete now.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, FrameError> {
        todo!("r-c4: take every whole frame off the front of the buffer; keep the rest")
    }

    /// Bytes held that are not yet part of a returned frame.
    pub fn buffered(&self) -> usize {
        todo!("r-c4: how many bytes are waiting")
    }
}
