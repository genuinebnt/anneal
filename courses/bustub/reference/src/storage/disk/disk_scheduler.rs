//! The disk scheduler runs disk reads and writes on a background thread. Callers hand it requests and wait on a future when they need the result,
//! so the buffer pool can start several I/Os and do other work meanwhile.

use std::io;
use std::sync::Arc;

use super::disk_manager::DiskIo;
use crate::common::config::{PageData, PageId};
use crate::common::promise::{Future, Promise};

// @begin 1b-03
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread::JoinHandle;

use crate::common::channel::{consume, Channel};
use crate::common::config::BUSTUB_PAGE_SIZE;
use crate::common::promise::promise;
//~ // TODO(1b-03): your imports go here.
// @end

/// What a finished request reports: the page buffer (the page read, or the page just written), or the I/O error.
pub type DiskResult = io::Result<Box<PageData>>;

/// One read or write of a page.
///
/// The request **owns** its buffer: a write carries the page to write; a read carries an empty buffer to fill. Either way the buffer comes back
/// through the future when the request is done.
pub struct DiskRequest {
    // @begin 1b-03
    is_write: bool,
    data: Box<PageData>,
    page_id: PageId,
    /// Completed with the result when the request is done.
    callback: Promise<DiskResult>,
    //~ // TODO(1b-03): the fields are yours: what a worker needs to do the I/O, and the way to answer the caller.
    // @end
}

impl DiskRequest {
    /// A read of `page_id`, and the future that will hold the page that was read.
    pub fn read(page_id: PageId) -> (DiskRequest, Future<DiskResult>) {
        // @begin 1b-03
        let (callback, future) = promise();
        (DiskRequest { is_write: false, data: Box::new([0; BUSTUB_PAGE_SIZE]), page_id, callback }, future)
        //~ todo!("1b-03: a request to read the page, and the future for its result")
        // @end
    }

    /// A write of `data` as `page_id`, and the future that will report whether it worked (and hand the buffer back).
    pub fn write(page_id: PageId, data: Box<PageData>) -> (DiskRequest, Future<DiskResult>) {
        // @begin 1b-03
        let (callback, future) = promise();
        (DiskRequest { is_write: true, data, page_id, callback }, future)
        //~ todo!("1b-03: a request to write the page, and the future for its result")
        // @end
    }
}

// @begin 1b-03
/// Does the I/O of one request.
fn run(disk: &dyn DiskIo, is_write: bool, page_id: PageId, data: &mut PageData) -> io::Result<()> {
    if is_write {
        disk.write_page(page_id, data)
    } else {
        disk.read_page(page_id, data)
    }
}

/// Runs one request against `disk`, then completes its callback with the buffer on success, or with the error.
fn execute(disk: &dyn DiskIo, request: DiskRequest) {
    let DiskRequest { is_write, mut data, page_id, callback } = request;
    // @begin 1b-04
    let result = catch_unwind(AssertUnwindSafe(|| run(disk, is_write, page_id, &mut data))).unwrap_or_else(|_| Err(io::Error::other("the disk panicked")));
    //~ let result = run(disk, is_write, page_id, &mut data);
    // @end
    callback.set(result.map(|()| data));
}
//~ // TODO(1b-03): private helpers go here.
// @end

/// Runs disk requests, one at a time and in the order they were scheduled, on a background thread.
pub struct DiskScheduler {
    // @begin 1b-03
    disk: Arc<dyn DiskIo>,
    /// `None` is the stop signal (BusTub's `std::nullopt`).
    request_queue: Arc<Channel<Option<DiskRequest>>>,
    background_thread: Option<JoinHandle<()>>,
    //~ // TODO(1b-03): the fields are yours; you will need the disk, a way to hand work to a thread, and the thread.
    // @end
}

