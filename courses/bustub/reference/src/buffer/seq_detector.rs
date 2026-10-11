//! Spotting a sequential scan and suggesting pages to read ahead.

pub struct SeqDetector {
    // @begin 1f-c4
    trigger: u32,
    depth: u32,
    last: Option<u32>,
    run: u32,
    /// The highest page suggested in the current run.
    suggested_to: Option<u32>,
    //~ _seq: (),
    // @end
}

impl SeqDetector {
    /// A detector that has seen no access. A run of `trigger` consecutive pages (at least 1) starts read-ahead of `depth` pages.
    pub fn new(trigger: u32, depth: u32) -> SeqDetector {
        // @begin 1f-c4
        SeqDetector { trigger: trigger.max(1), depth, last: None, run: 0, suggested_to: None }
        //~ todo!("1f-c4: a detector that has seen nothing")
        // @end
    }

    /// The client read `page`; returns the pages worth prefetching now.
    pub fn access(&mut self, page: u32) -> Vec<u32> {
        // @begin 1f-c4
        if self.last.is_some_and(|l| l.checked_add(1) == Some(page)) {
            self.run += 1;
        } else {
            self.run = 1;
            self.suggested_to = None;
        }
        self.last = Some(page);
        if self.run < self.trigger || self.depth == 0 {
            return Vec::new();
        }
        let from = self.suggested_to.map_or(page + 1, |s| s + 1).max(page + 1);
        let to = page.saturating_add(self.depth);
        if from > to {
            return Vec::new();
        }
        self.suggested_to = Some(to);
        (from..=to).collect()
        //~ todo!("1f-c4: track the run; when it is long enough, suggest the pages not suggested yet up to `depth` ahead")
        // @end
    }
}
