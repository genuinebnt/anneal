//! A replacement policy of your own: evict the frame that has been used the fewest times.

use crate::buffer::replacer::FrameReplacer;
use crate::common::config::{FrameId, PageId};

/// Least frequently used. Every frame counts its accesses; the victim is the evictable frame with the **fewest** accesses, and among equals the
/// one whose last access is **oldest**. A frame the replacer meets for the first time starts with one access and is not evictable. When a frame
/// is evicted or removed the replacer forgets it, count included. The contract of [`FrameReplacer`] holds as for every policy.
pub struct LfuReplacer {
    _lfu: (),
}

impl LfuReplacer {
    /// A replacer that tracks no frames.
    pub fn new() -> LfuReplacer {
        todo!("1c-c1: a replacer that knows no frames")
    }
}

impl Default for LfuReplacer {
    fn default() -> Self {
        LfuReplacer::new()
    }
}

impl FrameReplacer for LfuReplacer {
    fn record_access(&mut self, frame: FrameId, _page: PageId) {
        todo!("1c-c1: count the access")
    }

    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        todo!("1c-c1: mark the frame")
    }

    fn evict(&mut self) -> Option<FrameId> {
        todo!("1c-c1: the evictable frame with the fewest accesses, the oldest last access among equals")
    }

    fn remove(&mut self, frame: FrameId) {
        todo!("1c-c1: forget an evictable frame; a frame that is known but not evictable is a bug of the caller (panic)")
    }

    fn size(&self) -> usize {
        todo!("1c-c1: how many frames may be evicted")
    }
}
