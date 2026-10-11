//! A disk that limits how fast it is written to, with a token bucket. Time comes from a [`Clock`] so that a test can move it by hand.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::config::{PageData, PageId};
use crate::storage::disk::disk_manager::DiskIo;

/// A source of time: how long since some fixed moment. Never goes backwards.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

/// Lets writes through at a limited rate and refuses the rest (a write that is refused fails at once with `ErrorKind::WouldBlock` and does not
/// reach the disk underneath; reads are never limited). The limit is a **token bucket**: it holds up to `burst` tokens and starts full; every
/// second `writes_per_second` tokens are added (continuously, not once a second), never above `burst`; a write takes one whole token.
pub struct ThrottledDisk {
    _throttle: (),
}

const ONE: u128 = 1_000_000_000;

impl ThrottledDisk {
    /// A throttled disk over `inner` that starts with a full bucket of `burst` tokens and gains `writes_per_second` tokens a second.
    pub fn new(inner: Arc<dyn DiskIo>, clock: Arc<dyn Clock>, writes_per_second: u32, burst: u32) -> ThrottledDisk {
        todo!("1b-c1: a throttled disk with a full bucket")
    }
}

impl DiskIo for ThrottledDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        todo!("1b-c1: reads are never limited")
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        todo!("1b-c1: take a token if there is one, else refuse; the bucket refills with time")
    }

    fn delete_page(&self, page_id: PageId) {
        todo!("1b-c1: deletes are never limited")
    }
}
