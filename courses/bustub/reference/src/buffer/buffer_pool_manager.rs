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

// @begin 1f-01
use std::collections::HashMap;
use std::sync::Mutex;

use crate::common::config::BUSTUB_PAGE_SIZE;
use crate::storage::disk::disk_scheduler::{DiskRequest, DiskScheduler};

/// What the pool knows about a frame besides its bytes.
struct FrameMeta {
    /// The page the frame holds, if any.
    page_id: Option<PageId>,
    /// How many users have the page pinned.
    pin_count: usize,
    /// True if the bytes differ from the page on disk.
    dirty: bool,
}

/// Everything the pool's latch protects (BusTub's `bpm_latch_`).
struct Inner {
    /// Which frame holds each resident page.
    page_table: HashMap<PageId, FrameId>,
    /// Frames that hold no page.
    free_frames: Vec<FrameId>,
    meta: Vec<FrameMeta>,
    replacer: Box<dyn FrameReplacer>,
    /// The id the next `new_page` hands out.
    next_page_id: i32,
}
//~ // TODO(1f-01): your imports and private types go here.
// @end

pub struct BufferPoolManager {
    // @begin 1f-01
    num_frames: usize,
    /// The memory itself: one 8 KiB buffer per frame, each behind its own reader-writer latch.
    frames: Vec<RwLock<Box<PageData>>>,
    inner: Mutex<Inner>,
    disk_scheduler: DiskScheduler,
    //~ // TODO(1f-01): the fields are yours: the frames, what you know about each, and a way to reach the disk.
    // @end
}

impl BufferPoolManager {
    /// A pool of `num_frames` free frames over `disk`, evicting with an ARC replacer.
    pub fn new(num_frames: usize, disk: Arc<dyn DiskIo>) -> BufferPoolManager {
        // @begin 1f-01
        BufferPoolManager::with_replacer(num_frames, disk, Box::new(ArcReplacer::new(num_frames)))
        //~ todo!("1f-01: a pool with the ARC replacer; `with_replacer` does the work")
        // @end
    }

    /// A pool of `num_frames` frames on top of `disk`, evicting with `replacer`. Every frame starts free and zeroed.
    pub fn with_replacer(num_frames: usize, disk: Arc<dyn DiskIo>, replacer: Box<dyn FrameReplacer>) -> BufferPoolManager {
        // @begin 1f-01
        let frames = (0..num_frames).map(|_| RwLock::new(Box::new([0u8; BUSTUB_PAGE_SIZE]))).collect();
        let inner = Inner {
            page_table: HashMap::new(),
            free_frames: (0..num_frames).rev().map(FrameId).collect(),
            meta: (0..num_frames).map(|_| FrameMeta { page_id: None, pin_count: 0, dirty: false }).collect(),
            replacer,
            next_page_id: 0,
        };
        BufferPoolManager { num_frames, frames, inner: Mutex::new(inner), disk_scheduler: DiskScheduler::new(disk) }
        //~ todo!("1f-01: the frames and what you know about them, every frame free, and a way to reach the disk")
        // @end
    }

    /// The number of frames.
    pub fn size(&self) -> usize {
        // @begin 1f-01
        self.num_frames
        //~ todo!("1f-01: the number of frames")
        // @end
    }

    /// The bytes of a frame, behind its latch. Only meaningful for a frame that is pinned.
    pub fn frame_data(&self, frame: FrameId) -> &RwLock<Box<PageData>> {
        // @begin 1f-01
        &self.frames[frame.0]
        //~ todo!("1f-01: the bytes of the frame, behind a reader-writer latch")
        // @end
    }

    /// Hands out a fresh page id. The page isn't in memory or on disk yet; `fetch_page` brings it in (all zeros, since the disk
    /// has never seen it).
    pub fn new_page(&self) -> PageId {
        // @begin 1f-01
        let mut inner = self.inner.lock().unwrap();
        let id = PageId(inner.next_page_id);
        inner.next_page_id += 1;
        id
        //~ todo!("1f-01: a page id nobody has been given before; two callers never get the same one")
        // @end
    }

    // @begin 1f-01
    /// Reads a page from disk into `frame`. The caller holds the pool's latch and has the frame to itself.
    fn load(&self, page_id: PageId, frame: FrameId) {
        let (request, future) = DiskRequest::read(page_id);
        self.disk_scheduler.schedule(vec![request]);
        let data = future.get().expect("the disk scheduler is running").expect("reading a page from disk");
        self.frames[frame.0].write().unwrap().copy_from_slice(&*data);
    }
    // @end

