//! Tests for module 1g, the page guards. A test name starts with its stage: `s1g_02_…` belongs to stage 1g-02, and
//! `anneal course test` runs just those.
//!
//! The tests use only the public methods of the pool and of the guards, and a disk of their own (`MemDisk`). Anything that could
//! hang (a latch that is never released, a flush that waits for a lock it holds) waits with a timeout, so a deadlock fails the test
//! with a message instead of freezing it.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::{PageData, BUSTUB_PAGE_SIZE};
use bustub::storage::page::page_guard::{ReadPageGuard, WritePageGuard};
use bustub::storage::disk::disk_manager::DiskIo;
#[path = "common/pool.rs"]
mod pool;
use pool::{pool_with, MemDisk, Policy};
use proptest::prelude::*;

mod common;

const PS: usize = BUSTUB_PAGE_SIZE;
const WAIT: Duration = Duration::from_secs(10);

fn pool(frames: usize) -> (Arc<BufferPoolManager>, Arc<MemDisk>) {
    let (bpm, disk) = pool_with(Policy::Fifo, frames);
    (Arc::new(bpm), disk)
}

fn text(guard_data: &PageData, len: usize) -> String {
    String::from_utf8_lossy(&guard_data[..len]).into_owned()
}

// ---- 1g-01 · Guards that release themselves -------------------------------------------------------------------------------


#[derive(Clone, Copy, Debug)]
enum GuardOp {
    Read(usize),
    Write(usize, u8),
    Drop(usize),
    Release(usize),
}

fn guard_ops() -> impl Strategy<Value = Vec<GuardOp>> {
    prop::collection::vec(
        prop_oneof![
            4 => (0..8usize).prop_map(GuardOp::Read),
            3 => (0..8usize, any::<u8>()).prop_map(|(i, b)| GuardOp::Write(i, b)),
            4 => (0..16usize).prop_map(GuardOp::Drop),
            1 => (0..16usize).prop_map(GuardOp::Release),
        ],
        1..150,
    )
}

enum Held<'a> {
    Reader(ReadPageGuard<'a>, usize),
    Writer(WritePageGuard<'a>, usize),
}

impl Held<'_> {
    fn page(&self) -> usize {
        match self {
            Held::Reader(_, page) | Held::Writer(_, page) => *page,
        }
    }
}

/// Guards taken and dropped in any order: the pool's pin count of a page is the number of live guards on it, a writer is alone on its
/// page, and what a writer wrote is what the next guard sees even after the page was evicted in between.
fn run_guards(policy: Policy, frames: usize, ops: &[GuardOp]) -> Result<(), TestCaseError> {
    let (bpm, _disk) = pool_with(policy, frames);
    let pages: Vec<_> = (0..8).map(|_| bpm.new_page()).collect();
    let mut fill = [0u8; 8];
    let mut held: Vec<Held> = Vec::new();
    for (step, op) in ops.iter().enumerate() {
        let pinned_pages = |held: &[Held]| (0..8).filter(|&i| held.iter().any(|h| h.page() == i)).count();
        match *op {
            GuardOp::Read(i) | GuardOp::Write(i, _) => {
                let on_page = held.iter().filter(|h| h.page() == i).count();
                let writer_there = held.iter().any(|h| matches!(h, Held::Writer(_, q) if *q == i));
                let write = matches!(op, GuardOp::Write(..));
                if writer_there || (write && on_page > 0) {
                    continue; // this thread would wait for itself
                }
                let should_succeed = on_page > 0 || pinned_pages(&held) < frames;
                if write {
                    let got = bpm.checked_write_page(pages[i]);
                    prop_assert_eq!(got.is_some(), should_succeed, "step {}: checked_write_page with {} of {} frames pinned", step, pinned_pages(&held), frames);
                    if let Some(mut guard) = got {
                        prop_assert!(guard.get_data().iter().all(|&b| b == fill[i]), "step {}: page {} should hold {} in every byte", step, i, fill[i]);
                        if let GuardOp::Write(_, byte) = *op {
                            guard.get_data_mut().fill(byte);
                            fill[i] = byte;
                        }
                        held.push(Held::Writer(guard, i));
                    }
                } else {
                    let got = bpm.checked_read_page(pages[i]);
                    prop_assert_eq!(got.is_some(), should_succeed, "step {}: checked_read_page with {} of {} frames pinned", step, pinned_pages(&held), frames);
                    if let Some(guard) = got {
                        prop_assert!(guard.get_data().iter().all(|&b| b == fill[i]), "step {}: page {} should hold {} in every byte", step, i, fill[i]);
                        held.push(Held::Reader(guard, i));
                    }
                }
            }
            GuardOp::Drop(i) => {
                if !held.is_empty() {
                    let at = i % held.len();
                    drop(held.remove(at));
                }
            }
            GuardOp::Release(i) => {
                if !held.is_empty() {
                    let at = i % held.len();
                    match &mut held[at] {
                        Held::Reader(g, _) => g.release(),
                        Held::Writer(g, _) => g.release(),
                    }
                    // a released guard no longer counts: drop it now so the model and the pool agree
                    drop(held.remove(at));
                }
            }
        }
        for i in 0..8 {
            let live = held.iter().filter(|h| h.page() == i).count();
            if live > 0 {
                prop_assert_eq!(bpm.get_pin_count(pages[i]), Some(live), "step {}: pin count of page {} must be the number of live guards on it", step, i);
            }
        }
    }
    held.clear();
    for i in 0..8 {
        prop_assert!(matches!(bpm.get_pin_count(pages[i]), None | Some(0)), "page {} still pinned after every guard was dropped", i);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn s1g_01_pin_counts_match_the_live_guards_for_guards_taken_and_dropped_in_any_order(ops in guard_ops()) {
        run_guards(Policy::Fifo, 3, &ops)?;
    }
}

#[test]
fn s1g_01_a_read_guard_pins_the_page_and_shows_its_bytes() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let guard = bpm.checked_read_page(page).expect("a free frame");
    assert_eq!(guard.get_page_id(), page, "a read guard pins the page and shows its bytes");
    assert_eq!(bpm.get_pin_count(page), Some(1), "a read guard pins the page and shows its bytes");
    assert_eq!(guard.get_data().len(), PS, "a read guard pins the page and shows its bytes");
    assert!(guard.get_data().iter().all(|&b| b == 0), "a read guard pins the page and shows its bytes: expected `guard.get_data().iter().all(|&b| b == 0)`");
}

