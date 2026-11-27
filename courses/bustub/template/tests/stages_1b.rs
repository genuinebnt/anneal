//! Tests for module 1b, the disk scheduler. A test name starts with its stage: `s1b_03_…` belongs to stage 1b-03, and
//! `anneal course test` runs just those.
//!
//! The tests use only the public API: `Channel`, `promise`, `DiskRequest`, `DiskScheduler`, `ShardedDiskScheduler` and
//! `ReaderWriterLatch`. Several are properties: a random sequence of operations is run on your code and on a plain model,
//! and every answer must agree. Anything that could hang waits with a timeout, so a deadlock fails the test with a message
//! instead of freezing it.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use bustub::common::channel::{consume, Channel};
use bustub::common::config::{PageData, PageId, BUSTUB_PAGE_SIZE};
use bustub::common::promise::{promise, BrokenPromise, Future};
use bustub::common::rwlatch::ReaderWriterLatch;
use bustub::storage::disk::disk_manager::DiskIo;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;
use bustub::storage::disk::disk_scheduler::{DiskRequest, DiskResult, DiskScheduler, ShardedDiskScheduler};
use proptest::prelude::*;

const PS: usize = BUSTUB_PAGE_SIZE;
const WAIT: Duration = Duration::from_secs(10);
const SHORT: Duration = Duration::from_millis(150);

fn config() -> ProptestConfig {
    ProptestConfig { cases: 32, max_shrink_iters: 1000, failure_persistence: None, ..ProptestConfig::default() }
}

/// Runs `f` on a thread and returns its result, or fails the test if it does not finish in time: a hang is a bug, not a stuck run.
fn within<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(WAIT).unwrap_or_else(|_| panic!("timed out ({WAIT:?}): {what}"))
}

/// Waits for a future, but not forever.
fn wait<T: Send + 'static>(future: Future<T>) -> Result<T, BrokenPromise> {
    within("waiting for a future: a promise that is never set, or one dropped without waking the future", move || future.get())
}

fn page_of(byte: u8) -> Box<PageData> {
    Box::new([byte; PS])
}

fn memory_disk() -> Arc<DiskManagerUnlimitedMemory> {
    Arc::new(DiskManagerUnlimitedMemory::new())
}

/// A disk that keeps pages in memory, logs every operation ("R3", "W3", "D3") in the order the worker performed it, can be slowed
/// down, and can fail or panic for chosen pages.
struct TestDisk {
    inner: DiskManagerUnlimitedMemory,
    log: Mutex<Vec<String>>,
    delay: Duration,
    fail_page: Option<i32>,
    panic_page: Option<i32>,
}

impl TestDisk {
    fn new() -> TestDisk {
        TestDisk { inner: DiskManagerUnlimitedMemory::new(), log: Mutex::new(Vec::new()), delay: Duration::ZERO, fail_page: None, panic_page: None }
    }
    fn slow(ms: u64) -> TestDisk {
        TestDisk { delay: Duration::from_millis(ms), ..TestDisk::new() }
    }
    fn ops(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl DiskIo for TestDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        thread::sleep(self.delay);
        self.log.lock().unwrap().push(format!("R{}", page_id.0));
        if Some(page_id.0) == self.panic_page {
            panic!("this disk panics on page {}", page_id.0);
        }
        if Some(page_id.0) == self.fail_page {
            return Err(io::Error::other(format!("boom on page {}", page_id.0)));
        }
        self.inner.read_page(page_id, buf)
    }
    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        thread::sleep(self.delay);
        self.log.lock().unwrap().push(format!("W{}", page_id.0));
        if Some(page_id.0) == self.panic_page {
            panic!("this disk panics on page {}", page_id.0);
        }
        if Some(page_id.0) == self.fail_page {
            return Err(io::Error::other(format!("boom on page {}", page_id.0)));
        }
        self.inner.write_page(page_id, data)
    }
    fn delete_page(&self, page_id: PageId) {
        self.log.lock().unwrap().push(format!("D{}", page_id.0));
        self.inner.delete_page(page_id)
    }
}

// ---- 1b-01 · A queue that waits -----------------------------------------------------------------------------------

#[test]
fn s1b_01_put_never_blocks_and_len_counts_what_is_waiting() {
    let ch = within("putting into a channel nobody reads", || {
        let ch = Channel::new();
        for i in 0..10_000 {
            ch.put(i);
        }
        ch
    });
    assert_eq!(ch.len(), 10_000, "ten thousand elements are waiting");
    assert!(!ch.is_empty());
    assert!(Channel::<u8>::new().is_empty(), "a new channel is empty");
}

#[test]
fn s1b_01_get_waits_until_something_is_put() {
    let ch = Arc::new(Channel::new());
    let (tx, rx) = mpsc::channel();
    let getter = {
        let ch = Arc::clone(&ch);
        thread::spawn(move || tx.send(ch.get()).unwrap())
    };
    assert!(rx.recv_timeout(SHORT).is_err(), "get returned from an empty channel instead of waiting");
    ch.put(42);
    assert_eq!(rx.recv_timeout(WAIT).expect("get never noticed the put: a waiting getter must be woken"), 42);
    getter.join().unwrap();
}

