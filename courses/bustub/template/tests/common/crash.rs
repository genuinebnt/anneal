//! A disk that can "crash" (pages and a log that survive, nothing else) and a small world around it: a buffer pool, a log manager and a
//! store over the disk. Not part of the course: nothing here for you to write. Included by the tests that need it with
//! `#[path = "common/crash.rs"] mod crash;` (it needs `mod pool;` from `common/pool.rs` next to it).
#![allow(dead_code)]

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::{PageData, PageId, BUSTUB_PAGE_SIZE};
use bustub::recovery::log_io::LogIo;
use bustub::recovery::log_manager::LogManager;
use bustub::recovery::log_record::TxnId;
use bustub::recovery::store::Store;
use bustub::storage::disk::disk_manager::DiskIo;

use crate::pool::FifoReplacer;

/// Pages and a log that are exactly what has been written: what is in memory is not here.
pub struct CrashDisk {
    pages: Mutex<HashMap<i32, Box<PageData>>>,
    log: Mutex<Vec<u8>>,
    pub page_writes: AtomicUsize,
    pub log_appends: AtomicUsize,
}

impl CrashDisk {
    pub fn new() -> Arc<CrashDisk> {
        Arc::new(CrashDisk {
            pages: Mutex::new(HashMap::new()),
            log: Mutex::new(Vec::new()),
            page_writes: AtomicUsize::new(0),
            log_appends: AtomicUsize::new(0),
        })
    }

    /// What a restart finds: a copy of the pages and the log as they are now.
    pub fn crash(&self) -> Arc<CrashDisk> {
        self.crash_tearing(0)
    }

    /// The same, but the last `cut` bytes of the log never reached the disk (a write cut in the middle).
    pub fn crash_tearing(&self, cut: usize) -> Arc<CrashDisk> {
        let mut log = self.log.lock().unwrap().clone();
        log.truncate(log.len().saturating_sub(cut));
        Arc::new(CrashDisk {
            pages: Mutex::new(self.pages.lock().unwrap().clone()),
            log: Mutex::new(log),
            page_writes: AtomicUsize::new(0),
            log_appends: AtomicUsize::new(0),
        })
    }

    pub fn log_bytes(&self) -> Vec<u8> {
        self.log.lock().unwrap().clone()
    }

    pub fn set_log_bytes(&self, bytes: Vec<u8>) {
        *self.log.lock().unwrap() = bytes;
    }

    pub fn stored(&self, page: i32) -> Box<PageData> {
        self.pages.lock().unwrap().get(&page).cloned().unwrap_or_else(|| Box::new([0; BUSTUB_PAGE_SIZE]))
    }
}

impl DiskIo for CrashDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        match self.pages.lock().unwrap().get(&page_id.0) {
            Some(p) => buf.copy_from_slice(&**p),
            None => buf.fill(0),
        }
        Ok(())
    }
    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        self.page_writes.fetch_add(1, Ordering::SeqCst);
        self.pages.lock().unwrap().insert(page_id.0, Box::new(*data));
        Ok(())
    }
    fn delete_page(&self, page_id: PageId) {
        self.pages.lock().unwrap().remove(&page_id.0);
    }
}

impl LogIo for CrashDisk {
    fn append(&self, data: &[u8]) -> io::Result<()> {
        self.log_appends.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().extend_from_slice(data);
        Ok(())
    }
    fn read_all(&self) -> io::Result<Vec<u8>> {
        Ok(self.log.lock().unwrap().clone())
    }
    fn truncate(&self, len: u64) -> io::Result<()> {
        self.log.lock().unwrap().truncate(len as usize);
        Ok(())
    }
}

/// A pool (big enough that nothing is evicted), a log manager and the pages of a store, over a [`CrashDisk`].
pub struct World {
    pub disk: Arc<CrashDisk>,
    pub bpm: BufferPoolManager,
    pub log: LogManager,
    pub pages: Vec<PageId>,
}

impl World {
    /// A new database: `page_count` formatted pages, an empty log.
    pub fn fresh(page_count: usize) -> World {
        let disk = CrashDisk::new();
        let bpm = BufferPoolManager::with_replacer(64, disk.clone(), Box::new(FifoReplacer::default()));
        let log = LogManager::new(disk.clone()).unwrap();
        let pages = Store::create(&bpm, &log, page_count).pages().to_vec();
        World { disk, bpm, log, pages }
    }

    /// The system after a restart over what a crash left on `disk`: a new pool (empty), a new log manager, the same pages.
    pub fn restart(disk: Arc<CrashDisk>, pages: &[PageId]) -> World {
        let bpm = BufferPoolManager::with_replacer(64, disk.clone(), Box::new(FifoReplacer::default()));
        let log = LogManager::new(disk.clone()).unwrap();
        World { disk, bpm, log, pages: pages.to_vec() }
    }

    pub fn store(&self, first_txn: TxnId) -> Store<'_> {
        Store::open(&self.bpm, &self.log, self.pages.clone(), first_txn)
    }

    /// Crash now and restart.
    pub fn crash_and_restart(&self) -> World {
        World::restart(self.disk.crash(), &self.pages)
    }
}