#[test]
fn s1g_01_the_guard_derefs_to_the_page() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let frame = bpm.fetch_page(page).unwrap();
    bpm.frame_data(frame).write().unwrap()[..2].copy_from_slice(b"hi");
    bpm.unpin_page(page, true);
    let guard = bpm.checked_read_page(page).unwrap();
    assert_eq!(&guard[..2], b"hi", "the guard derefs to the page");
    assert_eq!(&guard.get_data()[..2], b"hi", "the guard derefs to the page");
}

#[test]
fn s1g_01_many_readers_share_a_page() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let a = bpm.checked_read_page(page).unwrap();
    let b = bpm.checked_read_page(page).unwrap();
    let c = bpm.checked_read_page(page).unwrap();
    assert_eq!(bpm.get_pin_count(page), Some(3), "many readers share a page");
    assert_eq!((a.get_page_id(), b.get_page_id(), c.get_page_id()), (page, page, page), "many readers share a page");
}

#[test]
fn s1g_01_no_guard_when_every_frame_is_pinned() {
    let (bpm, _) = pool(2);
    let (a, b, c) = (bpm.new_page(), bpm.new_page(), bpm.new_page());
    let _ga = bpm.checked_read_page(a).unwrap();
    let _gb = bpm.checked_read_page(b).unwrap();
    assert!(bpm.checked_read_page(c).is_none(), "no guard when every frame is pinned: expected `bpm.checked_read_page(c).is_none()`");
}


#[test]
fn s1g_01_dropping_a_read_guard_unpins_the_page() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let guard = bpm.checked_read_page(page).unwrap();
    assert_eq!(bpm.get_pin_count(page), Some(1), "dropping a read guard unpins the page");
    drop(guard);
    assert_eq!(bpm.get_pin_count(page), Some(0), "dropping a read guard unpins the page");
}

#[test]
fn s1g_01_leaving_a_scope_drops_the_guard() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    {
        let _guard = bpm.checked_read_page(page).expect("a frame");
    }
    assert_eq!(bpm.get_pin_count(page), Some(0), "leaving a scope drops the guard");
}

#[test]
fn s1g_01_release_is_idempotent() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let mut guard = bpm.checked_read_page(page).unwrap();
    guard.release();
    assert_eq!(bpm.get_pin_count(page), Some(0), "release is idempotent");
    guard.release(); // "Another drop should have no effect."
    assert_eq!(bpm.get_pin_count(page), Some(0), "release is idempotent");
    drop(guard); // and so does the destructor
    assert_eq!(bpm.get_pin_count(page), Some(0), "release is idempotent");
}

#[test]
fn s1g_01_a_released_guard_gives_up_the_latch() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let mut guard = bpm.checked_read_page(page).unwrap();
    let frame = bpm.fetch_page(page).unwrap(); // just to learn the frame; one more pin
    assert!(bpm.frame_data(frame).try_write().is_err(), "a reader holds the latch");
    guard.release();
    assert!(bpm.frame_data(frame).try_write().is_ok(), "released: the latch is free");
    bpm.unpin_page(page, false);
}