impl DiskScheduler {
    /// Starts the background work.
    pub fn new(disk: Arc<dyn DiskIo>) -> DiskScheduler {
        // @begin 1b-03
        let request_queue = Arc::new(Channel::new());
        let worker_queue = Arc::clone(&request_queue);
        let worker_disk = Arc::clone(&disk);
        let background_thread = Some(std::thread::spawn(move || consume(&worker_queue, |request| execute(&*worker_disk, request))));
        DiskScheduler { disk, request_queue, background_thread }
        //~ todo!("1b-03: start whatever runs the requests in the background, and build the scheduler")
        // @end
    }

    /// Hands the requests over, in order. Returns at once; each request's future says when it is done. Requests are run one at
    /// a time in the order they were scheduled, so a read scheduled after a write of the same page sees that write.
    pub fn schedule(&self, requests: Vec<DiskRequest>) {
        // @begin 1b-03
        for request in requests {
            self.request_queue.put(Some(request));
        }
        //~ todo!("1b-03: hand each request to the worker, keeping their order")
        // @end
    }

    /// A promise and its future, for building a request by hand.
    pub fn create_promise(&self) -> (Promise<DiskResult>, Future<DiskResult>) {
        // @begin 1b-03
        promise()
        //~ todo!("1b-03: a fresh promise/future pair")
        // @end
    }

    /// Frees a page on the disk.
    pub fn deallocate_page(&self, page_id: PageId) {
        // @begin 1b-04
        self.disk.delete_page(page_id)
        //~ todo!("1b-04: ask the disk to delete the page")
        // @end
    }
}

// @begin 1b-04
impl Drop for DiskScheduler {
    /// Stops the worker after it has finished everything already scheduled, and waits for it.
    fn drop(&mut self) {
        self.request_queue.put(None);
        if let Some(thread) = self.background_thread.take() {
            let _ = thread.join();
        }
    }
}
//~ // TODO(1b-04): when the scheduler goes away, finish everything already scheduled and stop the worker (a `Drop` impl).
// @end

/// A scheduler with several workers that still runs the requests for each page in order: requests for page `p` go to worker
/// `p % workers`, so two requests for the same page can never overtake each other, and requests for different pages overlap.
pub struct ShardedDiskScheduler {
    // @begin 1b-06
    queues: Vec<Arc<Channel<Option<DiskRequest>>>>,
    workers: Vec<JoinHandle<()>>,
    //~ // TODO(1b-06): the fields are yours.
    // @end
}

impl ShardedDiskScheduler {
    /// `workers` background workers (at least one: panic otherwise).
    pub fn new(disk: Arc<dyn DiskIo>, workers: usize) -> ShardedDiskScheduler {
        // @begin 1b-06
        assert!(workers > 0, "a scheduler needs at least one worker");
        let mut queues = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..workers {
            let queue: Arc<Channel<Option<DiskRequest>>> = Arc::new(Channel::new());
            let (worker_queue, worker_disk) = (Arc::clone(&queue), Arc::clone(&disk));
            handles.push(std::thread::spawn(move || consume(&worker_queue, |request| execute(&*worker_disk, request))));
            queues.push(queue);
        }
        ShardedDiskScheduler { queues, workers: handles }
        //~ todo!("1b-06: start the workers")
        // @end
    }

    /// Like `DiskScheduler::schedule`, but requests for different pages may run at the same time.
    pub fn schedule(&self, requests: Vec<DiskRequest>) {
        // @begin 1b-06
        for request in requests {
            let shard = request.page_id.0.rem_euclid(self.queues.len() as i32) as usize;
            self.queues[shard].put(Some(request));
        }
        //~ todo!("1b-06: send each request to a worker so that requests for one page stay in order")
        // @end
    }
}

// @begin 1b-06
impl Drop for ShardedDiskScheduler {
    fn drop(&mut self) {
        for queue in &self.queues {
            queue.put(None);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
//~ // TODO(1b-06): stop and join every worker when the scheduler goes away.
// @end
