//! Evicts the frame that was unpinned longest ago.
//!
//! The replacer holds the evictable (unpinned) frames. Unpinning a frame it lacks adds it as the **most recently used**; unpinning one
//! it holds changes nothing. Pinning removes it. `victim` removes and returns the **least recently used** frame.

use super::replacer::Replacer;
use crate::common::config::FrameId;

// @begin 1c-02
use crate::common::index_list::{Handle, IndexList};
use std::collections::HashMap;
//~ // TODO(1c-02): your imports go here.
// @end

pub struct LruReplacer {
    // @begin 1c-02
    capacity: usize,
    /// The evictable frames, least recently unpinned first.
    list: IndexList<FrameId>,
    /// Where each evictable frame is in `list`.
    handles: HashMap<FrameId, Handle>,
    //~ // TODO(1c-02): the fields are yours. `IndexList` from 1c-01 is one way to get O(1) removal from the middle; a design of your own is just as good.
    // @end
}

impl LruReplacer {
    /// A replacer for a pool of `num_pages` frames. Unpinning more distinct frames than that is a bug in the caller: it panics.
    pub fn new(num_pages: usize) -> LruReplacer {
        // @begin 1c-02
        LruReplacer { capacity: num_pages, list: IndexList::new(), handles: HashMap::new() }
        //~ todo!("1c-02: an empty replacer for `num_pages` frames")
        // @end
    }
}

impl Replacer for LruReplacer {
    fn victim(&mut self) -> Option<FrameId> {
        // @begin 1c-02
        let frame = self.list.pop_front()?;
        self.handles.remove(&frame);
        Some(frame)
        //~ todo!("1c-02: remove and return the least recently used frame, if there is one")
        // @end
    }

    fn pin(&mut self, frame: FrameId) {
        // @begin 1c-02
        if let Some(handle) = self.handles.remove(&frame) {
            self.list.remove(handle);
        }
        //~ todo!("1c-02: the frame may not be evicted any more; if it was not in the replacer, nothing happens")
        // @end
    }

    fn unpin(&mut self, frame: FrameId) {
        // @begin 1c-02
        if self.handles.contains_key(&frame) {
            return;
        }
        assert!(self.list.len() < self.capacity, "the replacer is full: more frames than it was made for");
        let handle = self.list.push_back(frame);
        self.handles.insert(frame, handle);
        //~ todo!("1c-02: the frame may be evicted; if it is new it is the most recently used, if it was already there nothing changes")
        // @end
    }

    fn size(&self) -> usize {
        // @begin 1c-02
        self.list.len()
        //~ todo!("1c-02: how many frames may be evicted")
        // @end
    }
}
