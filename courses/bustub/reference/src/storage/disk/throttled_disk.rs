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
    // @begin 1b-c1
    inner: Arc<dyn DiskIo>,
    clock: Arc<dyn Clock>,
    rate: u128,
    burst: u128,
    /// Tokens in billionths of a token (nanoseconds times rate counts exactly), and when they were last brought up to date.
    bucket: Mutex<(u128, Duration)>,
    //~ _throttle: (),
    // @end
}

const ONE: u128 = 1_000_000_000;

impl ThrottledDisk {
    /// A throttled disk over `inner` that starts with a full bucket of `burst` tokens and gains `writes_per_second` tokens a second.
    pub fn new(inner: Arc<dyn DiskIo>, clock: Arc<dyn Clock>, writes_per_second: u32, burst: u32) -> ThrottledDisk {
        // @begin 1b-c1
        let start = clock.now();
        ThrottledDisk { inner, clock, rate: writes_per_second as u128, burst: burst as u128 * ONE, bucket: Mutex::new((burst as u128 * ONE, start)) }
        //~ todo!("1b-c1: a throttled disk with a full bucket")
        // @end
    }
}

impl DiskIo for ThrottledDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        // @begin 1b-c1
        self.inner.read_page(page_id, buf)
        //~ todo!("1b-c1: reads are never limited")
        // @end
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        // @begin 1b-c1
        {
            let mut b = self.bucket.lock().unwrap();
            let now = self.clock.now();
            let elapsed = now.saturating_sub(b.1).as_nanos();
            b.0 = (b.0 + elapsed * self.rate).min(self.burst);
            b.1 = now;
            if b.0 < ONE {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "write rate limit reached"));
            }
            b.0 -= ONE;
        }
        self.inner.write_page(page_id, data)
        //~ todo!("1b-c1: take a token if there is one, else refuse; the bucket refills with time")
        // @end
    }

    fn delete_page(&self, page_id: PageId) {
        // @begin 1b-c1
        self.inner.delete_page(page_id)
        //~ todo!("1b-c1: deletes are never limited")
        // @end
    }
}