#[test]
fn s1b_01_consume_calls_f_on_each_element_in_order_and_eats_the_stop_signal() {
    let ch = Channel::new();
    for x in [Some(1), Some(2), None, Some(3)] {
        ch.put(x);
    }
    let mut seen = Vec::new();
    within("consume must return when it gets a None", move || {
        consume(&ch, |x| seen.push(x));
        (seen, ch)
    });
}

#[test]
fn s1b_01_consume_leaves_what_follows_the_stop_signal_in_the_channel() {
    let ch = Channel::new();
    for x in [Some(1), Some(2), None, Some(3)] {
        ch.put(x);
    }
    let (seen, ch) = within("consume must return when it gets a None", move || {
        let mut seen = Vec::new();
        consume(&ch, |x| seen.push(x));
        (seen, ch)
    });
    assert_eq!(seen, vec![1, 2], "f is called on the Some elements before the None, in order");
    assert_eq!(ch.len(), 1, "the None itself is consumed, and the element after it stays");
}

#[test]
fn s1b_01_every_element_reaches_exactly_one_of_many_consumers() {
    let ch: Arc<Channel<Option<u32>>> = Arc::new(Channel::new());
    let consumers: Vec<_> = (0..4)
        .map(|_| {
            let ch = Arc::clone(&ch);
            thread::spawn(move || {
                let mut got = Vec::new();
                consume(&ch, |x| got.push(x));
                got
            })
        })
        .collect();
    let producers: Vec<_> = (0..4u32)
        .map(|p| {
            let ch = Arc::clone(&ch);
            thread::spawn(move || {
                for i in 0..500 {
                    ch.put(Some(p * 1000 + i));
                }
            })
        })
        .collect();
    for p in producers {
        p.join().unwrap();
    }
    for _ in 0..4 {
        ch.put(None); // one stop signal per consumer
    }
    let mut all: Vec<u32> = within("the consumers should all stop", move || consumers.into_iter().flat_map(|c| c.join().unwrap()).collect());
    all.sort();
    let mut expected: Vec<u32> = (0..4u32).flat_map(|p| (0..500).map(move |i| p * 1000 + i)).collect();
    expected.sort();
    assert_eq!(all, expected, "every element must be delivered exactly once: none lost, none twice");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1b_01_a_single_thread_sees_a_first_in_first_out_queue(ops in prop::collection::vec(prop::option::of(any::<i32>()), 1..80)) {
        // Some(x) = put x; None = get (only done when the model says something is waiting)
        let ch = Channel::new();
        let mut model: VecDeque<i32> = VecDeque::new();
        for op in ops {
            match op {
                Some(x) => { ch.put(x); model.push_back(x); }
                None if !model.is_empty() => prop_assert_eq!(ch.get(), model.pop_front().unwrap(), "get must return the oldest element"),
                None => {}
            }
            prop_assert_eq!(ch.len(), model.len(), "len must be the number of elements waiting");
        }
    }
}

// ---- 1b-02 · A promise to answer later ------------------------------------------------------------------------

#[test]
fn s1b_02_a_value_set_before_get_is_returned() {
    let (p, f) = promise();
    assert!(!f.is_ready(), "nothing has been set yet");
    p.set(vec![1, 2, 3]);
    assert!(f.is_ready(), "a set value makes the future ready");
    assert_eq!(f.get(), Ok(vec![1, 2, 3]), "values that are not Clone move through");
}

#[test]
fn s1b_02_get_waits_for_a_promise_that_is_set_later_on_another_thread() {
    let (p, f) = promise();
    let (tx, rx) = mpsc::channel();
    let waiter = thread::spawn(move || tx.send(f.get()).unwrap());
    assert!(rx.recv_timeout(SHORT).is_err(), "get returned before anything was set");
    p.set("hello".to_string());
    assert_eq!(rx.recv_timeout(WAIT).expect("a set value must wake the waiting future"), Ok("hello".to_string()));
    waiter.join().unwrap();
}

#[test]
fn s1b_02_a_promise_dropped_without_a_value_breaks_the_future() {
    let (p, f) = promise::<u32>();
    drop(p);
    assert!(f.is_ready(), "a broken promise makes get return at once");
    assert_eq!(f.get(), Err(BrokenPromise));
}

#[test]
fn s1b_02_dropping_the_promise_wakes_a_future_that_is_already_waiting() {
    let (p, f) = promise::<u32>();
    let (tx, rx) = mpsc::channel();
    let waiter = thread::spawn(move || tx.send(f.get()).unwrap());
    assert!(rx.recv_timeout(SHORT).is_err(), "get returned before the promise was dropped");
    drop(p);
    assert_eq!(rx.recv_timeout(WAIT).expect("a dropped promise must wake the waiting future, or the waiter hangs forever"), Err(BrokenPromise));
    waiter.join().unwrap();
}

#[test]
fn s1b_02_a_set_promise_that_is_then_dropped_still_gives_its_value() {
    let (p, f) = promise();
    p.set(7u8); // `set` takes the promise by value, so it is dropped here
    assert_eq!(f.get(), Ok(7), "the drop that follows a set must not turn the value into BrokenPromise");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1b_02_any_value_set_from_another_thread_arrives(value in prop::collection::vec(any::<u8>(), 0..64), set_first in any::<bool>()) {
        let (p, f) = promise();
        let expected = value.clone();
        let setter = thread::spawn(move || {
            if !set_first { thread::sleep(Duration::from_millis(5)); }
            p.set(value);
        });
        if set_first { setter.join().unwrap(); prop_assert_eq!(f.get(), Ok(expected)); } else { prop_assert_eq!(wait(f), Ok(expected)); setter.join().unwrap(); }
    }
}

