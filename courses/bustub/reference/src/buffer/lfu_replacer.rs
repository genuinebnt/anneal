//! A replacement policy of your own: evict the frame that has been used the fewest times.

use crate::buffer::replacer::FrameReplacer;
use crate::common::config::{FrameId, PageId};

/// Least frequently used. Every frame counts its accesses; the victim is the evictable frame with the **fewest** accesses, and among equals the
/// one whose last access is **oldest**. A frame the replacer meets for the first time starts with one access and is not evictable. When a frame
/// is evicted or removed the replacer forgets it, count included. The contract of [`FrameReplacer`] holds as for every policy.
pub struct LfuReplacer {
    // @begin 1c-c1
    /// (frame, accesses, tick of the last access, evictable); a tick counts every `record_access`.
    frames: Vec<(FrameId, u64, u64, bool)>,
    tick: u64,
    //~ _lfu: (),
    // @end
}

impl LfuReplacer {
    /// A replacer that tracks no frames.
    pub fn new() -> LfuReplacer {
        // @begin 1c-c1
        LfuReplacer { frames: Vec::new(), tick: 0 }
        //~ todo!("1c-c1: a replacer that knows no frames")
        // @end
    }
}

impl Default for LfuReplacer {
    fn default() -> Self {
        LfuReplacer::new()
    }
}

impl FrameReplacer for LfuReplacer {
    fn record_access(&mut self, frame: FrameId, _page: PageId) {
        // @begin 1c-c1
        self.tick += 1;
        match self.frames.iter_mut().find(|e| e.0 == frame) {
            Some(e) => {
                e.1 += 1;
                e.2 = self.tick;
            }
            None => self.frames.push((frame, 1, self.tick, false)),
        }
        //~ todo!("1c-c1: count the access")
        // @end
    }

    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        // @begin 1c-c1
        if let Some(e) = self.frames.iter_mut().find(|e| e.0 == frame) {
            e.3 = evictable;
        }
        //~ todo!("1c-c1: mark the frame")
        // @end
    }

    fn evict(&mut self) -> Option<FrameId> {
        // @begin 1c-c1
        let at = self.frames.iter().enumerate().filter(|(_, e)| e.3).min_by_key(|(_, e)| (e.1, e.2)).map(|(i, _)| i)?;
        Some(self.frames.remove(at).0)
        //~ todo!("1c-c1: the evictable frame with the fewest accesses, the oldest last access among equals")
        // @end
    }

    fn remove(&mut self, frame: FrameId) {
        // @begin 1c-c1
        if let Some(at) = self.frames.iter().position(|e| e.0 == frame) {
            assert!(self.frames[at].3, "remove of frame {} which is not evictable", frame.0);
            self.frames.remove(at);
        }
        //~ todo!("1c-c1: forget an evictable frame; a frame that is known but not evictable is a bug of the caller (panic)")
        // @end
    }

    fn size(&self) -> usize {
        // @begin 1c-c1
        self.frames.iter().filter(|e| e.3).count()
        //~ todo!("1c-c1: how many frames may be evicted")
        // @end
    }
}
