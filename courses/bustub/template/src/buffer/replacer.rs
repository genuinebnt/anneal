//! The interface of a replacement policy.

use crate::common::config::{FrameId, PageId};

/// Tracks which frames may be evicted and picks the victim.
///
/// Methods take `&mut self`: the pool keeps its replacer under the lock that guards its other state, so no policy needs locking of its own.
pub trait Replacer {
    /// Removes and returns the frame the policy chooses to evict, or `None` if no frame may be evicted.
    fn victim(&mut self) -> Option<FrameId>;

    /// The frame is in use: it must not be evicted until it is unpinned. Pinning a frame the replacer doesn't hold does nothing.
    fn pin(&mut self, frame: FrameId);

    /// The frame is no longer in use: it may now be evicted.
    fn unpin(&mut self, frame: FrameId);

    /// How many frames may be evicted right now.
    fn size(&self) -> usize;
}

/// What a buffer pool needs from a replacement policy. The pool never names a concrete replacer, so any policy that keeps this contract fits.
///
/// A **frame** holds a **page**; the replacer decides which evictable frame to give up.
///
/// - `record_access(frame, page)`: `frame` now holds `page`, or its page was used again. A frame new to the replacer starts **not evictable**.
/// - `set_evictable(frame, evictable)`: whether the frame may be evicted. Frames the replacer does not know are ignored.
/// - `evict()`: removes and returns an evictable frame, or `None` if there is none. The replacer forgets it.
/// - `remove(frame)`: forgets an evictable frame (its page was deleted). Panics if the frame is known but not evictable.
/// - `size()`: the number of evictable frames.
pub trait FrameReplacer: Send {
    fn record_access(&mut self, frame: FrameId, page: PageId);
    fn set_evictable(&mut self, frame: FrameId, evictable: bool);
    fn evict(&mut self) -> Option<FrameId>;
    fn remove(&mut self, frame: FrameId);
    fn size(&self) -> usize;
}
