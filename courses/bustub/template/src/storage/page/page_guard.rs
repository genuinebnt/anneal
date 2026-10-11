//! A page guard is an RAII handle on a page in the buffer pool. While it exists the page is **pinned** (it can't be evicted) and **latched**
//! (shared for a `ReadPageGuard`, exclusive for a `WritePageGuard`). When it is dropped, in that order, the page is unlatched and then unpinned, so a
//! caller can't forget either, even on an early `return`, `?` or panic.

use std::ops::{Deref, DerefMut};

use crate::common::config::{PageData, PageId};

use std::marker::PhantomData;
// TODO(1g-01): your imports go here.

/// Shared access to a page. Many readers may hold one for the same page at once.
pub struct ReadPageGuard<'a> {
    _guard: PhantomData<&'a ()>,
    // TODO(1g-01): the fields are yours: the pool, the page, and whatever keeps the page latched.
}

// TODO(1g-01): the pool builds a guard when it hands out a page: a constructor of your own goes here.

impl ReadPageGuard<'_> {
    /// The id of the page this guard holds.
    pub fn get_page_id(&self) -> PageId {
        todo!("1g-01: the id of the page this guard holds")
    }

    /// The page's bytes. Panics if the guard has been released.
    pub fn get_data(&self) -> &PageData {
        todo!("1g-01: the bytes behind the read latch you are holding (panic if the guard was released)")
    }

    /// Writes the page to disk now. The guard keeps its pin and latch.
    pub fn flush(&mut self) {
        todo!("1g-02: write the bytes you hold to disk, without taking the frame latch again (you already hold it)")
    }

    /// Unlatches and unpins the page. Does nothing if the guard was already released.
    pub fn release(&mut self) {
        todo!("1g-01: if the guard still holds its latch: release the latch, then unpin the page; a second call does nothing")
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
        // TODO(1g-01): dropping a guard releases it
    }
}

/// Exclusive access to a page. Taking `get_data_mut` marks the page dirty, so the pool writes it back before reusing its frame.
pub struct WritePageGuard<'a> {
    _guard: PhantomData<&'a ()>,
    // TODO(1g-01): the fields are yours.
}

// TODO(1g-01): a constructor of your own goes here.

impl WritePageGuard<'_> {
    /// The id of the page this guard holds.
    pub fn get_page_id(&self) -> PageId {
        todo!("1g-01: the id of the page this guard holds")
    }

    /// The page's bytes. Panics if the guard has been released.
    pub fn get_data(&self) -> &PageData {
        todo!("1g-01: the bytes behind the write latch")
    }

    /// The page's bytes for modification. Marks the page dirty.
    pub fn get_data_mut(&mut self) -> &mut PageData {
        todo!("1g-01: remember that the page is dirty, then hand out the bytes mutably")
    }

    /// True if `get_data_mut` was called since the last flush.
    pub fn is_dirty(&self) -> bool {
        todo!("1g-01: whether the page has been handed out for modification since the last flush")
    }

    /// Writes the page to disk now, keeping the pin and the latch, and marks it clean.
    pub fn flush(&mut self) {
        todo!("1g-02: write the bytes you hold to disk, without taking the frame latch again; the page is now clean")
    }

    /// Unlatches and unpins the page, telling the pool whether it was modified. Does nothing if already released.
    pub fn release(&mut self) {
        todo!("1g-01: if the guard still holds its latch: release it, then unpin the page, passing on whether it was dirtied; a second call does nothing")
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
        // TODO(1g-01): dropping a guard releases it
    }
}