// ---- 1b-03 · The scheduler -----------------------------------------------------------------------------------------

fn write_req(page: i32, byte: u8) -> (DiskRequest, Future<DiskResult>) {
    DiskRequest::write(PageId(page), page_of(byte))
}

#[test]
fn s1b_03_a_read_scheduled_after_a_write_of_the_same_page_sees_it() {
    let sched = DiskScheduler::new(memory_disk());
    let (w, wf) = write_req(4, 0xAB);
    let (r, rf) = DiskRequest::read(PageId(4));
    sched.schedule(vec![w, r]);
    assert!(wait(wf).unwrap().is_ok(), "the write succeeds");
    let page = wait(rf).unwrap().unwrap();
    assert!(page.iter().all(|&b| b == 0xAB), "the read must come back with the bytes the earlier write stored");
}

#[test]
fn s1b_03_a_page_never_written_reads_back_as_zeros() {
    let sched = DiskScheduler::new(memory_disk());
    let (r, rf) = DiskRequest::read(PageId(9));
    sched.schedule(vec![r]);
    assert!(wait(rf).unwrap().unwrap().iter().all(|&b| b == 0), "an unwritten page reads as zeros through the scheduler too");
}

#[test]
fn s1b_03_a_write_future_hands_the_buffer_back() {
    let sched = DiskScheduler::new(memory_disk());
    let (w, wf) = write_req(1, 0x11);
    sched.schedule(vec![w]);
    let buf = wait(wf).unwrap().unwrap();
    assert!(buf.iter().all(|&b| b == 0x11), "the buffer that comes back is the page that was written");
}

#[test]
fn s1b_03_schedule_returns_before_the_io_is_done() {
    // A write takes 600 ms; handing it over takes microseconds. The bound leaves room for a slow machine.
    let disk = Arc::new(TestDisk::slow(600));
    let sched = DiskScheduler::new(disk);
    let (w, wf) = write_req(0, 1);
    let start = Instant::now();
    sched.schedule(vec![w]);
    assert!(start.elapsed() < Duration::from_millis(300), "schedule must hand the request over and return, not do the I/O itself");
    assert!(wait(wf).unwrap().is_ok());
}

#[test]
fn s1b_03_create_promise_gives_a_connected_pair() {
    let sched = DiskScheduler::new(memory_disk());
    let (p, f) = sched.create_promise();
    p.set(Ok(page_of(5)));
    assert!(wait(f).unwrap().unwrap().iter().all(|&b| b == 5), "a promise from create_promise completes its own future");
}

#[derive(Clone, Debug)]
enum Req {
    Write(i32, u8),
    Read(i32),
}

proptest! {
    #![proptest_config(config())]

    /// Whatever batches the requests arrive in, the scheduler behaves like running them one after another on a model.
    #[test]
    fn s1b_03_scheduled_requests_behave_like_running_them_in_order_on_a_model(
        reqs in prop::collection::vec(prop_oneof![(0..6i32, any::<u8>()).prop_map(|(p, b)| Req::Write(p, b)), (0..6i32).prop_map(Req::Read)], 1..50),
        batch in 1usize..6,
    ) {
        let sched = DiskScheduler::new(memory_disk());
        let mut model: std::collections::HashMap<i32, u8> = std::collections::HashMap::new();
        let mut futures: Vec<(Req, u8, Future<DiskResult>)> = Vec::new();
        for chunk in reqs.chunks(batch) {
            let mut requests = Vec::new();
            for r in chunk {
                match r {
                    Req::Write(p, b) => { let (req, f) = write_req(*p, *b); model.insert(*p, *b); requests.push(req); futures.push((r.clone(), *b, f)); }
                    Req::Read(p) => { let (req, f) = DiskRequest::read(PageId(*p)); requests.push(req); futures.push((r.clone(), *model.get(p).unwrap_or(&0), f)); }
                }
            }
            sched.schedule(requests);
        }
        for (req, expected, f) in futures {
            let page = wait(f).unwrap().unwrap();
            prop_assert!(page.iter().all(|&b| b == expected), "{req:?} should have produced a page full of {expected}, in the order the requests were scheduled");
        }
    }
}

// ---- 1b-04 · Shutdown and a failing disk ----------------------------------------------------------------------

#[test]
fn s1b_04_dropping_the_scheduler_finishes_everything_scheduled_before_it() {
    let disk = Arc::new(TestDisk::slow(1));
    let sched = DiskScheduler::new(disk.clone());
    let requests: Vec<_> = (0..100).map(|i| write_req(i, i as u8).0).collect();
    sched.schedule(requests);
    drop(sched); // no future was waited for: the drop itself must run the queue dry
    assert_eq!(disk.ops().len(), 100, "all 100 writes must have reached the disk by the time the drop returns");
}

#[test]
fn s1b_04_dropping_an_idle_scheduler_does_not_hang() {
    within("dropping a scheduler with an empty queue", || drop(DiskScheduler::new(memory_disk())));
}

