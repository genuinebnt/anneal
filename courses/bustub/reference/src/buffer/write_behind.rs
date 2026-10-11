//! Which dirty pages are due for a background flush.

use std::collections::BTreeMap;

pub struct WriteBehind {
    // @begin 1f-c3
    /// page -> the time it first became dirty (since its last flush).
    dirty: BTreeMap<u32, u64>,
    //~ _wb: (),
    // @end
}

impl WriteBehind {
    /// A tracker with no dirty page.
    pub fn new() -> WriteBehind {
        // @begin 1f-c3
        WriteBehind { dirty: BTreeMap::new() }
        //~ todo!("1f-c3: nothing is dirty")
        // @end
    }

    /// Records that `page` became dirty at `now`. A page that is already dirty keeps its earlier time.
    pub fn mark_dirty(&mut self, page: u32, now: u64) {
        // @begin 1f-c3
        self.dirty.entry(page).or_insert(now);
        //~ todo!("1f-c3: remember when the page first became dirty")
        // @end
    }

    /// Pages dirty for at least `max_age` as of `now`, oldest first (ties by page number).
    pub fn due(&self, now: u64, max_age: u64) -> Vec<u32> {
        // @begin 1f-c3
        let mut v: Vec<(u64, u32)> = self.dirty.iter().filter(|&(_, &since)| now.saturating_sub(since) >= max_age).map(|(&p, &since)| (since, p)).collect();
        v.sort();
        v.into_iter().map(|(_, p)| p).collect()
        //~ todo!("1f-c3: the pages that have been dirty long enough, oldest first")
        // @end
    }

    /// Forgets `page` once it is written back; returns whether it was dirty.
    pub fn flushed(&mut self, page: u32) -> bool {
        // @begin 1f-c3
        self.dirty.remove(&page).is_some()
        //~ todo!("1f-c3: forget the page")
        // @end
    }

    /// The number of dirty pages.
    pub fn dirty_count(&self) -> usize {
        // @begin 1f-c3
        self.dirty.len()
        //~ todo!("1f-c3: how many pages are dirty")
        // @end
    }

    /// The page that has been dirty longest, with the time it became dirty.
    pub fn oldest(&self) -> Option<(u32, u64)> {
        // @begin 1f-c3
        self.dirty.iter().map(|(&p, &s)| (s, p)).min().map(|(s, p)| (p, s))
        //~ todo!("1f-c3: the oldest dirty page")
        // @end
    }
}

impl Default for WriteBehind {
    fn default() -> Self {
        WriteBehind::new()
    }
}