#[test]
fn s1g_01_an_unpinned_page_can_be_evicted() {
    let (bpm, _) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    let guard = bpm.checked_read_page(a).unwrap();
    assert!(bpm.checked_read_page(b).is_none(), "a is pinned");
    drop(guard);
    assert!(bpm.checked_read_page(b).is_some(), "a was unpinned, so b can take its frame");
}

#[test]
fn s1g_01_each_guard_releases_only_its_own_pin() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let a = bpm.checked_read_page(page).unwrap();
    let b = bpm.checked_read_page(page).unwrap();
    drop(a);
    assert_eq!(bpm.get_pin_count(page), Some(1), "each guard releases only its own pin");
    drop(b);
    assert_eq!(bpm.get_pin_count(page), Some(0), "each guard releases only its own pin");
}


#[test]
fn s1g_01_a_write_guard_changes_the_page_and_the_change_is_seen() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    {
        let mut guard = bpm.checked_write_page(page).unwrap();
        assert_eq!(bpm.get_pin_count(page), Some(1), "a write guard changes the page and the change is seen");
        guard.get_data_mut()[..5].copy_from_slice(b"hello");
        assert_eq!(&guard.get_data()[..5], b"hello", "a write guard changes the page and the change is seen");
    }
    assert_eq!(bpm.get_pin_count(page), Some(0), "a write guard changes the page and the change is seen");
    assert_eq!(text(bpm.checked_read_page(page).unwrap().get_data(), 5), "hello", "a write guard changes the page and the change is seen");
}

#[test]
fn s1g_01_get_data_mut_marks_the_page_dirty_and_a_look_does_not() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let mut guard = bpm.checked_write_page(page).unwrap();
    assert!(!guard.is_dirty(), "get data mut marks the page dirty and a look does not: expected `!guard.is_dirty()`");
    let _ = guard.get_data();
    assert!(!guard.is_dirty(), "get data mut marks the page dirty and a look does not: expected `!guard.is_dirty()`");
    guard.get_data_mut()[0] = 1;
    assert!(guard.is_dirty(), "get data mut marks the page dirty and a look does not: expected `guard.is_dirty()`");
}

#[test]
fn s1g_01_dirt_reaches_the_pool_so_the_page_survives_eviction() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    bpm.checked_write_page(a).unwrap().get_data_mut()[..4].copy_from_slice(b"data");
    bpm.checked_write_page(b).unwrap(); // evicts a: written because the guard reported the dirt
    assert_eq!(disk.writes().len(), 1, "dirt reaches the pool so the page survives eviction");
    assert_eq!(text(bpm.checked_read_page(a).unwrap().get_data(), 4), "data", "dirt reaches the pool so the page survives eviction");
}

#[test]
fn s1g_01_an_untouched_write_guard_leaves_the_page_clean() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    drop(bpm.checked_write_page(a).unwrap());
    drop(bpm.checked_write_page(b).unwrap());
    assert_eq!(disk.writes().len(), 0, "nobody called get_data_mut: nothing to write back");
}

#[test]
fn s1g_01_a_read_guard_leaves_the_page_clean() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    drop(bpm.checked_read_page(a).unwrap());
    let mut early = bpm.checked_read_page(a).unwrap();
    early.release();
    drop(early);
    drop(bpm.checked_read_page(b).unwrap()); // evicts a
    assert_eq!(disk.writes().len(), 0, "a read guard only looks: a page it released, by drop or by release, is clean and is not written back");
}

#[test]
fn s1g_01_a_writer_excludes_everyone_else_until_it_is_dropped() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let guard = bpm.checked_write_page(page).unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = {
        let bpm = Arc::clone(&bpm);
        thread::spawn(move || {
            let g = bpm.checked_read_page(page).unwrap();
            tx.send(g.get_data()[0]).unwrap();
        })
    };
    thread::sleep(Duration::from_millis(100));
    assert!(rx.try_recv().is_err(), "the reader got in while the writer held the page");
    drop(guard);
    rx.recv_timeout(WAIT).expect("the reader never got in: was the latch released?");
    reader.join().unwrap();
}

#[test]
fn s1g_01_release_is_idempotent_for_writers_too() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let mut guard = bpm.checked_write_page(page).unwrap();
    guard.release();
    guard.release();
    assert_eq!(bpm.get_pin_count(page), Some(0), "release is idempotent for writers too");
    assert!(bpm.checked_write_page(page).is_some(), "this will hang if the latch was not released");
}

