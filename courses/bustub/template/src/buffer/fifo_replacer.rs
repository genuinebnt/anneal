//! First in, first out: the frame that entered the pool earliest is evicted first, however often it was used since.

use std::collections::{HashSet, VecDeque};

use crate::buffer::replacer::FrameReplacer;
use crate::common::config::{FrameId, PageId};

pub struct FifoReplacer {
    /// Frames in the order they arrived.
    order: VecDeque<FrameId>,
    evictable: HashSet<FrameId>,
    /// How many frames may be evicted now (what `size` answers).
    evictable_count: usize,
}

impl FifoReplacer {
    pub fn new() -> FifoReplacer {
        FifoReplacer { order: VecDeque::new(), evictable: HashSet::new(), evictable_count: 0 }
    }
}

impl Default for FifoReplacer {
    fn default() -> Self {
        FifoReplacer::new()
    }
}

impl FrameReplacer for FifoReplacer {
    /// A frame is queued the first time it is seen (not evictable); using it again changes nothing: this is FIFO, not LRU.
    fn record_access(&mut self, frame: FrameId, _page: PageId) {
        if !self.order.contains(&frame) {
            self.order.push_back(frame);
        }
    }

    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        if !self.order.contains(&frame) {
            return;
        }
        if evictable {
            if self.evictable.insert(frame) {
                self.evictable_count += 1;
            }
        } else {
            // TODO(1c-c2): this looks right and has a bug; find it and fix it
            self.evictable.remove(&frame);
        }
    }

    fn evict(&mut self) -> Option<FrameId> {
        let at = self.order.iter().position(|f| self.evictable.contains(f))?;
        let frame = self.order.remove(at)?;
        self.evictable.remove(&frame);
        self.evictable_count -= 1;
        Some(frame)
    }

    fn remove(&mut self, frame: FrameId) {
        if let Some(at) = self.order.iter().position(|f| *f == frame) {
            assert!(self.evictable.contains(&frame), "remove of frame {} which is not evictable", frame.0);
            self.order.remove(at);
            self.evictable.remove(&frame);
            self.evictable_count -= 1;
        }
    }

    fn size(&self) -> usize {
        self.evictable_count
    }
}
