//! A write-through page cache in front of any disk.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};

use crate::common::config::{PageData, PageId};
use crate::storage::disk::disk_manager::DiskIo;

struct Cache {
    pages: HashMap<PageId, Box<PageData>>,
    /// Oldest first: what to drop when the cache is full.
    order: VecDeque<PageId>,
}

/// Keeps up to `capacity` recently used pages in memory. Writes go to the disk at once (write-through), so the disk is always current; the
/// cache only saves reads. To everyone using it, it behaves exactly like the disk underneath.
pub struct CachedDisk {
    inner: Arc<dyn DiskIo>,
    capacity: usize,
    cache: Mutex<Cache>,
}

impl CachedDisk {
    pub fn new(inner: Arc<dyn DiskIo>, capacity: usize) -> CachedDisk {
        CachedDisk { inner, capacity, cache: Mutex::new(Cache { pages: HashMap::new(), order: VecDeque::new() }) }
    }

    fn remember(&self, cache: &mut Cache, page_id: PageId, data: &PageData) {
        if cache.pages.insert(page_id, Box::new(*data)).is_none() {
            cache.order.push_back(page_id);
        }
        while cache.pages.len() > self.capacity {
            if let Some(old) = cache.order.pop_front() {
                cache.pages.remove(&old);
            }
        }
    }
}

impl DiskIo for CachedDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(page) = cache.pages.get(&page_id) {
            *buf = **page;
            return Ok(());
        }
        self.inner.read_page(page_id, buf)?;
        self.remember(&mut cache, page_id, buf);
        Ok(())
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        let mut cache = self.cache.lock().unwrap();
        self.inner.write_page(page_id, data)?;
        self.remember(&mut cache, page_id, data);
        Ok(())
    }

    fn delete_page(&self, page_id: PageId) {
        // TODO(1a-c2): this looks right and has a bug; find it and fix it
        self.inner.delete_page(page_id);
    }
}