#[test]
fn s1g_01_deref_mut_works_like_get_data_mut() {
    let (bpm, _) = pool(3);
    let page = bpm.new_page();
    let mut guard = bpm.checked_write_page(page).unwrap();
    guard[0] = 7;
    assert!(guard.is_dirty(), "deref mut works like get data mut: expected `guard.is_dirty()`");
    assert_eq!(guard[0], 7, "deref mut works like get data mut");
}


#[test]
fn s1g_02_they_return_a_guard_when_there_is_room() {
    let (bpm, _) = pool(2);
    let page = bpm.new_page();
    let _r = bpm.read_page(page);
    let _w = bpm.read_page(page);
    assert_eq!(bpm.get_pin_count(page), Some(2), "they return a guard when there is room");
}

#[test]
#[should_panic(expected = "every frame is pinned")]
fn s1g_02_read_page_panics_when_nothing_can_be_evicted() {
    let (bpm, _) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    let _ga = bpm.read_page(a);
    let _gb = bpm.read_page(b);
}

#[test]
#[should_panic(expected = "every frame is pinned")]
fn s1g_02_write_page_panics_when_nothing_can_be_evicted() {
    let (bpm, _) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    let _ga = bpm.write_page(a);
    let _gb = bpm.write_page(b);
}


#[test]
fn s1g_02_a_write_guard_can_flush_its_page() {
    let (bpm, disk) = pool(2);
    let page = bpm.new_page();
    let mut guard = bpm.write_page(page);
    guard.get_data_mut()[..5].copy_from_slice(b"flush");
    assert!(guard.is_dirty(), "a write guard can flush its page: expected `guard.is_dirty()`");
    guard.flush();
    assert!(!guard.is_dirty(), "flushed: clean");
    assert_eq!(disk.writes().len(), 1, "a write guard can flush its page");
    let mut buf = [0u8; PS];
    disk.read_page(page, &mut buf).unwrap();
    assert_eq!(&buf[..5], b"flush", "a write guard can flush its page");
    assert_eq!(bpm.get_pin_count(page), Some(1), "the guard keeps its pin");
}

#[test]
fn s1g_02_flushing_a_clean_guard_still_writes() {
    let (bpm, disk) = pool(2);
    let page = bpm.new_page();
    let mut guard = bpm.write_page(page);
    guard.flush();
    assert_eq!(disk.writes().len(), 1, "flushing a clean guard still writes");
}

#[test]
fn s1g_02_a_read_guard_can_flush_too() {
    let (bpm, disk) = pool(2);
    let page = bpm.new_page();
    bpm.write_page(page).get_data_mut()[0] = 9;
    let mut guard = bpm.read_page(page);
    guard.flush();
    let mut buf = [0u8; PS];
    disk.read_page(page, &mut buf).unwrap();
    assert_eq!(buf[0], 9, "a read guard can flush too");
}

#[test]
fn s1g_02_flushing_while_holding_the_latch_does_not_deadlock() {
    let (bpm, _) = pool(2);
    let page = bpm.new_page();
    let (tx, rx) = mpsc::channel();
    let t = {
        let bpm = Arc::clone(&bpm);
        thread::spawn(move || {
            let mut guard = bpm.write_page(page);
            guard.get_data_mut()[0] = 1;
            guard.flush();
            tx.send(()).unwrap();
        })
    };
    rx.recv_timeout(WAIT).expect("flush() hung: it must not ask for the latch the guard already holds");
    t.join().unwrap();
}


#[test]
fn s1g_02_a_blocked_flush_does_not_block_the_rest_of_the_pool() {
    // Thread W holds the write latch on page P. Thread F calls flush_page(P): it has to wait for W. Meanwhile W asks the pool for
    // another page. If F waits while holding the pool's lock, W waits for F and F waits for W: deadlock.
    let (bpm, _) = pool(4);
    let (p, q) = (bpm.new_page(), bpm.new_page());
    let (done_tx, done_rx) = mpsc::channel();
    let main_bpm = Arc::clone(&bpm);
    let worker = thread::spawn(move || {
        let bpm = main_bpm;
        let mut guard_p = bpm.write_page(p);
        guard_p.get_data_mut()[0] = 5;
        let flusher = {
            let bpm = Arc::clone(&bpm);
            thread::spawn(move || bpm.flush_page(p))
        };
        thread::sleep(Duration::from_millis(200)); // F is now waiting for P's latch
        let guard_q = bpm.write_page(q); // needs the pool's lock
        drop(guard_q);
        drop(guard_p); // F can go on
        let flushed = flusher.join().unwrap();
        done_tx.send(flushed).unwrap();
    });
    let flushed = done_rx.recv_timeout(WAIT).expect("deadlock: flush_page waits for a latch while holding the pool's lock");
    assert!(flushed, "a blocked flush does not block the rest of the pool: expected `flushed`");
    worker.join().unwrap();
}

