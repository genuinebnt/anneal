/// Rebuilds an in-order byte stream from TCP-like segments that may arrive late, twice, overlapping, or not at all.
/// Stream offset 0 has sequence number `isn`; sequence numbers are 32-bit and wrap around.
pub struct Reassembler {
    // TODO: next offset to deliver, out-of-order chunks keyed by offset, bytes ready to read, the FIN position
}

impl Reassembler {
    /// `window`: how many bytes ahead of the next expected one you are willing to buffer.
    pub fn new(isn: u32, window: usize) -> Self {
        todo!()
    }

    /// A segment starting at sequence number `seq`. `fin` marks the end of the stream (just after this segment's data).
    /// Bytes outside `[next, next + window)` are dropped (the sender retransmits); where segments overlap, the bytes
    /// that arrived first win.
    pub fn segment(&mut self, seq: u32, data: &[u8], fin: bool) {
        todo!()
    }

    /// Takes every byte that is in order and not yet read.
    pub fn read(&mut self) -> Vec<u8> {
        todo!()
    }

    /// True once the FIN has been seen and every byte before it has been handed out.
    pub fn is_finished(&self) -> bool {
        todo!()
    }

    /// Bytes held out of order, waiting for an earlier gap to fill.
    pub fn buffered(&self) -> usize {
        todo!()
    }
}
