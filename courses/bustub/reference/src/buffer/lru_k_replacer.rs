//! The LRU-K policy (O'Neil, O'Neil and Weikum, SIGMOD 1993).
//!
//! Evicts the frame whose **backward k-distance** is largest: the time since its k-th most recent access. A frame with fewer than `k`
//! recorded accesses has an infinite distance, and among those the one whose *first* access is oldest goes first. A long scan cannot
//! flush the pool: scanned pages have been touched once, so they leave before pages touched twice.
//!
//! A logical clock advances by one on every recorded access. A frame is **tracked** from its first recorded access until it is evicted
//! or removed, which forget it, history included. Tracked frames start out not evictable.

// @begin 1d-01
use std::collections::{HashMap, VecDeque};
// @begin 1d-03
use std::collections::BTreeSet;
// @end
//~ // TODO(1d-01): your imports go here.
// @end

use crate::common::config::FrameId;

// @begin 1d-01
/// What the replacer remembers about one frame.
struct LruKNode {
    /// The timestamps of the last (up to k) accesses, oldest first.
    history: VecDeque<usize>,
    k: usize,
    fid: FrameId,
    is_evictable: bool,
}

impl LruKNode {
    fn new(fid: FrameId, k: usize) -> LruKNode {
        LruKNode { history: VecDeque::new(), k, fid, is_evictable: false }
    }

    /// Notes an access at time `timestamp`. Only the `k` most recent accesses are kept.
    fn record(&mut self, timestamp: usize) {
        self.history.push_back(timestamp);
        if self.history.len() > self.k {
            self.history.pop_front();
        }
    }

    /// The timestamp of the k-th most recent access, or `None` if there have been fewer than k accesses (distance +infinity).
    fn kth_timestamp(&self) -> Option<usize> {
        if self.history.len() < self.k {
            None
        } else {
            self.history.front().copied()
        }
    }

    /// The oldest timestamp kept.
    fn first_timestamp(&self) -> Option<usize> {
        self.history.front().copied()
    }
}

// @begin 1d-03
/// Where a node sorts in the eviction order (smallest first): frames with fewer than k accesses before the others, by first access;
/// then frames with k accesses, by their k-th most recent access (older = larger backward distance).
fn eviction_key(node: &LruKNode) -> (u8, usize, FrameId) {
    match node.kth_timestamp() {
        None => (0, node.first_timestamp().unwrap_or(0), node.fid),
        Some(t) => (1, t, node.fid),
    }
}
// @end
//~ // TODO(1d-01): private types and helpers go here.
// @end

pub struct LruKReplacer {
    // @begin 1d-01
    node_store: HashMap<FrameId, LruKNode>,
    /// Advances by one on every recorded access.
    current_timestamp: usize,
    /// How many frames are evictable.
    curr_size: usize,
    replacer_size: usize,
    k: usize,
    // @begin 1d-03
    /// The evictable frames in eviction order, so the next victim is the first element.
    order: BTreeSet<(u8, usize, FrameId)>,
    // @end
    //~ // TODO(1d-01): the fields are yours.
    // @end
}

impl LruKReplacer {
    /// A replacer for frames `0..num_frames` with history length `k` (at least 1; a smaller `k` panics).
    pub fn new(num_frames: usize, k: usize) -> LruKReplacer {
        // @begin 1d-01
        assert!(k >= 1, "k must be at least 1");
        LruKReplacer {
            node_store: HashMap::new(),
            current_timestamp: 0,
            curr_size: 0,
            replacer_size: num_frames,
            k,
            // @begin 1d-03
            order: BTreeSet::new(),
            // @end
        }
        //~ todo!("1d-01: an empty replacer; k must be at least 1")
        // @end
    }

    /// Records an access to `frame` at the current time, then advances the clock. A frame seen for the first time starts out not
    /// evictable. Panics if `frame` is not in `0..num_frames`.
    pub fn record_access(&mut self, frame: FrameId) {
        // @begin 1d-01
        assert!(frame.0 < self.replacer_size, "frame {} is out of range for a replacer of {} frames", frame.0, self.replacer_size);
        let now = self.current_timestamp;
        self.current_timestamp += 1;
        // @begin 1d-03
        if let Some(node) = self.node_store.get(&frame) {
            if node.is_evictable {
                self.order.remove(&eviction_key(node));
            }
        }
        // @end
        let k = self.k;
        let node = self.node_store.entry(frame).or_insert_with(|| LruKNode::new(frame, k));
        node.record(now);
        // @begin 1d-03
        if node.is_evictable {
            self.order.insert(eviction_key(node));
        }
        // @end
        //~ todo!("1d-01: panic for a frame id out of range; remember the access and advance the clock")
        // @end
    }

    /// Marks `frame` evictable or not. Frames the replacer does not track are ignored.
    pub fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        // @begin 1d-01
        let Some(node) = self.node_store.get_mut(&frame) else { return };
        if node.is_evictable != evictable {
            node.is_evictable = evictable;
            if evictable {
                self.curr_size += 1;
            } else {
                self.curr_size -= 1;
            }
            // @begin 1d-03
            if evictable {
                self.order.insert(eviction_key(node));
            } else {
                self.order.remove(&eviction_key(node));
            }
            // @end
        }
        //~ todo!("1d-01: ignore unknown frames; change the flag and keep the count of evictable frames right")
        // @end
    }

    /// How many frames are evictable.
    pub fn size(&self) -> usize {
        // @begin 1d-01
        self.curr_size
        //~ todo!("1d-01: the number of evictable frames")
        // @end
    }

    // @begin 1d-01
    /// The victim among the evictable frames by a scan of all of them (O(n)).
    fn pick_victim(&self) -> Option<FrameId> {
        self.node_store
            .values()
            .filter(|n| n.is_evictable)
            // @begin 1d-02
            .min_by_key(|n| match n.kth_timestamp() {
                None => (0, n.first_timestamp()),
                Some(t) => (1, Some(t)),
            })
            //~ .min_by_key(|_| 0)
            // @end
            .map(|n| n.fid)
    }

    /// Forgets `frame`'s node (and its place in the eviction order).
    fn remove_node(&mut self, frame: FrameId) {
        if let Some(node) = self.node_store.remove(&frame) {
            if node.is_evictable {
                self.curr_size -= 1;
                // @begin 1d-03
                self.order.remove(&eviction_key(&node));
                // @end
            }
        }
    }
    // @end

    /// Evicts the frame with the largest backward k-distance among the evictable frames, forgetting its history.
    /// `None` if no frame is evictable.
    pub fn evict(&mut self) -> Option<FrameId> {
        // @begin 1d-01
        // @begin 1d-03
        let victim = self.order.first()?.2;
        //~ let victim = self.pick_victim()?;
        // @end
        self.remove_node(victim);
        Some(victim)
        //~ todo!("1d-01: choose a victim among the evictable frames, forget it, and return it")
        // @end
    }

    /// Removes an evictable frame and its history, whatever its distance. Unknown frames are ignored.
    /// Panics if the frame is tracked but not evictable.
    pub fn remove(&mut self, frame: FrameId) {
        // @begin 1d-01
        let Some(node) = self.node_store.get(&frame) else { return };
        assert!(node.is_evictable, "frame {} is not evictable and cannot be removed", frame.0);
        self.remove_node(frame);
        //~ todo!("1d-01: ignore unknown frames; panic for a frame that is not evictable; otherwise forget it")
        // @end
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