#[test]
fn s1g_02_flush_page_still_writes_the_latest_bytes() {
    let (bpm, disk) = pool(2);
    let page = bpm.new_page();
    {
        let mut guard = bpm.write_page(page);
        guard.get_data_mut()[..3].copy_from_slice(b"abc");
    }
    assert!(bpm.flush_page(page), "flush page still writes the latest bytes: expected `bpm.flush_page(page)`");
    let mut buf = [0u8; PS];
    disk.read_page(page, &mut buf).unwrap();
    assert_eq!(&buf[..3], b"abc", "flush page still writes the latest bytes");
    assert_eq!(bpm.get_pin_count(page), Some(0), "flush_page leaves the pin count where it found it");
}

#[test]
fn s1g_02_a_flushed_page_is_not_left_pinned_so_it_can_be_evicted() {
    let (bpm, _) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    bpm.write_page(a).get_data_mut()[0] = 1;
    assert!(bpm.flush_page(a), "a flushed page is not left pinned so it can be evicted: expected `bpm.flush_page(a)`");
    assert!(bpm.checked_read_page(b).is_some(), "a is unpinned again: its frame can be reused");
}


#[test]
fn s1g_03_guards_moved_into_a_vec_keep_their_pages_pinned_until_dropped() {
    let (bpm, _) = pool(4);
    let pages: Vec<_> = (0..4).map(|_| bpm.new_page()).collect();
    let mut guards: Vec<_> = pages.iter().map(|&p| bpm.write_page(p)).collect();
    assert!(pages.iter().all(|&p| bpm.get_pin_count(p) == Some(1)), "guards moved into a vec keep their pages pinned until dropped: expected `pages.iter().all(|&p| bpm.get_pin_count(p) == Some(1))`");
    assert!(bpm.checked_read_page(bpm.new_page()).is_none(), "guards moved into a vec keep their pages pinned until dropped: expected `bpm.checked_read_page(bpm.new_page()).is_none()`");
    let first = guards.remove(0); // moved out of the Vec
    assert_eq!(bpm.get_pin_count(pages[0]), Some(1), "moving a guard doesn't release it");
    drop(first);
    assert_eq!(bpm.get_pin_count(pages[0]), Some(0), "guards moved into a vec keep their pages pinned until dropped");
    guards.clear();
    assert!(pages.iter().all(|&p| bpm.get_pin_count(p) == Some(0)), "guards moved into a vec keep their pages pinned until dropped: expected `pages.iter().all(|&p| bpm.get_pin_count(p) == Some(0))`");
}

#[test]
fn s1g_03_assigning_a_guard_drops_the_one_it_replaces() {
    let (bpm, _) = pool(4);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    let mut guard = bpm.read_page(a);
    let other = bpm.read_page(b);
    assert_eq!((bpm.get_pin_count(a), bpm.get_pin_count(b)), (Some(1), Some(1)), "assigning a guard drops the one it replaces");
    guard = other; // the old guard (page a) is dropped here
    assert_eq!(bpm.get_pin_count(a), Some(0), "assigning a guard drops the one it replaces");
    assert_eq!(bpm.get_pin_count(b), Some(1), "assigning a guard drops the one it replaces");
    assert_eq!(guard.get_page_id(), b, "assigning a guard drops the one it replaces");
}

#[test]
fn s1g_03_many_threads_incrementing_one_page_lose_nothing() {
    let (bpm, _) = pool(4);
    let page = bpm.new_page();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let bpm = Arc::clone(&bpm);
            thread::spawn(move || {
                for _ in 0..500 {
                    let mut guard = bpm.write_page(page);
                    let n = u32::from_le_bytes(guard.get_data()[..4].try_into().unwrap()) + 1;
                    guard.get_data_mut()[..4].copy_from_slice(&n.to_le_bytes());
                }
            })
        })
        .collect();
    handles.into_iter().for_each(|h| h.join().unwrap());
    let n = u32::from_le_bytes(bpm.read_page(page).get_data()[..4].try_into().unwrap());
    assert_eq!(n, 2000, "many threads incrementing one page lose nothing");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    /// The guard model once more, on a pool that uses the replacers you built.
    #[test]
    fn s1g_03_guards_behave_the_same_under_arc_and_lru_k(ops in guard_ops()) {
        run_guards(Policy::Arc, 3, &ops)?;
        run_guards(Policy::LruK(2), 3, &ops)?;
    }
}

// @@ challenge 1g-c1 begin
mod ch_1g_c1 {
    use proptest::prelude::*;

