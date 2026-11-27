//! Test doubles for the buffer pool tests: a disk that records what it is asked, and a plain replacer. Not part of the course:
//! nothing here for you to write.
#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bustub::buffer::arc_replacer::ArcReplacer;
use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::buffer::lru_k_replacer::LruKReplacer;
use bustub::buffer::replacer::FrameReplacer;
use bustub::common::config::{FrameId, PageData, PageId, BUSTUB_PAGE_SIZE};
use bustub::storage::disk::disk_manager::DiskIo;

const PS: usize = BUSTUB_PAGE_SIZE;

// ---- Test doubles --------------------------------------------------------------------------------------------------------

/// An in-memory disk that records what the pool asks of it.
pub struct MemDisk {
    pub pages: Mutex<HashMap<i32, Box<PageData>>>,
    pub reads: AtomicUsize,
    /// Every page id written, in order.
    pub writes: Mutex<Vec<i32>>,
    pub deletes: Mutex<Vec<i32>>,
}

impl MemDisk {
    pub fn new() -> Arc<MemDisk> {
        Arc::new(MemDisk {
            pages: Mutex::new(HashMap::new()),
            reads: AtomicUsize::new(0),
            writes: Mutex::new(Vec::new()),
            deletes: Mutex::new(Vec::new()),
        })
    }
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
    pub fn writes(&self) -> Vec<i32> {
        self.writes.lock().unwrap().clone()
    }
    pub fn written_pages(&self) -> BTreeSet<i32> {
        self.writes().into_iter().collect()
    }
    /// What is stored for a page (zeros if never written).
    pub fn stored(&self, page: i32) -> Box<PageData> {
        self.pages.lock().unwrap().get(&page).cloned().unwrap_or_else(|| Box::new([0; PS]))
    }
}

impl DiskIo for MemDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        match self.pages.lock().unwrap().get(&page_id.0) {
            Some(page) => buf.copy_from_slice(&**page),
            None => buf.fill(0),
        }
        Ok(())
    }
    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        self.writes.lock().unwrap().push(page_id.0);
        self.pages.lock().unwrap().insert(page_id.0, Box::new(*data));
        Ok(())
    }
    fn delete_page(&self, page_id: PageId) {
        self.deletes.lock().unwrap().push(page_id.0);
        self.pages.lock().unwrap().remove(&page_id.0);
    }
}

/// The simplest correct replacer: evict the evictable frame that was recorded first.
#[derive(Default)]
pub struct FifoReplacer {
    pub frames: Vec<(FrameId, bool)>,
}

impl FrameReplacer for FifoReplacer {
    fn record_access(&mut self, frame: FrameId, _page: PageId) {
        if !self.frames.iter().any(|(f, _)| *f == frame) {
            self.frames.push((frame, false));
        }
    }
    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        if let Some(entry) = self.frames.iter_mut().find(|(f, _)| *f == frame) {
            entry.1 = evictable;
        }
    }
    fn evict(&mut self) -> Option<FrameId> {
        let at = self.frames.iter().position(|&(_, evictable)| evictable)?;
        Some(self.frames.remove(at).0)
    }
    fn remove(&mut self, frame: FrameId) {
        if let Some(at) = self.frames.iter().position(|(f, _)| *f == frame) {
            assert!(self.frames[at].1, "removing a frame that is not evictable");
            self.frames.remove(at);
        }
    }
    fn size(&self) -> usize {
        self.frames.iter().filter(|(_, e)| *e).count()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Policy {
    Fifo,
    Arc,
    LruK(usize),
}

pub fn pool_with(policy: Policy, frames: usize) -> (BufferPoolManager, Arc<MemDisk>) {
    let disk = MemDisk::new();
    let bpm = match policy {
        Policy::Fifo => BufferPoolManager::with_replacer(frames, disk.clone(), Box::new(FifoReplacer::default())),
        Policy::Arc => BufferPoolManager::with_replacer(frames, disk.clone(), Box::new(ArcReplacer::new(frames))),
        Policy::LruK(k) => BufferPoolManager::with_replacer(frames, disk.clone(), Box::new(LruKReplacer::new(frames, k))),
    };
    (bpm, disk)
}

pub fn pool(frames: usize) -> (BufferPoolManager, Arc<MemDisk>) {
    pool_with(Policy::Fifo, frames)
}

/// A `FifoReplacer` the test can also look at: the pool owns one handle, the test keeps the other.
#[derive(Clone, Default)]
pub struct SharedFifo(pub Arc<Mutex<FifoReplacer>>);

impl FrameReplacer for SharedFifo {
    fn record_access(&mut self, frame: FrameId, page: PageId) {
        self.0.lock().unwrap().record_access(frame, page)
    }
    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        self.0.lock().unwrap().set_evictable(frame, evictable)
    }
    fn evict(&mut self) -> Option<FrameId> {
        self.0.lock().unwrap().evict()
    }
    fn remove(&mut self, frame: FrameId) {
        self.0.lock().unwrap().remove(frame)
    }
    fn size(&self) -> usize {
        self.0.lock().unwrap().size()
    }
}

/// A pool on a `SharedFifo`, plus the handle to look at the replacer.
pub fn pool_with_spy(frames: usize) -> (BufferPoolManager, Arc<MemDisk>, SharedFifo) {
    let disk = MemDisk::new();
    let spy = SharedFifo::default();
    (BufferPoolManager::with_replacer(frames, disk.clone(), Box::new(spy.clone())), disk, spy)
}
