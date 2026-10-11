//! The LRU-K policy (O'Neil, O'Neil and Weikum, SIGMOD 1993).
//!
//! Evicts the frame whose **backward k-distance** is largest: the time since its k-th most recent access. A frame with fewer than `k`
//! recorded accesses has an infinite distance, and among those the one whose *first* access is oldest goes first. A long scan cannot
//! flush the pool: scanned pages have been touched once, so they leave before pages touched twice.
//!
//! A logical clock advances by one on every recorded access. A frame is **tracked** from its first recorded access until it is evicted
//! or removed, which forget it, history included. Tracked frames start out not evictable.

// TODO(1d-01): your imports go here.

use crate::common::config::FrameId;

// TODO(1d-01): private types and helpers go here.

pub struct LruKReplacer {
    // TODO(1d-01): the fields are yours.
}

impl LruKReplacer {
    /// A replacer for frames `0..num_frames` with history length `k` (at least 1; a smaller `k` panics).
    pub fn new(num_frames: usize, k: usize) -> LruKReplacer {
        todo!("1d-01: an empty replacer; k must be at least 1")
    }

    /// Records an access to `frame` at the current time, then advances the clock. A frame seen for the first time starts out not
    /// evictable. Panics if `frame` is not in `0..num_frames`.
    pub fn record_access(&mut self, frame: FrameId) {
        todo!("1d-01: panic for a frame id out of range; remember the access and advance the clock")
    }

    /// Marks `frame` evictable or not. Frames the replacer does not track are ignored.
    pub fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        todo!("1d-01: ignore unknown frames; change the flag and keep the count of evictable frames right")
    }

    /// How many frames are evictable.
    pub fn size(&self) -> usize {
        todo!("1d-01: the number of evictable frames")
    }


    /// Evicts the frame with the largest backward k-distance among the evictable frames, forgetting its history.
    /// `None` if no frame is evictable.
    pub fn evict(&mut self) -> Option<FrameId> {
        todo!("1d-01: choose a victim among the evictable frames, forget it, and return it")
    }

    /// Removes an evictable frame and its history, whatever its distance. Unknown frames are ignored.
    /// Panics if the frame is tracked but not evictable.
    pub fn remove(&mut self, frame: FrameId) {
        todo!("1d-01: ignore unknown frames; panic for a frame that is not evictable; otherwise forget it")
    }
}

/// LRU-K ignores which page a frame holds: only the frame's own access history matters.
impl crate::buffer::replacer::FrameReplacer for LruKReplacer {
    fn record_access(&mut self, frame: FrameId, _page: crate::common::config::PageId) {
        LruKReplacer::record_access(self, frame)
    }
    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        LruKReplacer::set_evictable(self, frame, evictable)
    }
    fn evict(&mut self) -> Option<FrameId> {
        LruKReplacer::evict(self)
    }
    fn remove(&mut self, frame: FrameId) {
        LruKReplacer::remove(self, frame)
    }
    fn size(&self) -> usize {
        LruKReplacer::size(self)
    }
}