    use bustub::common::defer::Defer;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn log() -> Rc<RefCell<Vec<u32>>> {
        Rc::new(RefCell::new(Vec::new()))
    }

    #[test]
    fn s1g_c1_runs_when_the_scope_ends() {
        let l = log();
        {
            let l2 = l.clone();
            let _g = Defer::new(move || l2.borrow_mut().push(1));
            l.borrow_mut().push(0);
        }
        assert_eq!(*l.borrow(), vec![0, 1]);
    }

    #[test]
    fn s1g_c1_a_cancelled_guard_never_runs() {
        let l = log();
        {
            let l2 = l.clone();
            let mut g = Defer::new(move || l2.borrow_mut().push(1));
            g.cancel();
        }
        assert!(l.borrow().is_empty());
    }

    #[test]
    fn s1g_c1_run_now_runs_once_and_drop_does_not_run_it_again() {
        let l = log();
        {
            let l2 = l.clone();
            let mut g = Defer::new(move || l2.borrow_mut().push(1));
            g.run_now();
            g.run_now();
            assert_eq!(*l.borrow(), vec![1]);
        }
        assert_eq!(*l.borrow(), vec![1]);
    }

    #[test]
    fn s1g_c1_guards_run_in_reverse_order_of_creation() {
        let l = log();
        {
            let (a, b, c) = (l.clone(), l.clone(), l.clone());
            let _x = Defer::new(move || a.borrow_mut().push(1));
            let _y = Defer::new(move || b.borrow_mut().push(2));
            let _z = Defer::new(move || c.borrow_mut().push(3));
        }
        assert_eq!(*l.borrow(), vec![3, 2, 1]);
    }

    #[test]
    fn s1g_c1_an_early_return_and_a_question_mark_still_run_the_guard() {
        fn work(l: Rc<RefCell<Vec<u32>>>, fail: bool) -> Result<u32, ()> {
            let l2 = l.clone();
            let _g = Defer::new(move || l2.borrow_mut().push(99));
            if fail {
                Err(())?;
            }
            Ok(1)
        }
        let l = log();
        assert_eq!(work(l.clone(), true), Err(()));
        assert_eq!(work(l.clone(), false), Ok(1));
        assert_eq!(*l.borrow(), vec![99, 99]);
    }

    #[test]
    fn s1g_c1_a_panic_runs_the_guard_while_unwinding() {
        use std::sync::{Arc, Mutex};
        let l = Arc::new(Mutex::new(Vec::new()));
        let l2 = l.clone();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _g = Defer::new(move || l2.lock().unwrap().push(1));
            panic!("boom");
        }));
        assert!(r.is_err());
        assert_eq!(*l.lock().unwrap(), vec![1]);
    }

    #[test]
    fn s1g_c1_a_moved_guard_runs_once_when_its_new_owner_drops() {
        let l = log();
        let l2 = l.clone();
        let g = Defer::new(move || l2.borrow_mut().push(7));
        let holder = vec![g];
        assert!(l.borrow().is_empty(), "moving a guard into a vector does not run it");
        drop(holder);
        assert_eq!(*l.borrow(), vec![7]);
    }
}
// @@ challenge 1g-c1 end

// @@ challenge 1g-c2 begin
mod ch_1g_c2 {
    use proptest::prelude::*;

    use bustub::common::pin_handle::Pins;

    #[test]
    fn s1g_c2_pin_clone_and_drop_keep_the_count_exact() {
        let pins = Pins::new();
        let a = pins.pin();
        let b = pins.pin();
        assert_eq!(pins.count(), 2);
        let c = a.clone();
        assert_eq!(pins.count(), 3);
        drop(b);
        drop(a);
        assert_eq!(pins.count(), 1);
        drop(c);
        assert_eq!(pins.count(), 0);
    }

    #[test]
    fn s1g_c2_a_scope_leaves_the_count_as_it_found_it() {
        let pins = Pins::new();
        let keep = pins.pin();
        {
            let h = pins.pin();
            let _c = h.clone();
            assert_eq!(pins.count(), 3);
        }
        assert_eq!(pins.count(), 1);
        drop(keep);
    }

    #[test]
    fn s1g_c2_handles_on_other_threads_count_and_uncount() {
        let pins = Pins::new();
        let hs: Vec<_> = (0..8).map(|_| pins.pin()).collect();
        assert_eq!(pins.count(), 8);
        let threads: Vec<_> = hs.into_iter().map(|h| std::thread::spawn(move || drop(h))).collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(pins.count(), 0);
    }

