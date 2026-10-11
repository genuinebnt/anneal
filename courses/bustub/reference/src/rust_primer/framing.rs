//! Frames from a stream of bytes that arrives in pieces: a 2-byte big-endian length, then that many bytes.

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A frame announced more payload bytes than the decoder accepts.
    TooLong { len: usize },
}

pub struct FrameDecoder {
    // @begin r-c4
    buf: Vec<u8>,
    max_frame: usize,
    //~ _frames: (),
    // @end
}

impl FrameDecoder {
    /// A decoder with nothing buffered. A frame longer than `max_frame` bytes is refused.
    pub fn new(max_frame: usize) -> FrameDecoder {
        // @begin r-c4
        FrameDecoder { buf: Vec::new(), max_frame }
        //~ todo!("r-c4: a decoder with nothing buffered")
        // @end
    }

    /// Adds the next piece of the stream; returns the frames (payloads) that are complete now.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, FrameError> {
        // @begin r-c4
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();
        let mut at = 0;
        loop {
            let rest = &self.buf[at..];
            if rest.len() < 2 {
                break;
            }
            let len = u16::from_be_bytes([rest[0], rest[1]]) as usize;
            if len > self.max_frame {
                self.buf.clear();
                return Err(FrameError::TooLong { len });
            }
            if rest.len() < 2 + len {
                break;
            }
            frames.push(rest[2..2 + len].to_vec());
            at += 2 + len;
        }
        self.buf.drain(..at);
        Ok(frames)
        //~ todo!("r-c4: take every whole frame off the front of the buffer; keep the rest")
        // @end
    }

    /// Bytes held that are not yet part of a returned frame.
    pub fn buffered(&self) -> usize {
        // @begin r-c4
        self.buf.len()
        //~ todo!("r-c4: how many bytes are waiting")
        // @end
    }
}
