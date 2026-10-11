//! A page guard is an RAII handle on a page in the buffer pool. While it exists the page is **pinned** (it can't be evicted) and **latched**
//! (shared for a `ReadPageGuard`, exclusive for a `WritePageGuard`). When it is dropped, in that order, the page is unlatched and then unpinned, so a
//! caller can't forget either, even on an early `return`, `?` or panic.

use std::ops::{Deref, DerefMut};

use crate::common::config::{PageData, PageId};

// @begin 1g-01
use std::sync::{RwLockReadGuard, RwLockWriteGuard};

use crate::buffer::buffer_pool_manager::BufferPoolManager;
use crate::common::config::FrameId;
//~ use std::marker::PhantomData;
//~ // TODO(1g-01): your imports go here.
// @end

/// Shared access to a page. Many readers may hold one for the same page at once.
pub struct ReadPageGuard<'a> {
    // @begin 1g-01
    bpm: &'a BufferPoolManager,
    page_id: PageId,
    #[allow(dead_code)]
    frame: FrameId,
    /// `None` once the guard has been released.
    latch: Option<RwLockReadGuard<'a, Box<PageData>>>,
    //~ _guard: PhantomData<&'a ()>,
    //~ // TODO(1g-01): the fields are yours: the pool, the page, and whatever keeps the page latched.
    // @end
}

// @begin 1g-01
impl<'a> ReadPageGuard<'a> {
    /// Only the buffer pool makes guards: the page must already be pinned and its frame latched.
    pub(crate) fn new(bpm: &'a BufferPoolManager, page_id: PageId, frame: FrameId, latch: RwLockReadGuard<'a, Box<PageData>>) -> ReadPageGuard<'a> {
        ReadPageGuard { bpm, page_id, frame, latch: Some(latch) }
    }
}
//~ // TODO(1g-01): the pool builds a guard when it hands out a page: a constructor of your own goes here.
// @end

impl ReadPageGuard<'_> {
    /// The id of the page this guard holds.
    pub fn get_page_id(&self) -> PageId {
        // @begin 1g-01
        self.page_id
        //~ todo!("1g-01: the id of the page this guard holds")
        // @end
    }

    /// The page's bytes. Panics if the guard has been released.
    pub fn get_data(&self) -> &PageData {
        // @begin 1g-01
        self.latch.as_ref().expect("this guard has been released")
        //~ todo!("1g-01: the bytes behind the read latch you are holding (panic if the guard was released)")
        // @end
    }

    /// Writes the page to disk now. The guard keeps its pin and latch.
    pub fn flush(&mut self) {
        // @begin 1g-02
        let data: PageData = *self.get_data();
        self.bpm.write_page_data(self.page_id, &data);
        //~ todo!("1g-02: write the bytes you hold to disk, without taking the frame latch again (you already hold it)")
        // @end
    }

    /// Unlatches and unpins the page. Does nothing if the guard was already released.
    pub fn release(&mut self) {
        // @begin 1g-01
        if let Some(latch) = self.latch.take() {
            drop(latch); // unlatch first ...
            self.bpm.unpin_page(self.page_id, false); // ... then unpin, so an unpinned page is never latched
        }
        //~ todo!("1g-01: if the guard still holds its latch: release the latch, then unpin the page; a second call does nothing")
        // @end
    }
}

impl Deref for ReadPageGuard<'_> {
    type Target = PageData;
    fn deref(&self) -> &PageData {
        self.get_data()
    }
}

impl Drop for ReadPageGuard<'_> {
    fn drop(&mut self) {
        // @begin 1g-01
        self.release();
        //~ // TODO(1g-01): dropping a guard releases it
        // @end
    }
}

/// Exclusive access to a page. Taking `get_data_mut` marks the page dirty, so the pool writes it back before reusing its frame.
pub struct WritePageGuard<'a> {
    // @begin 1g-01
    bpm: &'a BufferPoolManager,
    page_id: PageId,
    #[allow(dead_code)]
    frame: FrameId,
    latch: Option<RwLockWriteGuard<'a, Box<PageData>>>,
    is_dirty: bool,
    //~ _guard: PhantomData<&'a ()>,
    //~ // TODO(1g-01): the fields are yours.
    // @end
}

// @begin 1g-01
impl<'a> WritePageGuard<'a> {
    pub(crate) fn new(bpm: &'a BufferPoolManager, page_id: PageId, frame: FrameId, latch: RwLockWriteGuard<'a, Box<PageData>>) -> WritePageGuard<'a> {
        WritePageGuard { bpm, page_id, frame, latch: Some(latch), is_dirty: false }
    }
}
//~ // TODO(1g-01): a constructor of your own goes here.
// @end

impl WritePageGuard<'_> {
    /// The id of the page this guard holds.
    pub fn get_page_id(&self) -> PageId {
        // @begin 1g-01
        self.page_id
        //~ todo!("1g-01: the id of the page this guard holds")
        // @end
    }

    /// The page's bytes. Panics if the guard has been released.
    pub fn get_data(&self) -> &PageData {
        // @begin 1g-01
        self.latch.as_ref().expect("this guard has been released")
        //~ todo!("1g-01: the bytes behind the write latch")
        // @end
    }

    /// The page's bytes for modification. Marks the page dirty.
    pub fn get_data_mut(&mut self) -> &mut PageData {
        // @begin 1g-01
        self.is_dirty = true;
        self.latch.as_mut().expect("this guard has been released")
        //~ todo!("1g-01: remember that the page is dirty, then hand out the bytes mutably")
        // @end
    }

    /// True if `get_data_mut` was called since the last flush.
    pub fn is_dirty(&self) -> bool {
        // @begin 1g-01
        self.is_dirty
        //~ todo!("1g-01: whether the page has been handed out for modification since the last flush")
        // @end
    }

    /// Writes the page to disk now, keeping the pin and the latch, and marks it clean.
    pub fn flush(&mut self) {
        // @begin 1g-02
        let data: PageData = *self.get_data();
        self.bpm.write_page_data(self.page_id, &data);
        self.is_dirty = false;
        //~ todo!("1g-02: write the bytes you hold to disk, without taking the frame latch again; the page is now clean")
        // @end
    }

    /// Unlatches and unpins the page, telling the pool whether it was modified. Does nothing if already released.
    pub fn release(&mut self) {
        // @begin 1g-01
        if let Some(latch) = self.latch.take() {
            drop(latch);
            self.bpm.unpin_page(self.page_id, self.is_dirty);
        }
        //~ todo!("1g-01: if the guard still holds its latch: release it, then unpin the page, passing on whether it was dirtied; a second call does nothing")
        // @end
    }
}

impl Deref for WritePageGuard<'_> {
    type Target = PageData;
    fn deref(&self) -> &PageData {
        self.get_data()
    }
}

impl DerefMut for WritePageGuard<'_> {
    fn deref_mut(&mut self) -> &mut PageData {
        self.get_data_mut()
    }
}

impl Drop for WritePageGuard<'_> {
    fn drop(&mut self) {
        // @begin 1g-01
        self.release();
        //~ // TODO(1g-01): dropping a guard releases it
        // @end
    }
}
