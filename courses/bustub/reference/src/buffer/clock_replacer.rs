//! The CLOCK policy: an approximation of LRU that needs one bit per frame.
//!
//! The evictable frames sit on a **ring** with a **hand** pointing at one of them; each has a reference bit.
//!
//! - `unpin(f)` sets `f`'s bit if it is on the ring, otherwise adds it just **behind the hand** with its bit set.
//! - `pin(f)` removes `f`. The hand keeps pointing at the same frame, or at the next one if `f` was under it.
//! - `victim()` sweeps the hand, clearing set bits (a second chance), until it finds a frame with its bit clear. That frame is removed
//!   and returned, and the hand points at the frame after it.

use super::replacer::Replacer;
use crate::common::config::FrameId;

pub struct ClockReplacer {
    // @begin 1c-03
    capacity: usize,
    /// The evictable frames in ring order, each with its reference bit.
    ring: Vec<(FrameId, bool)>,
    /// Index into `ring` of the frame the hand points at.
    hand: usize,
    //~ // TODO(1c-03): the fields are yours: a ring, a hand, and a reference bit per frame.
    // @end
}

impl ClockReplacer {
    /// A replacer for a pool of `num_pages` frames. Unpinning more distinct frames than that is a bug in the caller: it panics.
    pub fn new(num_pages: usize) -> ClockReplacer {
        // @begin 1c-03
        ClockReplacer { capacity: num_pages, ring: Vec::new(), hand: 0 }
        //~ todo!("1c-03: an empty replacer for `num_pages` frames")
        // @end
    }
}

impl Replacer for ClockReplacer {
    fn unpin(&mut self, frame: FrameId) {
        // @begin 1c-03
        match self.ring.iter_mut().find(|(f, _)| *f == frame) {
            Some(slot) => slot.1 = true,
            None => {
                assert!(self.ring.len() < self.capacity, "the replacer is full: more frames than it was made for");
                self.ring.insert(self.hand, (frame, true));
                self.hand = (self.hand + 1) % self.ring.len();
            }
        }
        //~ todo!("1c-03: a frame already on the ring gets its reference bit set; a new one joins just behind the hand, bit set")
        // @end
    }

    fn size(&self) -> usize {
        // @begin 1c-03
        self.ring.len()
        //~ todo!("1c-03: how many frames are on the ring")
        // @end
    }

    fn pin(&mut self, frame: FrameId) {
        // @begin 1c-03
        let Some(at) = self.ring.iter().position(|(f, _)| *f == frame) else { return };
        self.ring.remove(at);
        if at < self.hand {
            self.hand -= 1;
        }
        if self.hand >= self.ring.len() {
            self.hand = 0;
        }
        //~ todo!("1c-03: take the frame off the ring; the hand keeps pointing at the same frame it did (wrapping past the end)")
        // @end
    }

    fn victim(&mut self) -> Option<FrameId> {
        // @begin 1c-03
        if self.ring.is_empty() {
            return None;
        }
        loop {
            if self.ring[self.hand].1 {
                self.ring[self.hand].1 = false;
                self.hand = (self.hand + 1) % self.ring.len();
            } else {
                let (frame, _) = self.ring.remove(self.hand);
                if self.hand >= self.ring.len() {
                    self.hand = 0;
                }
                return Some(frame);
            }
        }
        //~ todo!("1c-03: sweep the hand: clear set bits and move on, remove and return the first frame whose bit is clear")
        // @end
    }
}
