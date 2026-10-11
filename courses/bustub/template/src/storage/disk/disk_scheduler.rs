//! The disk scheduler runs disk reads and writes on a background thread. Callers hand it requests and wait on a future when they need the result,
//! so the buffer pool can start several I/Os and do other work meanwhile.

use std::io;
use std::sync::Arc;

use super::disk_manager::DiskIo;
use crate::common::config::{PageData, PageId};
use crate::common::promise::{Future, Promise};

// TODO(1b-03): your imports go here.

/// What a finished request reports: the page buffer (the page read, or the page just written), or the I/O error.
pub type DiskResult = io::Result<Box<PageData>>;

/// One read or write of a page.
///
/// The request **owns** its buffer: a write carries the page to write; a read carries an empty buffer to fill. Either way the buffer comes back
/// through the future when the request is done.
pub struct DiskRequest {
    // TODO(1b-03): the fields are yours: what a worker needs to do the I/O, and the way to answer the caller.
}

impl DiskRequest {
    /// A read of `page_id`, and the future that will hold the page that was read.
    pub fn read(page_id: PageId) -> (DiskRequest, Future<DiskResult>) {
        todo!("1b-03: a request to read the page, and the future for its result")
    }

    /// A write of `data` as `page_id`, and the future that will report whether it worked (and hand the buffer back).
    pub fn write(page_id: PageId, data: Box<PageData>) -> (DiskRequest, Future<DiskResult>) {
        todo!("1b-03: a request to write the page, and the future for its result")
    }
}

// TODO(1b-03): private helpers go here.

/// Runs disk requests, one at a time and in the order they were scheduled, on a background thread.
pub struct DiskScheduler {
    // TODO(1b-03): the fields are yours; you will need the disk, a way to hand work to a thread, and the thread.
}

impl DiskScheduler {
    /// Starts the background work.
    pub fn new(disk: Arc<dyn DiskIo>) -> DiskScheduler {
        todo!("1b-03: start whatever runs the requests in the background, and build the scheduler")
    }

    /// Hands the requests over, in order. Returns at once; each request's future says when it is done. Requests are run one at
    /// a time in the order they were scheduled, so a read scheduled after a write of the same page sees that write.
    pub fn schedule(&self, requests: Vec<DiskRequest>) {
        todo!("1b-03: hand each request to the worker, keeping their order")
    }

    /// A promise and its future, for building a request by hand.
    pub fn create_promise(&self) -> (Promise<DiskResult>, Future<DiskResult>) {
        todo!("1b-03: a fresh promise/future pair")
    }

    /// Frees a page on the disk.
    pub fn deallocate_page(&self, page_id: PageId) {
        todo!("1b-04: ask the disk to delete the page")
    }
}

// TODO(1b-04): when the scheduler goes away, finish everything already scheduled and stop the worker (a `Drop` impl).

/// A scheduler with several workers that still runs the requests for each page in order: requests for page `p` go to worker
/// `p % workers`, so two requests for the same page can never overtake each other, and requests for different pages overlap.
pub struct ShardedDiskScheduler {
    // TODO(1b-06): the fields are yours.
}

impl ShardedDiskScheduler {
    /// `workers` background workers (at least one: panic otherwise).
    pub fn new(disk: Arc<dyn DiskIo>, workers: usize) -> ShardedDiskScheduler {
        todo!("1b-06: start the workers")
    }

    /// Like `DiskScheduler::schedule`, but requests for different pages may run at the same time.
    pub fn schedule(&self, requests: Vec<DiskRequest>) {
        todo!("1b-06: send each request to a worker so that requests for one page stay in order")
    }
}

// TODO(1b-06): stop and join every worker when the scheduler goes away.