    #[test]
    fn s1g_c2_a_moved_handle_is_counted_once() {
        let pins = Pins::new();
        let h = pins.pin();
        let moved = h;
        assert_eq!(pins.count(), 1);
        drop(moved);
        assert_eq!(pins.count(), 0);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: after any sequence of pins, clones and drops the count is the number of live handles.
        #[test]
        fn s1g_c2_property_the_count_is_the_number_of_live_handles(ops in proptest::collection::vec((0u8..3, 0usize..8), 0..80)) {
            let pins = Pins::new();
            let mut live = Vec::new();
            for (op, i) in ops {
                match op {
                    0 => live.push(pins.pin()),
                    1 if !live.is_empty() => { let h = live[i % live.len()].clone(); live.push(h); }
                    _ if !live.is_empty() => { live.swap_remove(i % live.len()); }
                    _ => {}
                }
                prop_assert_eq!(pins.count(), live.len());
            }
        }
    }
}
// @@ challenge 1g-c2 end

// @@ challenge 1g-c3 begin
mod ch_1g_c3 {
    use proptest::prelude::*;

    use bustub::common::scoped_pin::ScopedPin;
    use std::cell::Cell;

    #[test]
    fn s1g_c3_dropping_uncounts() {
        let c = Cell::new(0);
        {
            let _a = ScopedPin::new(&c);
            let _b = ScopedPin::new(&c);
            assert_eq!(c.get(), 2);
        }
        assert_eq!(c.get(), 0);
    }

    #[test]
    fn s1g_c3_releasing_uncounts_once_and_returns_the_count_after() {
        let c = Cell::new(0);
        let a = ScopedPin::new(&c);
        let _b = ScopedPin::new(&c);
        assert_eq!(a.release(), 1);
        assert_eq!(c.get(), 1, "the released guard must not uncount again when it goes out of scope");
    }

    #[test]
    fn s1g_c3_a_mix_of_releases_and_drops() {
        let c = Cell::new(0);
        let guards: Vec<_> = (0..5).map(|_| ScopedPin::new(&c)).collect();
        let mut left = 5;
        for (i, g) in guards.into_iter().enumerate() {
            if i % 2 == 0 {
                left -= 1;
                assert_eq!(g.release(), left);
            } else {
                drop(g);
                left -= 1;
            }
            assert_eq!(c.get(), left);
        }
        assert_eq!(c.get(), 0);
    }

    #[test]
    fn s1g_c3_releasing_the_last_guard_leaves_zero_and_the_value_returned_is_zero() {
        let c = Cell::new(0);
        let g = ScopedPin::new(&c);
        assert_eq!(g.release(), 0);
        assert_eq!(c.get(), 0, "it must stay at zero: a second uncount would underflow");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the counter is always the number of guards that are neither dropped nor released.
        #[test]
        fn s1g_c3_property_the_counter_equals_the_live_guards(ops in proptest::collection::vec((0u8..3, 0usize..6), 0..60)) {
            let c = Cell::new(0);
            let mut live = Vec::new();
            for (op, i) in ops {
                match op {
                    0 => live.push(ScopedPin::new(&c)),
                    1 if !live.is_empty() => { let g = live.swap_remove(i % live.len()); let n = g.release(); prop_assert_eq!(n, live.len()); }
                    _ if !live.is_empty() => { live.swap_remove(i % live.len()); }
                    _ => {}
                }
                prop_assert_eq!(c.get(), live.len());
            }
        }
    }
}
// @@ challenge 1g-c3 end

// @@ challenge 1g-c4 begin
mod ch_1g_c4 {
    use proptest::prelude::*;

    use bustub::common::timed_latch::TimedLatch;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn s1g_c4_a_free_latch_is_taken_at_once() {
        let l = TimedLatch::new();
        assert!(l.try_acquire());
        assert!(!l.try_acquire());
        assert!(l.release());
        assert!(l.acquire_timeout(Duration::from_millis(0)));
    }

    #[test]
    fn s1g_c4_a_held_latch_times_out_after_about_the_timeout() {
        let l = TimedLatch::new();
        l.try_acquire();
        let t = Instant::now();
        assert!(!l.acquire_timeout(Duration::from_millis(60)));
        let waited = t.elapsed();
        assert!(waited >= Duration::from_millis(55), "returned after {waited:?}, before the timeout");
        assert!(waited < Duration::from_secs(5));
        assert!(l.release(), "a timed-out wait leaves the latch held by its owner");
    }

    #[test]
    fn s1g_c4_a_release_within_the_timeout_hands_the_latch_to_the_waiter() {
        let l = Arc::new(TimedLatch::new());
        l.try_acquire();
        let l2 = l.clone();
        let h = std::thread::spawn(move || l2.acquire_timeout(Duration::from_secs(10)));
        std::thread::sleep(Duration::from_millis(40));
        assert!(l.release());
        assert!(h.join().unwrap(), "the waiter must be woken by the release");
        assert!(!l.try_acquire(), "and now the waiter holds it");
    }

