//! The ARC replacement policy (Megiddo and Modha, FAST 2003).
//!
//! ARC keeps four lists, each ordered from the **oldest** entry to the **newest**:
//!
//! - `mru` holds the live frames seen **once** recently; `mfu` the live frames seen **at least twice**.
//! - `mru_ghost` and `mfu_ghost` remember the page ids (not the data) of pages recently evicted from `mru` and `mfu`.
//!
//! A hit on a ghost shows which side was evicted too eagerly and moves the **target size** `p` of `mru`. The policy adapts between
//! recency-heavy and frequency-heavy workloads, and a scan cannot flush the frequently used pages.

// TODO(1e-01): your imports go here.
use crate::common::config::{FrameId, PageId};

// TODO(1e-01): private types and helpers go here.

pub struct ArcReplacer {
    // TODO(1e-01): the fields are yours.
}

impl ArcReplacer {
    /// A replacer for a pool of `num_frames` frames (`c` in the paper).
    pub fn new(num_frames: usize) -> ArcReplacer {
        todo!("1e-01: an empty replacer for `num_frames` frames")
    }

    /// How many live frames are evictable.
    pub fn size(&self) -> usize {
        todo!("1e-01: the number of evictable frames")
    }



    /// `frame` now holds (or is accessed as) `page_id`. A frame that is already live is a hit and its page is the same.
    pub fn record_access(&mut self, frame: FrameId, page_id: PageId) {
        todo!("1e-01: a new frame goes to the newest end of mru and is not evictable until marked")
    }

    /// Marks a live frame evictable or not. Frames the replacer does not hold are ignored.
    pub fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        todo!("1e-01: ignore unknown frames; change the flag and keep the count right")
    }

    /// Evicts a frame: from `mru` if it holds at least `p` frames, else from `mfu` (and from the other list if the first has no
    /// evictable frame). The victim's page becomes a ghost. `None` if no frame is evictable.
    pub fn evict(&mut self) -> Option<FrameId> {
        todo!("1e-01: choose an evictable frame, forget it, and return it")
    }

    /// Removes an evictable frame without leaving a ghost (the page was deleted, not evicted). Unknown frames are ignored.
    /// Panics if the frame is not evictable.
    pub fn remove(&mut self, frame: FrameId) {
        todo!("1e-01: ignore unknown frames; panic for a frame that is not evictable; forget it, leaving no ghost")
    }
}

impl crate::buffer::replacer::FrameReplacer for ArcReplacer {
    fn record_access(&mut self, frame: FrameId, page: PageId) {
        ArcReplacer::record_access(self, frame, page)
    }
    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        ArcReplacer::set_evictable(self, frame, evictable)
    }
    fn evict(&mut self) -> Option<FrameId> {
        ArcReplacer::evict(self)
    }
    fn remove(&mut self, frame: FrameId) {
        ArcReplacer::remove(self, frame)
    }
    fn size(&self) -> usize {
        ArcReplacer::size(self)
    }
}
