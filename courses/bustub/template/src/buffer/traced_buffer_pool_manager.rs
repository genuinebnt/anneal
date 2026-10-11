//! A buffer pool wrapper that counts the pages latched for reading and for writing, to bound how much an operation latches.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::buffer_pool_manager::BufferPoolManager;
use crate::common::config::PageId;
use crate::storage::page::page_guard::{ReadPageGuard, WritePageGuard};

pub struct TracedBufferPoolManager<'a> {
    bpm: &'a BufferPoolManager,
    reads: AtomicUsize,
    writes: AtomicUsize,
}

impl<'a> TracedBufferPoolManager<'a> {
    pub fn new(bpm: &'a BufferPoolManager) -> TracedBufferPoolManager<'a> {
        TracedBufferPoolManager { bpm, reads: AtomicUsize::new(0), writes: AtomicUsize::new(0) }
    }

    /// The buffer pool underneath, for calls that are not counted.
    pub fn inner(&self) -> &'a BufferPoolManager {
        self.bpm
    }

    pub fn new_page(&self) -> PageId {
        self.bpm.new_page()
    }

    pub fn delete_page(&self, page_id: PageId) -> bool {
        self.bpm.delete_page(page_id)
    }

    /// Latches `page_id` for reading, and counts it.
    pub fn read_page(&self, page_id: PageId) -> ReadPageGuard<'a> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.bpm.read_page(page_id)
    }

    /// Latches `page_id` for writing, and counts it.
    pub fn write_page(&self, page_id: PageId) -> WritePageGuard<'a> {
        self.writes.fetch_add(1, Ordering::Relaxed);
        self.bpm.write_page(page_id)
    }

    pub fn get_reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    pub fn get_writes(&self) -> usize {
        self.writes.load(Ordering::Relaxed)
    }
}