    // @begin 1f-02
    /// Writes the bytes of `frame` to disk as page `page_id`. The caller holds the pool's latch; nobody has the frame latched.
    fn store(&self, page_id: PageId, frame: FrameId) {
        let mut copy = Box::new([0u8; BUSTUB_PAGE_SIZE]);
        copy.copy_from_slice(&**self.frames[frame.0].read().unwrap());
        let (request, future) = DiskRequest::write(page_id, copy);
        self.disk_scheduler.schedule(vec![request]);
        future.get().expect("the disk scheduler is running").expect("writing a page to disk");
    }
    // @end

    /// Pins `page_id`, bringing it into memory if needed, and returns its frame. `None` if the page is not in memory and every frame
    /// is pinned. Each successful call must be matched by an `unpin_page`. A page that was never written reads as zeros.
    pub fn fetch_page(&self, page_id: PageId) -> Option<FrameId> {
        // @begin 1f-01
        let mut inner = self.inner.lock().unwrap();
        if let Some(&frame) = inner.page_table.get(&page_id) {
            inner.meta[frame.0].pin_count += 1;
            inner.replacer.record_access(frame, page_id);
            inner.replacer.set_evictable(frame, false);
            return Some(frame);
        }
        let frame = match inner.free_frames.pop() {
            Some(frame) => frame,
            // @begin 1f-02
            None => {
                let frame = inner.replacer.evict()?;
                let old = inner.meta[frame.0].page_id.take().expect("an evicted frame holds a page");
                if inner.meta[frame.0].dirty {
                    self.store(old, frame);
                    inner.meta[frame.0].dirty = false;
                }
                inner.page_table.remove(&old);
                frame
            }
            //~ None => return None, // TODO(1f-02): no free frame: evict an unpinned page, writing it back first if it is dirty
            // @end
        };
        self.load(page_id, frame);
        inner.page_table.insert(page_id, frame);
        inner.meta[frame.0] = FrameMeta { page_id: Some(page_id), pin_count: 1, dirty: false };
        inner.replacer.record_access(frame, page_id);
        inner.replacer.set_evictable(frame, false);
        Some(frame)
        //~ todo!("1f-01: pin the page, reading it into a free frame if it is not in memory")
        // @end
    }

    /// Releases one pin. `is_dirty` says the caller modified the page. `false` if the page isn't in memory or wasn't pinned.
    pub fn unpin_page(&self, page_id: PageId, is_dirty: bool) -> bool {
        // @begin 1f-01
        let mut inner = self.inner.lock().unwrap();
        let Some(&frame) = inner.page_table.get(&page_id) else { return false };
        let meta = &mut inner.meta[frame.0];
        if meta.pin_count == 0 {
            return false;
        }
        meta.pin_count -= 1;
        meta.dirty |= is_dirty;
        if meta.pin_count == 0 {
            inner.replacer.set_evictable(frame, true);
        }
        true
        //~ todo!("1f-01: drop one pin; remember the dirt; when the last pin goes, the frame may be evicted")
        // @end
    }

    /// The page's pin count, or `None` if the page isn't in memory.
    pub fn get_pin_count(&self, page_id: PageId) -> Option<usize> {
        // @begin 1f-01
        let inner = self.inner.lock().unwrap();
        let frame = inner.page_table.get(&page_id)?;
        Some(inner.meta[frame.0].pin_count)
        //~ todo!("1f-01: the pin count of the page, if it is in memory")
        // @end
    }

    /// Writes the page to disk (whether or not it is dirty) and clears its dirty flag. `false` if it isn't in memory.
    pub fn flush_page(&self, page_id: PageId) -> bool {
        // @begin 1f-03
        // @begin 1g-02
        // Pin the page under the lock, then let go of the lock before waiting for the frame's latch: a writer that holds the latch
        // may need the pool lock before it can let go, and holding the lock here would deadlock the two.
        let frame = {
            let mut inner = self.inner.lock().unwrap();
            let Some(&frame) = inner.page_table.get(&page_id) else { return false };
            inner.meta[frame.0].pin_count += 1;
            inner.replacer.set_evictable(frame, false);
            inner.meta[frame.0].dirty = false;
            frame
        };
        let mut copy = Box::new([0u8; BUSTUB_PAGE_SIZE]);
        copy.copy_from_slice(&**self.frames[frame.0].read().unwrap());
        self.write_page_data(page_id, &copy);
        self.unpin_page(page_id, false);
        true
        //~ let mut inner = self.inner.lock().unwrap();
        //~ let Some(&frame) = inner.page_table.get(&page_id) else { return false };
        //~ self.store(page_id, frame);
        //~ inner.meta[frame.0].dirty = false;
        //~ true
        // @end
        //~ todo!("1f-03: write the page out and mark it clean")
        // @end
    }