    #[test]
    fn s1g_c4_releasing_a_latch_that_is_not_held_says_so() {
        let l = TimedLatch::new();
        assert!(!l.release());
        assert!(l.try_acquire());
    }

    #[test]
    fn s1g_c4_each_release_lets_exactly_one_of_many_waiters_in() {
        let l = Arc::new(TimedLatch::new());
        l.try_acquire();
        let got = Arc::new(AtomicUsize::new(0));
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let (l, got) = (l.clone(), got.clone());
                std::thread::spawn(move || {
                    if l.acquire_timeout(Duration::from_millis(600)) {
                        got.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        std::thread::sleep(Duration::from_millis(80));
        l.release();
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(got.load(Ordering::SeqCst), 1, "one release, one grant; the others timed out");
    }
}
// @@ challenge 1g-c4 end

// @@ challenge 1g-c5 begin
mod ch_1g_c5 {
    use proptest::prelude::*;

    use bustub::common::slot_pair::{Slots, TransferError};
    use std::sync::Arc;
    use std::time::Duration;

    /// Runs `f` on its own thread and fails the test if it has not returned in `secs` seconds (a deadlock would otherwise hang the run).
    fn finish_within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(secs)).expect("did not finish: a deadlock?")
    }

    #[test]
    fn s1g_c5_a_transfer_moves_the_amount_and_failures_change_nothing() {
        let s = Slots::new(&[10, 5]);
        assert_eq!(s.transfer(0, 1, 4), Ok(()));
        assert_eq!((s.balance(0), s.balance(1)), (Some(6), Some(9)));
        assert_eq!(s.transfer(1, 0, 20), Err(TransferError::Insufficient));
        assert_eq!(s.transfer(0, 7, 1), Err(TransferError::NoSuchSlot(7)));
        assert_eq!(s.transfer(9, 1, 1), Err(TransferError::NoSuchSlot(9)));
        assert_eq!((s.balance(0), s.balance(1), s.total()), (Some(6), Some(9), 15));
        assert_eq!(s.balance(2), None);
    }

    #[test]
    fn s1g_c5_a_transfer_to_the_same_slot_succeeds_and_does_not_lock_twice() {
        let s = Arc::new(Slots::new(&[3, 3]));
        let s2 = s.clone();
        let r = finish_within(5, move || s2.transfer(1, 1, 2));
        assert_eq!(r, Ok(()));
        assert_eq!(s.balance(1), Some(3));
    }

    #[test]
    fn s1g_c5_opposite_directions_between_the_same_two_slots_both_finish() {
        let s = Arc::new(Slots::new(&[100, 100]));
        let (a, b) = (s.clone(), s.clone());
        finish_within(20, move || {
            let t1 = std::thread::spawn(move || {
                for _ in 0..5000 {
                    let _ = a.transfer(0, 1, 1);
                }
            });
            let t2 = std::thread::spawn(move || {
                for _ in 0..5000 {
                    let _ = b.transfer(1, 0, 1);
                }
            });
            t1.join().unwrap();
            t2.join().unwrap();
        });
        assert_eq!(s.total(), 200);
    }

    #[test]
    fn s1g_c5_many_threads_on_random_pairs_conserve_the_total() {
        let s = Arc::new(Slots::new(&[50; 6]));
        let s2 = s.clone();
        finish_within(30, move || {
            let hs: Vec<_> = (0..6u64)
                .map(|t| {
                    let s = s2.clone();
                    std::thread::spawn(move || {
                        let mut x = t * 7919 + 1;
                        for _ in 0..3000 {
                            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                            let (f, to) = ((x >> 33) as usize % 6, (x >> 17) as usize % 6);
                            let _ = s.transfer(f, to, ((x >> 8) % 20) as i64);
                        }
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
        });
        assert_eq!(s.total(), 300);
        assert!((0..6).all(|i| s.balance(i).unwrap() >= 0));
    }

    #[test]
    fn s1g_c5_total_is_a_consistent_snapshot_while_transfers_run() {
        let s = Arc::new(Slots::new(&[100, 100, 100]));
        let mover = {
            let s = s.clone();
            std::thread::spawn(move || {
                for i in 0..4000usize {
                    let _ = s.transfer(i % 3, (i + 1) % 3, 1);
                }
            })
        };
        let s2 = s.clone();
        finish_within(20, move || {
            for _ in 0..500 {
                assert_eq!(s2.total(), 300, "a total taken in the middle of a transfer saw the amount in neither slot");
            }
        });
        mover.join().unwrap();
    }
}
// @@ challenge 1g-c5 end
