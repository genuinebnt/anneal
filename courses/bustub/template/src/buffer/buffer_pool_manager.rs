//! A fixed number of pages cached in memory.
//!
//! A page is read into a **frame** when first needed. Callers **pin** a page while they use it, so it cannot be taken away; when a frame
//! is needed and none is free, the replacer picks a page nobody has pinned, which is written back first if it was modified.
//!
//! The pool works with any [`FrameReplacer`].

use std::sync::{Arc, RwLock};

use super::arc_replacer::ArcReplacer;
use super::replacer::FrameReplacer;
use crate::common::config::{FrameId, PageData, PageId};
use crate::storage::disk::disk_manager::DiskIo;
use crate::storage::page::page_guard::{ReadPageGuard, WritePageGuard};

// TODO(1f-01): your imports and private types go here.

pub struct BufferPoolManager {
    // TODO(1f-01): the fields are yours: the frames, what you know about each, and a way to reach the disk.
}

impl BufferPoolManager {
    /// A pool of `num_frames` free frames over `disk`, evicting with an ARC replacer.
    pub fn new(num_frames: usize, disk: Arc<dyn DiskIo>) -> BufferPoolManager {
        todo!("1f-01: a pool with the ARC replacer; `with_replacer` does the work")
    }

    /// A pool of `num_frames` frames on top of `disk`, evicting with `replacer`. Every frame starts free and zeroed.
    pub fn with_replacer(num_frames: usize, disk: Arc<dyn DiskIo>, replacer: Box<dyn FrameReplacer>) -> BufferPoolManager {
        todo!("1f-01: the frames and what you know about them, every frame free, and a way to reach the disk")
    }

    /// The number of frames.
    pub fn size(&self) -> usize {
        todo!("1f-01: the number of frames")
    }

    /// The bytes of a frame, behind its latch. Only meaningful for a frame you have pinned.
    pub fn frame_data(&self, frame: FrameId) -> &RwLock<Box<PageData>> {
        todo!("1f-01: the bytes of the frame, behind a reader-writer latch")
    }

    /// Hands out a fresh page id. The page isn't in memory or on disk yet; `fetch_page` brings it in (all zeros, since the disk
    /// has never seen it).
    pub fn new_page(&self) -> PageId {
        todo!("1f-01: a page id nobody has been given before; two callers never get the same one")
    }



    /// Pins `page_id`, bringing it into memory if needed, and returns its frame. `None` if the page is not in memory and every frame
    /// is pinned. Each successful call must be matched by an `unpin_page`. A page that was never written reads as zeros.
    pub fn fetch_page(&self, page_id: PageId) -> Option<FrameId> {
        todo!("1f-01: pin the page, reading it into a free frame if it is not in memory")
    }

    /// Releases one pin. `is_dirty` says the caller modified the page. `false` if the page isn't in memory or wasn't pinned.
    pub fn unpin_page(&self, page_id: PageId, is_dirty: bool) -> bool {
        todo!("1f-01: drop one pin; remember the dirt; when the last pin goes, the frame may be evicted")
    }

    /// The page's pin count, or `None` if the page isn't in memory.
    pub fn get_pin_count(&self, page_id: PageId) -> Option<usize> {
        todo!("1f-01: the pin count of the page, if it is in memory")
    }

    /// Writes the page to disk (whether or not it is dirty) and clears its dirty flag. `false` if it isn't in memory.
    pub fn flush_page(&self, page_id: PageId) -> bool {
        todo!("1f-03: write the page out and mark it clean")
    }

    /// Flushes every page in memory.
    pub fn flush_all_pages(&self) {
        todo!("1f-03: flush every page that is in memory")
    }

    /// Removes the page from memory and frees its disk space. `false` if somebody has it pinned; `true` otherwise (also when it
    /// wasn't in memory). A dirty page is simply dropped: it is being deleted.
    pub fn delete_page(&self, page_id: PageId) -> bool {
        todo!("1f-03: refuse if the page is pinned; otherwise drop it from memory without writing it, free its frame, and tell the disk")
    }

    /// Pins `page_id` and takes its read latch. `None` if the page cannot be brought into memory because every frame is pinned.
    pub fn checked_read_page(&self, page_id: PageId) -> Option<ReadPageGuard<'_>> {
        todo!("1g-01: pin the page, take the frame's read latch, and hand out a guard that releases both")
    }

    /// Pins `page_id` and takes its write latch. `None` if the page cannot be brought into memory because every frame is pinned.
    pub fn checked_write_page(&self, page_id: PageId) -> Option<WritePageGuard<'_>> {
        todo!("1g-01: like checked_read_page, with the write latch")
    }

    /// Like `checked_read_page`, but panics if the page can't be brought in.
    pub fn read_page(&self, page_id: PageId) -> ReadPageGuard<'_> {
        todo!("1g-02: checked_read_page, but a failure is a panic")
    }

    /// Like `checked_write_page`, but panics if the page can't be brought in.
    pub fn write_page(&self, page_id: PageId) -> WritePageGuard<'_> {
        todo!("1g-02: checked_write_page, but a failure is a panic")
    }

}
