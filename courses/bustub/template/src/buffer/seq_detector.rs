//! Spotting a sequential scan and suggesting pages to read ahead.

pub struct SeqDetector {
    _seq: (),
}

impl SeqDetector {
    /// A detector that has seen no access. A run of `trigger` consecutive pages (at least 1) starts read-ahead of `depth` pages.
    pub fn new(trigger: u32, depth: u32) -> SeqDetector {
        todo!("1f-c4: a detector that has seen nothing")
    }

    /// The client read `page`; returns the pages worth prefetching now.
    pub fn access(&mut self, page: u32) -> Vec<u32> {
        todo!("1f-c4: track the run; when it is long enough, suggest the pages not suggested yet up to `depth` ahead")
    }
}
