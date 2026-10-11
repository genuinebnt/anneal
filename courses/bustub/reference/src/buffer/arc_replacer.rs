//! The ARC replacement policy (Megiddo and Modha, FAST 2003).
//!
//! ARC keeps four lists, each ordered from the **oldest** entry to the **newest**:
//!
//! - `mru` holds the live frames seen **once** recently; `mfu` the live frames seen **at least twice**.
//! - `mru_ghost` and `mfu_ghost` remember the page ids (not the data) of pages recently evicted from `mru` and `mfu`.
//!
//! A hit on a ghost shows which side was evicted too eagerly and moves the **target size** `p` of `mru`. The policy adapts between
//! recency-heavy and frequency-heavy workloads, and a scan cannot flush the frequently used pages.

// @begin 1e-01
use std::collections::HashMap;

use crate::common::index_list::{Handle, IndexList};
//~ // TODO(1e-01): your imports go here.
// @end
use crate::common::config::{FrameId, PageId};

// @begin 1e-01
/// Which of the four lists an entry is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArcStatus {
    Mru,
    Mfu,
    MruGhost,
    MfuGhost,
}

/// A frame that holds a page right now (on `mru` or `mfu`).
struct Alive {
    page_id: PageId,
    evictable: bool,
    status: ArcStatus,
    handle: Handle,
}

/// A page that was evicted recently (on `mru_ghost` or `mfu_ghost`). It has no frame.
struct Ghost {
    status: ArcStatus,
    handle: Handle,
}
//~ // TODO(1e-01): private types and helpers go here.
// @end

pub struct ArcReplacer {
    // @begin 1e-01
    mru: IndexList<FrameId>,
    mfu: IndexList<FrameId>,
    mru_ghost: IndexList<PageId>,
    mfu_ghost: IndexList<PageId>,
    /// Where each live frame is.
    alive: HashMap<FrameId, Alive>,
    /// Where each ghost page is.
    ghost: HashMap<PageId, Ghost>,
    /// How many live frames are evictable.
    curr_size: usize,
    /// `p` in the paper: how many frames `mru` should hold.
    mru_target_size: usize,
    /// `c` in the paper: the number of frames.
    replacer_size: usize,
    //~ // TODO(1e-01): the fields are yours.
    // @end
}

impl ArcReplacer {
    /// A replacer for a pool of `num_frames` frames (`c` in the paper).
    pub fn new(num_frames: usize) -> ArcReplacer {
        // @begin 1e-01
        ArcReplacer {
            mru: IndexList::new(),
            mfu: IndexList::new(),
            mru_ghost: IndexList::new(),
            mfu_ghost: IndexList::new(),
            alive: HashMap::new(),
            ghost: HashMap::new(),
            curr_size: 0,
            mru_target_size: 0,
            replacer_size: num_frames,
        }
        //~ todo!("1e-01: an empty replacer for `num_frames` frames")
        // @end
    }

    /// How many live frames are evictable.
    pub fn size(&self) -> usize {
        // @begin 1e-01
        self.curr_size
        //~ todo!("1e-01: the number of evictable frames")
        // @end
    }

    // @begin 1e-01
    /// Puts `frame`, now holding `page_id`, at the newest end of the `mru` or `mfu` list. It is not evictable yet.
    fn push_alive(&mut self, frame: FrameId, page_id: PageId, status: ArcStatus) {
        let handle = if status == ArcStatus::Mfu { self.mfu.push_back(frame) } else { self.mru.push_back(frame) };
        self.alive.insert(frame, Alive { page_id, evictable: false, status, handle });
    }
    // @end

    // @begin 1e-02
    /// Remembers `page_id` as a ghost at the newest end of the `mru_ghost` or `mfu_ghost` list.
    fn push_ghost(&mut self, page_id: PageId, status: ArcStatus) {
        let handle = if status == ArcStatus::MfuGhost { self.mfu_ghost.push_back(page_id) } else { self.mru_ghost.push_back(page_id) };
        self.ghost.insert(page_id, Ghost { status, handle });
    }

    /// The oldest evictable frame on `mru` or `mfu`.
    fn oldest_evictable(&self, status: ArcStatus) -> Option<FrameId> {
        let list = if status == ArcStatus::Mfu { &self.mfu } else { &self.mru };
        list.iter().copied().find(|frame| self.alive[frame].evictable)
    }
    // @end