#[test]
fn s1b_04_a_disk_error_is_reported_through_the_future() {
    let mut disk = TestDisk::new();
    disk.fail_page = Some(3);
    let sched = DiskScheduler::new(Arc::new(disk));
    let (w, wf) = write_req(3, 1);
    sched.schedule(vec![w]);
    let result = wait(wf).expect("the promise must be completed even when the I/O fails");
    let err = result.expect_err("the write failed, so the future carries the error");
    assert!(err.to_string().contains("boom"), "the disk's own error reaches the caller: {err}");
}

#[test]
fn s1b_04_a_panicking_disk_gives_an_error_and_the_worker_keeps_going() {
    let mut disk = TestDisk::new();
    disk.panic_page = Some(13);
    let sched = DiskScheduler::new(Arc::new(disk));
    let (bad, bad_f) = write_req(13, 1);
    let (good, good_f) = write_req(2, 2);
    sched.schedule(vec![bad, good]);
    let outcome = wait(bad_f).expect("a promise the worker never completes leaves the caller waiting forever");
    assert!(outcome.is_err(), "a disk that panics must be reported as an I/O error");
    assert!(wait(good_f).unwrap().is_ok(), "the next request must still be served: one bad request must not kill the worker thread");
}

#[test]
fn s1b_04_deallocate_page_asks_the_disk_to_delete_it() {
    let disk = Arc::new(TestDisk::new());
    let sched = DiskScheduler::new(disk.clone());
    sched.deallocate_page(PageId(8));
    assert_eq!(disk.ops(), vec!["D8".to_string()], "the disk is asked to delete page 8, once");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1b_04_the_disk_sees_the_requests_in_the_order_they_were_scheduled(pages in prop::collection::vec(0..8i32, 1..40)) {
        let disk = Arc::new(TestDisk::new());
        let sched = DiskScheduler::new(disk.clone());
        let expected: Vec<String> = pages.iter().map(|p| format!("W{p}")).collect();
        sched.schedule(pages.iter().map(|&p| write_req(p, 1).0).collect());
        drop(sched);
        prop_assert_eq!(disk.ops(), expected);
    }
}

// ---- 1b-05 · The reader-writer latch ---------------------------------------------------------------------------

#[test]
fn s1b_05_a_write_changes_what_the_next_read_sees() {
    let latch = ReaderWriterLatch::new(vec![1, 2]);
    latch.write().push(3);
    assert_eq!(*latch.read(), vec![1, 2, 3]);
}

#[test]
fn s1b_05_many_writers_never_lose_an_update() {
    let latch = Arc::new(ReaderWriterLatch::new(0u64));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let latch = Arc::clone(&latch);
            thread::spawn(move || {
                for _ in 0..2000 {
                    *latch.write() += 1;
                }
            })
        })
        .collect();
    within("eight writers", move || handles.into_iter().for_each(|h| h.join().unwrap()));
    assert_eq!(*latch.read(), 16_000, "an increment under the write latch is never lost");
}

#[test]
fn s1b_05_a_reader_never_sees_a_write_half_done() {
    let latch = Arc::new(ReaderWriterLatch::new((0u32, 0u32)));
    let stop = Arc::new(AtomicUsize::new(0));
    let writer = {
        let (latch, stop) = (Arc::clone(&latch), Arc::clone(&stop));
        thread::spawn(move || {
            for i in 1..=3000 {
                let mut g = latch.write();
                g.0 = i;
                thread::yield_now(); // a window in which a reader could look, if the latch let it
                g.1 = i;
            }
            stop.store(1, Ordering::SeqCst);
        })
    };
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let (latch, stop) = (Arc::clone(&latch), Arc::clone(&stop));
            thread::spawn(move || {
                while stop.load(Ordering::SeqCst) == 0 {
                    let g = latch.read();
                    assert_eq!(g.0, g.1, "a reader saw the two halves of a write at different values");
                }
            })
        })
        .collect();
    within("the writer and readers", move || {
        writer.join().unwrap();
        readers.into_iter().for_each(|r| r.join().unwrap());
    });
}

#[test]
fn s1b_05_several_readers_can_hold_the_latch_at_the_same_time() {
    let latch = Arc::new(ReaderWriterLatch::new(1));
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (latch, barrier) = (Arc::clone(&latch), Arc::clone(&barrier));
            thread::spawn(move || {
                let _g = latch.read();
                barrier.wait(); // returns only if all four threads are inside the latch together
            })
        })
        .collect();
    within("four readers meeting inside the read latch: readers must not exclude each other", move || handles.into_iter().for_each(|h| h.join().unwrap()));
}

#[test]
fn s1b_05_a_writer_keeps_readers_out_until_it_lets_go() {
    let latch = Arc::new(ReaderWriterLatch::new(0));
    let guard = latch.write();
    let (tx, rx) = mpsc::channel();
    let reader = {
        let latch = Arc::clone(&latch);
        thread::spawn(move || tx.send(*latch.read()).unwrap())
    };
    assert!(rx.recv_timeout(SHORT).is_err(), "a reader got in while a writer held the latch");
    drop(guard);
    assert_eq!(rx.recv_timeout(WAIT).expect("the reader must be let in when the writer lets go"), 0);
    reader.join().unwrap();
}