    /// Flushes every page in memory.
    pub fn flush_all_pages(&self) {
        // @begin 1f-03
        let pages: Vec<PageId> = self.inner.lock().unwrap().page_table.keys().copied().collect();
        for page in pages {
            self.flush_page(page);
        }
        //~ todo!("1f-03: flush every page that is in memory")
        // @end
    }

    /// Removes the page from memory and frees its disk space. `false` if somebody has it pinned; `true` otherwise (also when it
    /// wasn't in memory). A dirty page is simply dropped: it is being deleted.
    pub fn delete_page(&self, page_id: PageId) -> bool {
        // @begin 1f-03
        let mut inner = self.inner.lock().unwrap();
        if let Some(&frame) = inner.page_table.get(&page_id) {
            if inner.meta[frame.0].pin_count > 0 {
                return false;
            }
            inner.page_table.remove(&page_id);
            inner.replacer.remove(frame);
            inner.meta[frame.0] = FrameMeta { page_id: None, pin_count: 0, dirty: false };
            inner.free_frames.push(frame);
            self.frames[frame.0].write().unwrap().fill(0);
        }
        self.disk_scheduler.deallocate_page(page_id);
        true
        //~ todo!("1f-03: refuse if the page is pinned; otherwise drop it from memory without writing it, free its frame, and tell the disk")
        // @end
    }

    /// Pins `page_id` and takes its read latch. `None` if the page cannot be brought into memory because every frame is pinned.
    pub fn checked_read_page(&self, page_id: PageId) -> Option<ReadPageGuard<'_>> {
        // @begin 1g-01
        let frame = self.fetch_page(page_id)?;
        // The pool's lock is NOT held here: waiting for a latch while holding it would block every other thread.
        let latch = self.frames[frame.0].read().unwrap();
        Some(ReadPageGuard::new(self, page_id, frame, latch))
        //~ todo!("1g-01: pin the page, take the frame's read latch, and hand out a guard that releases both")
        // @end
    }

    /// Pins `page_id` and takes its write latch. `None` if the page cannot be brought into memory because every frame is pinned.
    pub fn checked_write_page(&self, page_id: PageId) -> Option<WritePageGuard<'_>> {
        // @begin 1g-01
        let frame = self.fetch_page(page_id)?;
        let latch = self.frames[frame.0].write().unwrap();
        Some(WritePageGuard::new(self, page_id, frame, latch))
        //~ todo!("1g-01: like checked_read_page, with the write latch")
        // @end
    }

    /// Like `checked_read_page`, but panics if the page can't be brought in.
    pub fn read_page(&self, page_id: PageId) -> ReadPageGuard<'_> {
        // @begin 1g-02
        self.checked_read_page(page_id).expect("every frame is pinned: no room to bring the page into memory")
        //~ todo!("1g-02: checked_read_page, but a failure is a panic")
        // @end
    }

    /// Like `checked_write_page`, but panics if the page can't be brought in.
    pub fn write_page(&self, page_id: PageId) -> WritePageGuard<'_> {
        // @begin 1g-02
        self.checked_write_page(page_id).expect("every frame is pinned: no room to bring the page into memory")
        //~ todo!("1g-02: checked_write_page, but a failure is a panic")
        // @end
    }

    // @begin 1g-02
    /// Writes `data` to disk as page `page_id` without touching the pool's state. For guards that already hold the page's latch.
    pub(crate) fn write_page_data(&self, page_id: PageId, data: &PageData) {
        let (request, future) = DiskRequest::write(page_id, Box::new(*data));
        self.disk_scheduler.schedule(vec![request]);
        future.get().expect("the disk scheduler is running").expect("writing a page to disk");
    }
    // @end
}