    /// `frame` now holds (or is accessed as) `page_id`. A frame that is already live is a hit and its page is the same.
    pub fn record_access(&mut self, frame: FrameId, page_id: PageId) {
        // @begin 1e-01
        // @begin 1e-02
        if let Some(alive) = self.alive.get(&frame) {
            // A hit on a live frame: it has now been seen at least twice, so it belongs at the newest end of `mfu`.
            let (status, handle) = (alive.status, alive.handle);
            if status == ArcStatus::Mfu {
                self.mfu.move_to_back(handle);
            } else {
                self.mru.remove(handle);
                let handle = self.mfu.push_back(frame);
                let alive = self.alive.get_mut(&frame).expect("just looked it up");
                alive.status = ArcStatus::Mfu;
                alive.handle = handle;
            }
            return;
        }
        //~ if self.alive.contains_key(&frame) {
        //~     return; // TODO(1e-02): a hit moves the frame to the newest end of mfu
        //~ }
        // @end
        // @begin 1e-03
        if let Some(ghost) = self.ghost.remove(&page_id) {
            // A hit on a ghost: the page was evicted too early. It comes back as a live `mfu` entry.
            match ghost.status {
                ArcStatus::MruGhost => {
                    // Evicting from mru was a mistake: grow its target. The step is larger when mfu_ghost is the bigger list.
                    let delta = if self.mru_ghost.len() >= self.mfu_ghost.len() { 1 } else { self.mfu_ghost.len() / self.mru_ghost.len() };
                    self.mru_target_size = (self.mru_target_size + delta).min(self.replacer_size);
                    self.mru_ghost.remove(ghost.handle);
                }
                ArcStatus::MfuGhost => {
                    // Evicting from mfu was a mistake: shrink the mru target.
                    let delta = if self.mfu_ghost.len() >= self.mru_ghost.len() { 1 } else { self.mru_ghost.len() / self.mfu_ghost.len() };
                    self.mru_target_size = self.mru_target_size.saturating_sub(delta);
                    self.mfu_ghost.remove(ghost.handle);
                }
                ArcStatus::Mru | ArcStatus::Mfu => unreachable!("ghost lists hold ghosts only"),
            }
            self.push_alive(frame, page_id, ArcStatus::Mfu);
            return;
        }
        // A page ARC has not seen. Keep the four lists within their limits: mru + mru_ghost at most `c`, all four at most `2c`.
        let recent = self.mru.len() + self.mru_ghost.len();
        if recent >= self.replacer_size {
            if let Some(page) = self.mru_ghost.pop_front() {
                self.ghost.remove(&page);
            }
        } else if recent + self.mfu.len() + self.mfu_ghost.len() >= 2 * self.replacer_size {
            if let Some(page) = self.mfu_ghost.pop_front() {
                self.ghost.remove(&page);
            }
        }
        //~ // TODO(1e-03): ghost hits adapt the target; the ghost lists stay within their limits
        // @end
        self.push_alive(frame, page_id, ArcStatus::Mru);
        //~ todo!("1e-01: a new frame goes to the newest end of mru and is not evictable until marked")
        // @end
    }

    /// Marks a live frame evictable or not. Frames the replacer does not hold are ignored.
    pub fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        // @begin 1e-01
        let Some(alive) = self.alive.get_mut(&frame) else { return };
        if alive.evictable != evictable {
            alive.evictable = evictable;
            if evictable {
                self.curr_size += 1;
            } else {
                self.curr_size -= 1;
            }
        }
        //~ todo!("1e-01: ignore unknown frames; change the flag and keep the count right")
        // @end
    }

    /// Evicts a frame: from `mru` if it holds at least `p` frames, else from `mfu` (and from the other list if the first has no
    /// evictable frame). The victim's page becomes a ghost. `None` if no frame is evictable.
    pub fn evict(&mut self) -> Option<FrameId> {
        // @begin 1e-01
        // @begin 1e-02
        // @begin 1e-03
        let order = if self.mru.len() >= self.mru_target_size { [ArcStatus::Mru, ArcStatus::Mfu] } else { [ArcStatus::Mfu, ArcStatus::Mru] };
        //~ let order = [ArcStatus::Mru, ArcStatus::Mfu];
        // @end
        let (frame, status) = order.into_iter().find_map(|status| self.oldest_evictable(status).map(|frame| (frame, status)))?;
        //~ let (frame, status) = self.alive.iter().find(|(_, alive)| alive.evictable).map(|(&frame, alive)| (frame, alive.status))?;
        // @end
        let alive = self.alive.remove(&frame).expect("a listed frame is alive");
        if status == ArcStatus::Mfu {
            self.mfu.remove(alive.handle);
        } else {
            self.mru.remove(alive.handle);
        }
        // @begin 1e-02
        self.push_ghost(alive.page_id, if status == ArcStatus::Mfu { ArcStatus::MfuGhost } else { ArcStatus::MruGhost });
        // @end
        self.curr_size -= 1;
        Some(frame)
        //~ todo!("1e-01: choose an evictable frame, forget it, and return it")
        // @end
    }

    /// Removes an evictable frame without leaving a ghost (the page was deleted, not evicted). Unknown frames are ignored.
    /// Panics if the frame is not evictable.
    pub fn remove(&mut self, frame: FrameId) {
        // @begin 1e-01
        let Some(alive) = self.alive.get(&frame) else { return };
        assert!(alive.evictable, "frame {} is not evictable and cannot be removed", frame.0);
        let alive = self.alive.remove(&frame).expect("just looked it up");
        if alive.status == ArcStatus::Mfu {
            self.mfu.remove(alive.handle);
        } else {
            self.mru.remove(alive.handle);
        }
        self.curr_size -= 1;
        //~ todo!("1e-01: ignore unknown frames; panic for a frame that is not evictable; forget it, leaving no ghost")
        // @end
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