#[test]
fn s1b_05_a_latch_is_still_usable_after_a_holder_panicked() {
    let latch = Arc::new(ReaderWriterLatch::new(5));
    let l2 = Arc::clone(&latch);
    let _ = thread::spawn(move || {
        let _g = l2.write();
        panic!("a holder panics");
    })
    .join();
    assert_eq!(*latch.read(), 5, "a latch whose holder panicked must still give its guard (poisoning is not this latch's business)");
    *latch.write() = 6;
    assert_eq!(*latch.read(), 6);
}

// ---- 1b-06 · Several workers, one page at a time ---------------------------------------------------------------

#[test]
fn s1b_06_a_sharded_scheduler_needs_at_least_one_worker() {
    let result = std::panic::catch_unwind(|| ShardedDiskScheduler::new(memory_disk(), 0));
    assert!(result.is_err(), "zero workers is a bug in the caller: it panics");
}

#[test]
fn s1b_06_requests_for_different_pages_overlap_in_time() {
    // Overlapped, 8 writes of 100 ms on 4 workers take about 200 ms; one after another they take 800 ms.
    let disk = Arc::new(TestDisk::slow(100));
    let sched = ShardedDiskScheduler::new(disk, 4);
    let start = Instant::now();
    let futures: Vec<_> = (0..8)
        .map(|p| {
            let (req, f) = write_req(p, 1);
            sched.schedule(vec![req]);
            f
        })
        .collect();
    for f in futures {
        assert!(wait(f).unwrap().is_ok());
    }
    let took = start.elapsed();
    assert!(took < Duration::from_millis(8 * 100 * 3 / 4), "8 writes of 100 ms to different pages on 4 workers took {took:?}: one worker would take 800 ms, they should overlap");
}

#[test]
fn s1b_06_dropping_an_idle_sharded_scheduler_does_not_hang() {
    within("dropping a sharded scheduler with empty queues", || drop(ShardedDiskScheduler::new(memory_disk(), 3)));
}

#[test]
fn s1b_06_dropping_a_sharded_scheduler_finishes_everything_scheduled() {
    let disk = Arc::new(TestDisk::slow(1));
    let sched = ShardedDiskScheduler::new(disk.clone(), 3);
    sched.schedule((0..60).map(|i| write_req(i, 1).0).collect());
    drop(sched);
    assert_eq!(disk.ops().len(), 60, "all 60 writes must have reached the disk when the drop returns");
}

proptest! {
    #![proptest_config(config())]

    /// The covariant: any number of workers gives the same answers as one, because requests for one page never overtake each other.
    #[test]
    fn s1b_06_requests_for_one_page_stay_in_order_whatever_the_number_of_workers(
        reqs in prop::collection::vec(prop_oneof![(0..5i32, any::<u8>()).prop_map(|(p, b)| Req::Write(p, b)), (0..5i32).prop_map(Req::Read)], 1..40),
        workers in 1usize..5,
    ) {
        let sched = ShardedDiskScheduler::new(memory_disk(), workers);
        let mut model: std::collections::HashMap<i32, u8> = std::collections::HashMap::new();
        let mut futures: Vec<(Req, u8, Future<DiskResult>)> = Vec::new();
        for r in &reqs {
            match r {
                Req::Write(p, b) => { let (req, f) = write_req(*p, *b); model.insert(*p, *b); sched.schedule(vec![req]); futures.push((r.clone(), *b, f)); }
                Req::Read(p) => { let (req, f) = DiskRequest::read(PageId(*p)); sched.schedule(vec![req]); futures.push((r.clone(), *model.get(p).unwrap_or(&0), f)); }
            }
        }
        for (req, expected, f) in futures {
            let page = wait(f).unwrap().unwrap();
            prop_assert!(page.iter().all(|&b| b == expected), "{req:?} with {workers} workers should have produced a page full of {expected}");
        }
    }
}

// ---- 1b-07 · Boss: threads on every side, and BusTub's own tests -------------------------------------------------

#[test]
fn s1b_07_many_threads_scheduling_at_once_each_get_their_own_answers() {
    let sched = Arc::new(DiskScheduler::new(memory_disk()));
    let handles: Vec<_> = (0..4i32)
        .map(|t| {
            let sched = Arc::clone(&sched);
            thread::spawn(move || {
                for i in 0..50i32 {
                    let page = t * 100 + i;
                    let (w, wf) = write_req(page, (page % 251) as u8);
                    let (r, rf) = DiskRequest::read(PageId(page));
                    sched.schedule(vec![w]);
                    sched.schedule(vec![r]);
                    assert!(wait(wf).unwrap().is_ok());
                    let got = wait(rf).unwrap().unwrap();
                    assert!(got.iter().all(|&b| b == (page % 251) as u8), "thread {t} read the wrong bytes for page {page}");
                }
            })
        })
        .collect();
    within("four scheduling threads", move || handles.into_iter().for_each(|h| h.join().unwrap()));
}

