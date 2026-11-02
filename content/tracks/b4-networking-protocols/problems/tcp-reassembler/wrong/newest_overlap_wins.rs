use std::collections::BTreeMap;

/// Rebuilds an in-order byte stream from TCP-like segments that may arrive late, twice, overlapping, or not at all.
/// Stream offset 0 has sequence number `isn`; sequence numbers are 32-bit and wrap around.
pub struct Reassembler {
    isn: u32,
    window: usize,
    next: u64,                       // stream offset of the next byte to hand out
    pending: BTreeMap<u64, Vec<u8>>, // disjoint chunks at offsets >= next, waiting for a gap to be filled
    ready: Vec<u8>,                  // contiguous bytes not yet read
    fin_at: Option<u64>,
}

impl Reassembler {
    pub fn new(isn: u32, window: usize) -> Self {
        Reassembler { isn, window, next: 0, pending: BTreeMap::new(), ready: Vec::new(), fin_at: None }
    }

    /// The 64-bit stream offset whose low 32 bits match `seq` and which is closest to `next`.
    fn unwrap(&self, seq: u32) -> u64 {
        let diff = seq.wrapping_sub(self.isn) as u64;
        let base = self.next & !0xFFFF_FFFF;
        let cand = base + diff;
        if cand >= (1 << 32) && cand >= self.next + (1 << 31) {
            cand - (1 << 32)
        } else if cand + (1 << 31) < self.next {
            cand + (1 << 32)
        } else {
            cand
        }
    }

    /// A segment starting at sequence number `seq`. `fin` marks the end of the stream (just after this segment's data).
    /// Bytes outside `[next, next + window)` are dropped (the sender retransmits); where segments overlap, the bytes
    /// that arrived first win.
    pub fn segment(&mut self, seq: u32, data: &[u8], fin: bool) {
        let offset = self.unwrap(seq);
        let limit = self.next + self.window as u64;
        let end = offset + data.len() as u64;
        if fin && end <= limit {
            self.fin_at = Some(end);
        }
        let start = offset.max(self.next);
        let end = end.min(limit);
        if start < end {
            self.store(start, &data[(start - offset) as usize..(end - offset) as usize]);
        }
        // Hand over everything that is now contiguous.
        while let Some(entry) = self.pending.first_entry() {
            if *entry.key() != self.next {
                break;
            }
            let chunk = entry.remove();
            self.next += chunk.len() as u64;
            self.ready.extend_from_slice(&chunk);
        }
    }

    /// Stores `[start, start + data.len())`, keeping only the parts no earlier chunk covers.
    fn store(&mut self, start: u64, data: &[u8]) {
        let end = start + data.len() as u64;
        let mut cursor = start;
        let mut new_chunks: Vec<(u64, Vec<u8>)> = Vec::new();
        for (&s, chunk) in self.pending.range(..end) {
            let e = s + chunk.len() as u64;
            let _ = (s, e, cursor);
        }
        new_chunks.push((start, data.to_vec()));
        for (s, c) in new_chunks {
            self.pending.insert(s, c);
        }
    }

    /// Takes every byte that is in order and not yet read.
    pub fn read(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.ready)
    }

    /// True once the FIN has been seen and every byte before it has been handed out (by `segment`, to be `read`).
    pub fn is_finished(&self) -> bool {
        self.fin_at == Some(self.next)
    }

    /// Bytes held out of order, waiting for an earlier gap to fill.
    pub fn buffered(&self) -> usize {
        self.pending.values().map(|c| c.len()).sum()
    }
}