#[test]
fn s1b_07_the_scheduler_works_over_the_real_disk_manager_too() {
    let dir = std::env::temp_dir().join(format!("bustub-rs-1b-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dm = Arc::new(bustub::storage::disk::disk_manager::DiskManager::new(dir.join("t.bustub")).unwrap());
    let sched = DiskScheduler::new(dm);
    let (w, wf) = write_req(7, 0x5A);
    let (r, rf) = DiskRequest::read(PageId(7));
    sched.schedule(vec![w, r]);
    assert!(wait(wf).unwrap().is_ok());
    assert!(wait(rf).unwrap().unwrap().iter().all(|&b| b == 0x5A), "a page written through the scheduler to a real file reads back");
    drop(sched);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- 1b-c1 and 1b-c2: challenges ------------------------------------------------------------------------------------------------------------

use bustub::common::gate::Gate;
use bustub::storage::disk::throttled_disk::{Clock, ThrottledDisk};

/// A clock that moves only when the test says so.
struct ManualClock(std::sync::atomic::AtomicU64);

impl ManualClock {
    fn new() -> Arc<ManualClock> {
        Arc::new(ManualClock(std::sync::atomic::AtomicU64::new(0)))
    }
    fn advance(&self, d: Duration) {
        self.0.fetch_add(d.as_nanos() as u64, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::SeqCst))
    }
}

fn throttled(rate: u32, burst: u32) -> (ThrottledDisk, Arc<ManualClock>, Arc<DiskManagerUnlimitedMemory>) {
    let clock = ManualClock::new();
    let inner = Arc::new(DiskManagerUnlimitedMemory::new());
    (ThrottledDisk::new(inner.clone(), clock.clone(), rate, burst), clock, inner)
}

fn page(byte: u8) -> PageData {
    [byte; PS]
}

#[test]
fn s1b_c1_a_full_bucket_lets_a_burst_through_and_then_refuses() {
    let (d, _clock, inner) = throttled(1, 3);
    for i in 0..3 {
        d.write_page(PageId(i), &page(1)).expect("the first three writes use the burst");
    }
    let err = d.write_page(PageId(3), &page(1)).expect_err("the bucket is empty");
    assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    let mut buf = [0u8; PS];
    inner.read_page(PageId(3), &mut buf).unwrap();
    assert_eq!(buf, [0u8; PS], "a refused write never reaches the disk underneath");
}

#[test]
fn s1b_c1_tokens_come_back_with_time_continuously() {
    let (d, clock, _inner) = throttled(2, 1);
    d.write_page(PageId(0), &page(1)).unwrap();
    assert!(d.write_page(PageId(1), &page(1)).is_err());
    clock.advance(Duration::from_millis(250));
    assert!(d.write_page(PageId(1), &page(1)).is_err(), "a quarter of a second at 2 per second is half a token");
    clock.advance(Duration::from_millis(250));
    assert!(d.write_page(PageId(1), &page(1)).is_ok(), "two quarters make a whole token: tokens add up between calls");
    assert!(d.write_page(PageId(2), &page(1)).is_err());
}

#[test]
fn s1b_c1_the_bucket_never_holds_more_than_the_burst() {
    let (d, clock, _inner) = throttled(10, 2);
    clock.advance(Duration::from_secs(100));
    assert!(d.write_page(PageId(0), &page(1)).is_ok());
    assert!(d.write_page(PageId(1), &page(1)).is_ok());
    assert!(d.write_page(PageId(2), &page(1)).is_err(), "100 seconds of idleness do not buy more than the burst of 2");
}

#[test]
fn s1b_c1_reads_and_deletes_are_never_limited() {
    let (d, _clock, _inner) = throttled(1, 1);
    d.write_page(PageId(0), &page(7)).unwrap();
    assert!(d.write_page(PageId(1), &page(8)).is_err());
    let mut buf = [0u8; PS];
    for _ in 0..100 {
        d.read_page(PageId(0), &mut buf).unwrap();
        assert_eq!(buf, page(7));
    }
    d.delete_page(PageId(0));
}

#[test]
fn s1b_c1_many_threads_never_get_more_writes_than_there_are_tokens() {
    let (d, _clock, _inner) = throttled(1, 20);
    let d = Arc::new(d);
    let ok = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..8)
        .map(|t| {
            let (d, ok) = (Arc::clone(&d), Arc::clone(&ok));
            thread::spawn(move || {
                for i in 0..10 {
                    if d.write_page(PageId(t * 10 + i), &page(1)).is_ok() {
                        ok.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(ok.load(Ordering::SeqCst), 20, "80 attempts against a bucket of 20 tokens and a clock that does not move");
}

proptest! {
    #![proptest_config(config())]

    /// Property: against a model that counts tokens exactly (in billionths), for any mix of writes and pauses.
    #[test]
    fn s1b_c1_property_a_throttled_disk_matches_a_token_bucket(rate in 1u32..5, burst in 1u32..5, steps in proptest::collection::vec((any::<bool>(), 0u64..8), 1..60)) {
        let (d, clock, _inner) = throttled(rate, burst);
        let one: u128 = 1_000_000_000;
        let mut tokens: u128 = burst as u128 * one;
        for (i, (write, quarters)) in steps.into_iter().enumerate() {
            if !write {
                let dt = Duration::from_millis(250 * quarters);
                clock.advance(dt);
                tokens = (tokens + dt.as_nanos() * rate as u128).min(burst as u128 * one);
                continue;
            }
            let want = if tokens >= one { tokens -= one; true } else { false };
            let got = d.write_page(PageId(i as i32), &page(1)).is_ok();
            prop_assert_eq!(got, want, "write number {}", i);
        }
    }
}

#[test]
fn s1b_c2_a_gate_that_is_open_lets_everybody_through() {
    let g = Gate::new();
    assert!(!g.is_open());
    g.open();
    assert!(g.is_open());
    g.wait();
    g.wait();
    g.open();
    assert!(g.is_open(), "opening twice is fine");
}

#[test]
fn s1b_c2_a_waiter_goes_on_when_the_gate_opens() {
    let g = Arc::new(Gate::new());
    let (tx, rx) = mpsc::channel();
    let h = {
        let g = Arc::clone(&g);
        thread::spawn(move || {
            g.wait();
            tx.send(()).unwrap();
        })
    };
    assert!(rx.recv_timeout(SHORT).is_err(), "the gate is closed: the waiter waits");
    g.open();
    rx.recv_timeout(WAIT).expect("the waiter goes on once the gate is open");
    h.join().unwrap();
}

#[test]
fn s1b_c2_every_waiter_goes_on_not_just_one() {
    let g = Arc::new(Gate::new());
    let (tx, rx) = mpsc::channel();
    let hs: Vec<_> = (0..4)
        .map(|i| {
            let (g, tx) = (Arc::clone(&g), tx.clone());
            thread::spawn(move || {
                g.wait();
                tx.send(i).unwrap();
            })
        })
        .collect();
    thread::sleep(SHORT);
    g.open();
    let mut got = Vec::new();
    for _ in 0..4 {
        got.push(rx.recv_timeout(WAIT).expect("a waiter was left behind: opening the gate must release everyone who waits at it"));
    }
    got.sort();
    assert_eq!(got, vec![0, 1, 2, 3]);
    for h in hs {
        h.join().unwrap();
    }
}

#[test]
fn s1b_c2_a_late_arrival_does_not_wait() {
    let g = Arc::new(Gate::new());
    g.open();
    let (tx, rx) = mpsc::channel();
    let h = {
        let g = Arc::clone(&g);
        thread::spawn(move || {
            g.wait();
            tx.send(()).unwrap();
        })
    };
    rx.recv_timeout(WAIT).expect("a gate that is already open lets a late waiter through at once");
    h.join().unwrap();
}

#[test]
fn s1b_c2_gates_are_independent() {
    let (a, b) = (Arc::new(Gate::new()), Arc::new(Gate::new()));
    let (tx, rx) = mpsc::channel();
    let h = {
        let (b, tx) = (Arc::clone(&b), tx.clone());
        thread::spawn(move || {
            b.wait();
            tx.send(()).unwrap();
        })
    };
    a.open();
    assert!(rx.recv_timeout(SHORT).is_err(), "opening one gate does not open another");
    b.open();
    rx.recv_timeout(WAIT).unwrap();
    h.join().unwrap();
}

// @@ challenge 1b-c3 begin
mod ch_1b_c3 {
    use proptest::prelude::*;

    use bustub::storage::disk::request_queue::RequestQueue;

    #[test]
    fn s1b_c3_the_most_urgent_goes_first_and_ties_go_in_arrival_order() {
        let mut q = RequestQueue::new();
        for (p, x) in [(1, 'a'), (3, 'b'), (3, 'c'), (2, 'd')] {
            q.push(p, x);
        }
        assert_eq!(q.peek_priority(), Some(3));
        assert_eq!((q.pop(), q.pop(), q.pop(), q.pop(), q.pop()), (Some('b'), Some('c'), Some('d'), Some('a'), None));
    }

    #[test]
    fn s1b_c3_a_late_urgent_request_overtakes_older_ones() {
        let mut q = RequestQueue::new();
        q.push(1, "old1");
        q.push(1, "old2");
        q.push(9, "urgent");
        assert_eq!(q.pop(), Some("urgent"));
        assert_eq!(q.pop(), Some("old1"), "equals keep their arrival order");
    }

    #[test]
    fn s1b_c3_an_empty_queue_has_nothing() {
        let mut q: RequestQueue<u8> = RequestQueue::new();
        assert!(q.is_empty());
        assert_eq!((q.pop(), q.peek_priority(), q.len()), (None, None, 0));
    }

    #[test]
    fn s1b_c3_items_need_no_ordering_of_their_own() {
        struct NoOrd(u32);
        let mut q = RequestQueue::new();
        q.push(2, NoOrd(10));
        q.push(2, NoOrd(20));
        assert_eq!(q.pop().map(|x| x.0), Some(10));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the pop order is the push order stably sorted by descending priority, with pops mixed between pushes.
        #[test]
        fn s1b_c3_property_pops_equal_a_stable_sort(ops in proptest::collection::vec(prop_oneof![(0u8..4, any::<u16>()).prop_map(Some), Just(None)], 0..60)) {
            let mut q = RequestQueue::new();
            let mut model: Vec<(u8, u16)> = Vec::new(); // in arrival order
            for op in ops {
                match op {
                    Some((p, x)) => { q.push(p, x); model.push((p, x)); }
                    None => {
                        let best = model.iter().enumerate().max_by_key(|&(i, &(p, _))| (p, std::cmp::Reverse(i))).map(|(i, _)| i);
                        prop_assert_eq!(q.peek_priority(), best.map(|i| model[i].0));
                        prop_assert_eq!(q.pop(), best.map(|i| model.remove(i).1));
                    }
                }
                prop_assert_eq!(q.len(), model.len());
            }
        }
    }
}
// @@ challenge 1b-c3 end

// @@ challenge 1b-c4 begin
mod ch_1b_c4 {
    use proptest::prelude::*;

    use bustub::storage::disk::coalesce::{coalesce, Op};
    use std::collections::HashMap;

    use Op::{Read as R, Write as W};

    /// Replays operations on a disk and returns what the reads saw, and the final contents.
    fn replay(ops: &[Op]) -> (Vec<Option<u8>>, Vec<(u32, u8)>) {
        let mut disk: HashMap<u32, u8> = HashMap::new();
        let mut seen = Vec::new();
        for op in ops {
            match *op {
                W(p, v) => {
                    disk.insert(p, v);
                }
                R(p) => seen.push(disk.get(&p).copied()),
            }
        }
        let mut fin: Vec<_> = disk.into_iter().collect();
        fin.sort();
        (seen, fin)
    }

    #[test]
    fn s1b_c4_repeated_writes_to_one_page_become_the_last_one() {
        assert_eq!(coalesce(&[W(1, 1), W(1, 2), W(1, 3)]), vec![W(1, 3)]);
    }

    #[test]
    fn s1b_c4_a_read_in_between_protects_the_write_it_would_see() {
        assert_eq!(coalesce(&[W(1, 1), R(1), W(1, 2)]), vec![W(1, 1), R(1), W(1, 2)]);
    }

    #[test]
    fn s1b_c4_writes_to_other_pages_in_between_do_not_matter() {
        assert_eq!(coalesce(&[W(1, 1), W(2, 9), W(1, 2)]), vec![W(2, 9), W(1, 2)]);
    }

    #[test]
    fn s1b_c4_a_read_of_another_page_does_not_protect_a_write() {
        assert_eq!(coalesce(&[W(1, 1), R(2), W(1, 2)]), vec![R(2), W(1, 2)]);
    }

    #[test]
    fn s1b_c4_empty_and_single_operations() {
        assert_eq!(coalesce(&[]), vec![]);
        assert_eq!(coalesce(&[R(5)]), vec![R(5)]);
        assert_eq!(coalesce(&[W(5, 1)]), vec![W(5, 1)]);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: same read results and final contents, idempotent, never longer, and minimal (no two writes to a page without a read between).
        #[test]
        fn s1b_c4_property_coalescing_changes_no_read_and_is_minimal(raw in proptest::collection::vec((any::<bool>(), 0u32..4, 0u8..5), 0..40)) {
            let ops: Vec<Op> = raw.into_iter().map(|(w, p, v)| if w { W(p, v) } else { R(p) }).collect();
            let out = coalesce(&ops);
            prop_assert_eq!(replay(&out), replay(&ops));
            prop_assert!(out.len() <= ops.len());
            prop_assert_eq!(coalesce(&out), out.clone());
            prop_assert_eq!(out.iter().filter(|o| matches!(o, R(_))).count(), ops.iter().filter(|o| matches!(o, R(_))).count());
            for page in 0..4 {
                let mut since_write = false; // a write is pending with no read since
                for op in &out {
                    match *op {
                        W(p, _) if p == page => {
                            prop_assert!(!since_write, "two writes to page {} with no read between them in {:?}", page, out);
                            since_write = true;
                        }
                        R(p) if p == page => since_write = false,
                        _ => {}
                    }
                }
            }
        }
    }
}
// @@ challenge 1b-c4 end

// @@ challenge 1b-c5 begin
mod ch_1b_c5 {
    use proptest::prelude::*;

    use bustub::storage::disk::job_pool::JobPool;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn run(workers: usize, jobs: usize) -> usize {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut pool = JobPool::new(workers);
        let mut accepted = 0;
        for _ in 0..jobs {
            let c = Arc::clone(&counter);
            if pool.submit(move || {
                std::thread::yield_now();
                c.fetch_add(1, Ordering::SeqCst);
            }) {
                accepted += 1;
            }
        }
        pool.shutdown();
        assert_eq!(accepted, jobs);
        counter.load(Ordering::SeqCst)
    }

    #[test]
    fn s1b_c5_every_submitted_job_runs_before_shutdown_returns() {
        assert_eq!(run(4, 200), 200, "jobs were still queued when the workers stopped");
    }

    #[test]
    fn s1b_c5_one_worker_and_many_workers() {
        assert_eq!(run(1, 100), 100);
        assert_eq!(run(8, 300), 300);
    }

    #[test]
    fn s1b_c5_a_job_submitted_after_shutdown_is_refused_and_does_not_run() {
        let ran = Arc::new(AtomicUsize::new(0));
        let mut pool = JobPool::new(2);
        pool.shutdown();
        let r = Arc::clone(&ran);
        assert!(!pool.submit(move || {
            r.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn s1b_c5_shutting_down_twice_and_dropping_are_fine() {
        let c = Arc::new(AtomicUsize::new(0));
        let mut pool = JobPool::new(2);
        for _ in 0..20 {
            let c = Arc::clone(&c);
            pool.submit(move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }
        pool.shutdown();
        pool.shutdown();
        drop(pool);
        assert_eq!(c.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn s1b_c5_dropping_the_pool_also_finishes_the_queue() {
        let c = Arc::new(AtomicUsize::new(0));
        {
            let pool = JobPool::new(3);
            for _ in 0..150 {
                let c = Arc::clone(&c);
                pool.submit(move || {
                    std::thread::yield_now();
                    c.fetch_add(1, Ordering::SeqCst);
                });
            }
        }
        assert_eq!(c.load(Ordering::SeqCst), 150);
    }
}
// @@ challenge 1b-c5 end
