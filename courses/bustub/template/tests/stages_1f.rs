//! Tests for module 1f, the buffer pool. A test name starts with its stage: `s1f_02_…` belongs to stage 1f-02, and
//! `anneal course test` runs just those.
//!
//! The tests use only the pool's public methods and a disk of their own (`MemDisk`, which counts what the pool asks of it). Most
//! tests run on a pool with a plain replacer written in this file (`FifoReplacer`), so they test the pool and not your ARC
//! replacer; a few then run the same checks with the replacers you built. The main property is a **model**: a random sequence of
//! new, fetch, write, unpin, flush and delete operations is run on the pool and on a table of "what each page should contain and
//! how often it is pinned". Whatever was evicted in between, a page must read back what was last written to it.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::{FrameId, PageId, BUSTUB_PAGE_SIZE};
use bustub::storage::disk::disk_manager::DiskIo;
use proptest::prelude::*;

mod common;

const PS: usize = BUSTUB_PAGE_SIZE;

fn p(n: i32) -> PageId {
    PageId(n)
}

fn config() -> ProptestConfig {
    ProptestConfig { cases: 48, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

use bustub::buffer::replacer::FrameReplacer;
#[path = "common/pool.rs"]
mod pool;
use pool::{pool, pool_with, pool_with_spy, MemDisk, Policy};

/// Fetches a page, writes `text` at the start of its frame, and unpins it dirty (or clean).
fn write_text(bpm: &BufferPoolManager, page: PageId, text: &str, dirty: bool) {
    let frame = bpm.fetch_page(page).expect("a frame for the page");
    bpm.frame_data(frame).write().unwrap()[..text.len()].copy_from_slice(text.as_bytes());
    assert!(bpm.unpin_page(page, dirty), "unpinning a page this helper pinned");
}

/// The first `len` bytes of a page, fetched and unpinned.
fn read_text(bpm: &BufferPoolManager, page: PageId, len: usize) -> String {
    let frame = bpm.fetch_page(page).expect("a frame for the page");
    let text = String::from_utf8_lossy(&bpm.frame_data(frame).read().unwrap()[..len]).into_owned();
    assert!(bpm.unpin_page(page, false), "unpinning a page this helper pinned");
    text
}

// ---- The model ------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Op {
    New,
    Fetch(usize),
    Write(usize, u8),
    Unpin(usize, bool),
    Flush(usize),
    Delete(usize),
}

fn ops(with_flush_delete: bool) -> impl Strategy<Value = Vec<Op>> {
    let extra = if with_flush_delete { 2 } else { 0 };
    prop::collection::vec(
        prop_oneof![
            2 => Just(Op::New),
            6 => (0..24usize).prop_map(Op::Fetch),
            4 => (0..24usize, any::<u8>()).prop_map(|(i, b)| Op::Write(i, b)),
            6 => (0..24usize, any::<bool>()).prop_map(|(i, b)| Op::Unpin(i, b)),
            extra => (0..24usize).prop_map(Op::Flush),
            extra => (0..24usize).prop_map(Op::Delete),
        ],
        1..200,
    )
}

struct PageModel {
    id: PageId,
    /// Every byte of the page is this, as last written.
    fill: u8,
    pins: usize,
    /// The frame it was pinned in (while pinned).
    frame: Option<FrameId>,
    /// Written since the last unpin that reported it dirty.
    unreported: bool,
    may_be_written: bool,
    deleted: bool,
}

fn pick_mut(pages: &mut [PageModel], i: usize) -> Option<&mut PageModel> {
    let n = pages.len();
    if n == 0 {
        None
    } else {
        pages.get_mut(i % n)
    }
}

/// Runs `ops` on `bpm` and on the model. With `max_pages`, `New` stops creating pages at that many (so no eviction is ever needed).
fn run_model(bpm: &BufferPoolManager, disk: &MemDisk, frames: usize, ops: &[Op], max_pages: usize) -> Result<(), TestCaseError> {
    let mut pages: Vec<PageModel> = Vec::new();
    for (step, op) in ops.iter().enumerate() {
        let pinned = pages.iter().filter(|m| m.pins > 0).count();
        match *op {
            Op::New => {
                if pages.len() >= max_pages {
                    continue;
                }
                let id = bpm.new_page();
                prop_assert!(pages.iter().all(|m| m.id != id), "step {}: new_page returned {:?} twice", step, id);
                pages.push(PageModel { id, fill: 0, pins: 0, frame: None, unreported: false, may_be_written: false, deleted: false });
            }
            Op::Fetch(i) => {
                let Some(m) = pick_mut(&mut pages, i).filter(|m| !m.deleted) else { continue };
                let got = bpm.fetch_page(m.id);
                // a pinned page is in memory; any other page can be brought in exactly when some frame is not pinned
                let should_succeed = m.pins > 0 || pinned < frames;
                prop_assert_eq!(
                    got.is_some(),
                    should_succeed,
                    "step {}: fetch_page({:?}) with {} of {} frames pinned and this page pinned {} times",
                    step,
                    m.id,
                    pinned,
                    frames,
                    m.pins
                );
                if let Some(frame) = got {
                    if m.pins > 0 {
                        prop_assert_eq!(Some(frame), m.frame, "step {}: a page that is in memory keeps its frame", step);
                    } else {
                        let data = bpm.frame_data(frame).read().unwrap();
                        prop_assert!(data.iter().all(|&b| b == m.fill), "step {}: page {:?} should hold {} in every byte but starts with {} (it was evicted and brought back, or never written)", step, m.id, m.fill, data[0]);
                    }
                    m.pins += 1;
                    m.frame = Some(frame);
                }
            }
            Op::Write(i, byte) => {
                let Some(m) = pick_mut(&mut pages, i).filter(|m| m.pins > 0) else { continue };
                bpm.frame_data(m.frame.unwrap()).write().unwrap().fill(byte);
                m.fill = byte;
                m.unreported = true;
                m.may_be_written = true;
            }
            Op::Unpin(i, flag) => {
                let Some(m) = pick_mut(&mut pages, i).filter(|m| !m.deleted) else { continue };
                if m.pins == 0 {
                    // nothing to unpin: the pool says so (and a page that is not in memory has no pin either)
                    prop_assert!(
                        !bpm.unpin_page(m.id, false),
                        "step {}: unpin_page of {:?}, which is not pinned, must return false",
                        step,
                        m.id
                    );
                    continue;
                }
                // report the writes at the last unpin at the latest; an earlier unpin may report them or not
                let report = if m.pins == 1 { m.unreported } else { m.unreported && flag };
                prop_assert!(
                    bpm.unpin_page(m.id, report),
                    "step {}: unpin_page of {:?}, pinned {} times, must return true",
                    step,
                    m.id,
                    m.pins
                );
                if report {
                    m.unreported = false;
                }
                m.pins -= 1;
                if m.pins == 0 {
                    m.frame = None;
                }
            }
            Op::Flush(i) => {
                let Some(m) = pick_mut(&mut pages, i).filter(|m| !m.deleted) else { continue };
                m.may_be_written = true; // flush writes the page whether or not it is dirty
                let ok = bpm.flush_page(m.id);
                if m.pins > 0 {
                    prop_assert!(ok, "step {}: flush_page of the pinned page {:?} must succeed", step, m.id);
                    prop_assert!(
                        disk.stored(m.id.0).iter().all(|&b| b == m.fill),
                        "step {}: flush_page must write the page's current bytes ({} in every byte)",
                        step,
                        m.fill
                    );
                }
            }
            Op::Delete(i) => {
                let Some(m) = pick_mut(&mut pages, i).filter(|m| !m.deleted) else { continue };
                let ok = bpm.delete_page(m.id);
                prop_assert_eq!(
                    ok,
                    m.pins == 0,
                    "step {}: delete_page({:?}) succeeds exactly when nobody has the page pinned (pins: {})",
                    step,
                    m.id,
                    m.pins
                );
                if ok {
                    m.deleted = true;
                }
            }
        }
        for m in pages.iter().filter(|m| m.pins > 0) {
            prop_assert_eq!(bpm.get_pin_count(m.id), Some(m.pins), "step {}: pin count of {:?}", step, m.id);
        }
    }
    // a page that was never modified and never flushed is never written to disk
    let modified: BTreeSet<i32> = pages.iter().filter(|m| m.may_be_written).map(|m| m.id.0).collect();
    let written = disk.written_pages();
    prop_assert!(
        written.is_subset(&modified),
        "pages {:?} were written to disk but never modified or flushed",
        written.difference(&modified).collect::<Vec<_>>()
    );
    Ok(())
}

// ---- 1f-01 · Pages in memory ---------------------------------------------------------------------------------------------

#[test]
fn s1f_01_the_pool_reports_its_size() {
    let (bpm, _) = pool(7);
    assert_eq!(bpm.size(), 7, "the pool reports its size");
}

#[test]
fn s1f_01_new_page_ids_are_distinct_and_cost_no_frame_and_no_io() {
    let (bpm, disk) = pool(2);
    let ids: BTreeSet<i32> = (0..50).map(|_| bpm.new_page().0).collect();
    assert_eq!(ids.len(), 50, "fifty calls give fifty different ids");
    assert_eq!((disk.reads(), disk.writes().len()), (0, 0), "new_page does not touch the disk");
    // and no frame was used up: two pages can still be pinned
    let (a, b) = (bpm.new_page(), bpm.new_page());
    assert!(bpm.fetch_page(a).is_some() && bpm.fetch_page(b).is_some(), "new_page must not occupy a frame");
}

#[test]
fn s1f_01_threads_never_get_the_same_page_id() {
    let (bpm, _) = pool(2);
    let bpm = Arc::new(bpm);
    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn({
                let bpm = Arc::clone(&bpm);
                move || (0..200).map(|_| bpm.new_page().0).collect::<Vec<_>>()
            })
        })
        .collect();
    let all: Vec<i32> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
    assert_eq!(all.iter().collect::<BTreeSet<_>>().len(), all.len(), "1600 ids from 8 threads must all be different");
}

#[test]
fn s1f_01_a_page_nobody_wrote_arrives_zeroed_and_a_page_on_disk_arrives_as_stored() {
    let (bpm, disk) = pool(2);
    let fresh = bpm.new_page();
    let frame = bpm.fetch_page(fresh).unwrap();
    assert!(bpm.frame_data(frame).read().unwrap().iter().all(|&b| b == 0), "a new page reads as zeros");
    bpm.unpin_page(fresh, false);
    disk.write_page(p(40), &[7; PS]).unwrap();
    let frame = bpm.fetch_page(p(40)).unwrap();
    assert!(bpm.frame_data(frame).read().unwrap().iter().all(|&b| b == 7), "a page that is on disk is read from disk");
}

#[test]
fn s1f_01_fetching_pins_and_unpinning_releases() {
    let (bpm, _) = pool(3);
    let a = bpm.new_page();
    assert_eq!(bpm.get_pin_count(a), None, "a page that was not fetched is not in memory");
    for expected in 1..=3 {
        bpm.fetch_page(a).unwrap();
        assert_eq!(bpm.get_pin_count(a), Some(expected), "fetching pins and unpinning releases");
    }
    assert!(bpm.unpin_page(a, false), "fetching pins and unpinning releases: expected `bpm.unpin_page(a, false)`");
    assert_eq!(bpm.get_pin_count(a), Some(2), "fetching pins and unpinning releases");
    assert!(
        bpm.unpin_page(a, false) && bpm.unpin_page(a, false),
        "fetching pins and unpinning releases: expected `bpm.unpin_page(a, false) && bpm.unpin_page(a, false)`"
    );
    assert_eq!(bpm.get_pin_count(a), Some(0), "fetching pins and unpinning releases");
    assert!(!bpm.unpin_page(a, false), "a page that is not pinned cannot be unpinned");
    assert!(!bpm.unpin_page(p(99), false), "neither can a page that is not in memory");
}

#[test]
fn s1f_01_a_page_in_memory_keeps_its_frame_and_its_bytes_and_other_pages_get_other_frames() {
    let (bpm, disk) = pool(3);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    let fa = bpm.fetch_page(a).unwrap();
    bpm.frame_data(fa).write().unwrap()[..5].copy_from_slice(b"hello");
    let fa2 = bpm.fetch_page(a).unwrap();
    let fb = bpm.fetch_page(b).unwrap();
    assert_eq!(fa, fa2, "fetching a page that is in memory returns its frame");
    assert_ne!(fa, fb, "two pages in memory are in different frames");
    assert_eq!(&bpm.frame_data(fa2).read().unwrap()[..5], b"hello", "and the bytes are still there");
    assert_eq!(disk.reads(), 2, "a hit does not read the disk again");
}

#[test]
fn s1f_01_with_every_frame_pinned_no_other_page_can_be_fetched() {
    let (bpm, _) = pool(2);
    let (a, b, c) = (bpm.new_page(), bpm.new_page(), bpm.new_page());
    bpm.fetch_page(a).unwrap();
    bpm.fetch_page(b).unwrap();
    assert_eq!(bpm.fetch_page(c), None, "both frames are pinned: no room for a third page");
    assert!(bpm.fetch_page(a).is_some(), "but a page that is already in memory can be pinned again");
}

proptest! {
    #![proptest_config(config())]

    /// With no more pages than frames nothing is ever evicted: the pool is a table of pinned buffers.
    #[test]
    fn s1f_01_a_pool_with_room_for_every_page_behaves_like_the_model(ops in ops(false)) {
        let (bpm, disk) = pool(6);
        run_model(&bpm, &disk, 6, &ops, 6)?;
    }
}

// ---- 1f-02 · Eviction that loses nothing -----------------------------------------------------------------------------------

#[test]
fn s1f_02_an_unpinned_page_makes_room_for_another() {
    let (bpm, _) = pool(2);
    let (a, b, c) = (bpm.new_page(), bpm.new_page(), bpm.new_page());
    bpm.fetch_page(a).unwrap();
    bpm.fetch_page(b).unwrap();
    assert_eq!(bpm.fetch_page(c), None, "an unpinned page makes room for another");
    bpm.unpin_page(a, false);
    assert!(bpm.fetch_page(c).is_some(), "page a is not pinned, so its frame can be reused");
    assert_eq!(bpm.get_pin_count(a), None, "and page a is no longer in memory");
}

#[test]
fn s1f_02_pinned_pages_are_never_the_victim() {
    let (bpm, _) = pool(3);
    let pages: Vec<PageId> = (0..10).map(|_| bpm.new_page()).collect();
    bpm.fetch_page(pages[0]).unwrap(); // stays pinned throughout
    write_text(&bpm, pages[0], "pinned", true);
    bpm.fetch_page(pages[0]).unwrap();
    for &page in &pages[1..] {
        bpm.fetch_page(page).unwrap();
        bpm.unpin_page(page, false);
    }
    assert!(bpm.get_pin_count(pages[0]).is_some_and(|n| n >= 1), "the pinned page is still in memory");
}

#[test]
fn s1f_02_a_dirty_page_survives_eviction() {
    let (bpm, _) = pool(2);
    let pages: Vec<PageId> = (0..6).map(|_| bpm.new_page()).collect();
    write_text(&bpm, pages[0], "remember me", true);
    for &page in &pages[1..] {
        write_text(&bpm, page, "other", true); // forces page 0 out
    }
    assert_eq!(read_text(&bpm, pages[0], 11), "remember me", "an evicted dirty page is written back and read again");
}

#[test]
fn s1f_02_a_clean_page_costs_no_write_and_its_unreported_changes_are_not_promised() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    bpm.fetch_page(a).unwrap();
    bpm.unpin_page(a, false); // unchanged
    bpm.fetch_page(b).unwrap(); // evicts a
    bpm.unpin_page(b, false);
    bpm.fetch_page(a).unwrap();
    assert_eq!(disk.writes(), Vec::<i32>::new(), "clean pages are never written back");
}

#[test]
fn s1f_02_dirt_accumulates_across_pins() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    let frame = bpm.fetch_page(a).unwrap();
    bpm.fetch_page(a).unwrap(); // two pins
    bpm.frame_data(frame).write().unwrap()[0] = 9;
    bpm.unpin_page(a, true); // reports the change
    bpm.unpin_page(a, false); // the last unpin says "clean": the earlier report must not be forgotten
    bpm.fetch_page(b).unwrap();
    assert_eq!(disk.written_pages(), BTreeSet::from([a.0]), "the page was reported dirty once, so it is written back");
    bpm.unpin_page(b, false);
    assert_eq!(read_text(&bpm, a, 1).as_bytes(), [9], "and its change survived");
}

#[test]
fn s1f_02_the_pool_tells_the_replacer_when_a_page_is_in_use_and_when_it_is_not() {
    let (bpm, _, spy) = pool_with_spy(3);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    bpm.fetch_page(a).unwrap();
    bpm.fetch_page(a).unwrap();
    bpm.fetch_page(b).unwrap();
    assert_eq!(spy.size(), 0, "pinned pages are not evictable");
    bpm.unpin_page(a, false);
    assert_eq!(spy.size(), 0, "a is still pinned once");
    bpm.unpin_page(a, false);
    assert_eq!(spy.size(), 1, "the last unpin of a makes its frame evictable");
    bpm.fetch_page(a).unwrap();
    assert_eq!(spy.size(), 0, "fetching an unpinned page that is in memory makes it non-evictable again");
    bpm.unpin_page(a, false);
    bpm.unpin_page(b, false);
    assert_eq!(spy.size(), 2, "the pool tells the replacer when a page is in use and when it is not");
}

#[test]
fn s1f_02_many_pages_pass_through_a_small_pool() {
    let (bpm, _) = pool(3);
    let pages: Vec<PageId> = (0..100).map(|_| bpm.new_page()).collect();
    for (i, &page) in pages.iter().enumerate() {
        write_text(&bpm, page, &format!("page number {i}"), true);
    }
    for (i, &page) in pages.iter().enumerate().rev() {
        let text = format!("page number {i}");
        assert_eq!(read_text(&bpm, page, text.len()), text, "many pages pass through a small pool");
    }
}

proptest! {
    #![proptest_config(config())]

    /// More pages than frames, any interleaving of pins, writes and unpins: what was last written is what is read, `fetch_page`
    /// fails exactly when every frame is pinned, pin counts are exact, and a page that was never modified is never written.
    #[test]
    fn s1f_02_a_small_pool_behaves_like_the_model_whatever_is_evicted(ops in ops(false)) {
        let (bpm, disk) = pool(3);
        run_model(&bpm, &disk, 3, &ops, usize::MAX)?;
    }
}

#[test]
fn s1f_02_the_pool_works_under_the_replacers_you_built() {
    for policy in [Policy::Arc, Policy::LruK(2)] {
        let (bpm, disk) = pool_with(policy, 4);
        let pages: Vec<PageId> = (0..40).map(|_| bpm.new_page()).collect();
        for round in 0..3u8 {
            for (i, &page) in pages.iter().enumerate() {
                if (i + round as usize) % 3 == 0 {
                    write_text(&bpm, page, &format!("{policy:?}-{round}-{i}"), true);
                } else {
                    read_text(&bpm, page, 1);
                }
            }
        }
        for (i, &page) in pages.iter().enumerate() {
            let last = (0..3u8).rev().find(|&r| (i + r as usize) % 3 == 0);
            if let Some(r) = last {
                let text = format!("{policy:?}-{r}-{i}");
                assert_eq!(read_text(&bpm, page, text.len()), text, "{policy:?}: page {i}");
            }
        }
        assert!(
            disk.written_pages().len() <= 40,
            "the pool works under the replacers you built: expected `disk.written_pages().len() <= 40`"
        );
    }
}

// ---- 1f-03 · Flush and delete ----------------------------------------------------------------------------------------------

#[test]
fn s1f_03_flush_page_writes_the_page_even_when_it_is_clean() {
    let (bpm, disk) = pool(2);
    let a = bpm.new_page();
    write_text(&bpm, a, "flushed", false); // not reported dirty
    assert!(bpm.flush_page(a), "flush page writes the page even when it is clean: expected `bpm.flush_page(a)`");
    assert_eq!(&disk.stored(a.0)[..7], b"flushed", "flush writes the page's current bytes");
}

#[test]
fn s1f_03_flushing_a_page_that_is_not_in_memory_fails() {
    let (bpm, disk) = pool(2);
    assert!(!bpm.flush_page(p(5)), "flushing a page that is not in memory fails: expected `!bpm.flush_page(p(5))`");
    assert!(disk.writes().is_empty(), "flushing a page that is not in memory fails: expected `disk.writes().is_empty()`");
}

#[test]
fn s1f_03_a_flushed_page_is_clean_so_eviction_does_not_write_it_again() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    write_text(&bpm, a, "x", true);
    assert!(bpm.flush_page(a), "a flushed page is clean so eviction does not write it again: expected `bpm.flush_page(a)`");
    assert_eq!(disk.writes(), vec![a.0], "a flushed page is clean so eviction does not write it again");
    bpm.fetch_page(b).unwrap(); // evicts a
    assert_eq!(disk.writes(), vec![a.0], "page a was clean after the flush: evicting it writes nothing more");
}

#[test]
fn s1f_03_a_pinned_page_can_be_flushed_and_stays_pinned() {
    let (bpm, disk) = pool(2);
    let a = bpm.new_page();
    let frame = bpm.fetch_page(a).unwrap();
    bpm.frame_data(frame).write().unwrap()[0] = 42;
    assert!(bpm.flush_page(a), "a pinned page can be flushed and stays pinned: expected `bpm.flush_page(a)`");
    assert_eq!(disk.stored(a.0)[0], 42, "a pinned page can be flushed and stays pinned");
    assert_eq!(bpm.get_pin_count(a), Some(1), "flushing does not change the pin count");
}

#[test]
fn s1f_03_flush_all_pages_writes_every_page_in_memory() {
    let (bpm, disk) = pool(4);
    let pages: Vec<PageId> = (0..3).map(|_| bpm.new_page()).collect();
    for (i, &page) in pages.iter().enumerate() {
        write_text(&bpm, page, &format!("p{i}"), false);
    }
    bpm.flush_all_pages();
    assert_eq!(disk.written_pages(), pages.iter().map(|q| q.0).collect::<BTreeSet<_>>(), "flush all pages writes every page in memory");
}

#[test]
fn s1f_03_a_pinned_page_cannot_be_deleted_but_an_unpinned_one_can_and_its_frame_is_free_again() {
    let (bpm, disk) = pool(1);
    let (a, b) = (bpm.new_page(), bpm.new_page());
    bpm.fetch_page(a).unwrap();
    assert!(!bpm.delete_page(a), "a pinned page cannot be deleted");
    bpm.unpin_page(a, false);
    assert!(
        bpm.delete_page(a),
        "a pinned page cannot be deleted but an unpinned one can and its frame is free again: expected `bpm.delete_page(a)`"
    );
    assert_eq!(bpm.get_pin_count(a), None, "a deleted page is not in memory");
    assert!(bpm.fetch_page(b).is_some(), "and its frame can be used again");
    assert!(disk.deletes.lock().unwrap().contains(&a.0), "the disk is told to free the page");
}

#[test]
fn s1f_03_deleting_a_page_makes_the_replacer_forget_its_frame() {
    let (bpm, _, spy) = pool_with_spy(2);
    let a = bpm.new_page();
    bpm.fetch_page(a).unwrap();
    bpm.unpin_page(a, false);
    assert_eq!(spy.size(), 1, "deleting a page makes the replacer forget its frame");
    assert!(bpm.delete_page(a), "deleting a page makes the replacer forget its frame: expected `bpm.delete_page(a)`");
    assert_eq!(spy.size(), 0, "the frame is free now, so it must not be offered as a victim any more");
}

#[test]
fn s1f_03_deleting_a_page_that_is_not_in_memory_succeeds_and_a_deleted_dirty_page_is_not_written() {
    let (bpm, disk) = pool(2);
    assert!(bpm.delete_page(p(77)), "nothing to do counts as success");
    let a = bpm.new_page();
    write_text(&bpm, a, "doomed", true);
    assert!(
        bpm.delete_page(a),
        "deleting a page that is not in memory succeeds and a deleted dirty page is not written: expected `bpm.delete_page(a)`"
    );
    assert!(bpm.delete_page(a), "deleting twice is fine");
    assert!(disk.writes().is_empty(), "a page that is deleted is not written back");
}

proptest! {
    #![proptest_config(config())]

    /// The same model with flush and delete in the mix, on a pool smaller than the set of pages.
    #[test]
    fn s1f_03_flush_and_delete_agree_with_the_model(ops in ops(true)) {
        let (bpm, disk) = pool(3);
        run_model(&bpm, &disk, 3, &ops, usize::MAX)?;
    }
}

// ---- 1f-04 · Boss -------------------------------------------------------------------------------------------------------

#[test]
fn s1f_04_threads_share_a_small_pool_without_losing_updates() {
    // 8 threads x 300 increments on random pages out of 40, through a pool of 12 frames. Every increment is a fetch, a change
    // under the frame's write latch, and an unpin(dirty). At the end every page holds exactly the number of increments applied.
    let (bpm, _) = pool_with(Policy::Arc, 12);
    let bpm = Arc::new(bpm);
    let pages: Vec<PageId> = (0..40).map(|_| bpm.new_page()).collect();
    let applied: Arc<Vec<AtomicUsize>> = Arc::new((0..40).map(|_| AtomicUsize::new(0)).collect());
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let (bpm, pages, applied) = (Arc::clone(&bpm), pages.clone(), Arc::clone(&applied));
            thread::spawn(move || {
                let mut x = 1 + t as u64;
                for _ in 0..300 {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    let i = ((x >> 33) % 40) as usize;
                    let frame = loop {
                        if let Some(frame) = bpm.fetch_page(pages[i]) {
                            break frame;
                        }
                        thread::yield_now(); // every frame is pinned right now: try again
                    };
                    {
                        let mut data = bpm.frame_data(frame).write().unwrap();
                        let n = u32::from_le_bytes(data[..4].try_into().unwrap()) + 1;
                        data[..4].copy_from_slice(&n.to_le_bytes());
                    }
                    applied[i].fetch_add(1, Ordering::SeqCst);
                    assert!(
                        bpm.unpin_page(pages[i], true),
                        "threads share a small pool without losing updates: expected `bpm.unpin_page(pages[i], true)`"
                    );
                }
            })
        })
        .collect();
    handles.into_iter().for_each(|h| h.join().unwrap());
    for (i, &page) in pages.iter().enumerate() {
        let frame = bpm.fetch_page(page).expect("a free frame at the end");
        let n = u32::from_le_bytes(bpm.frame_data(frame).read().unwrap()[..4].try_into().unwrap());
        bpm.unpin_page(page, false);
        assert_eq!(n as usize, applied[i].load(Ordering::SeqCst), "page {i}");
    }
}

#[test]
fn s1f_04_the_replacer_changes_which_pages_are_in_memory_but_not_what_they_contain() {
    // The same sequence of writes through three pools with three different replacers must leave every page with the same bytes.
    let mut seen: Vec<Vec<String>> = Vec::new();
    for policy in [Policy::Fifo, Policy::Arc, Policy::LruK(2)] {
        let (bpm, _) = pool_with(policy, 5);
        let pages: Vec<PageId> = (0..30).map(|_| bpm.new_page()).collect();
        let mut x = 7u64;
        for step in 0..600 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let page = pages[((x >> 33) % 30) as usize];
            write_text(&bpm, page, &format!("{step:04}"), true);
        }
        seen.push(pages.iter().map(|&page| read_text(&bpm, page, 4)).collect());
    }
    assert_eq!(seen[0], seen[1], "FIFO and ARC pools must hold the same data");
    assert_eq!(seen[0], seen[2], "FIFO and LRU-K pools must hold the same data");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, ..config() })]

    /// The model, run once under each replacer you built.
    #[test]
    fn s1f_04_the_model_holds_under_arc_and_lru_k(ops in ops(true)) {
        for policy in [Policy::Arc, Policy::LruK(2)] {
            let (bpm, disk) = pool_with(policy, 4);
            run_model(&bpm, &disk, 4, &ops, usize::MAX)?;
        }
    }
}

// @@ challenge 1f-c1 begin
mod ch_1f_c1 {
    use proptest::prelude::*;

    use bustub::buffer::pin_table::{NotPinned, PinTable};
    use std::collections::BTreeMap;

    #[test]
    fn s1f_c1_pins_count_up_and_down() {
        let mut t = PinTable::new();
        assert_eq!((t.pin(3), t.pin(3)), (1, 2), "this check");
        assert_eq!(t.unpin(3), Ok(1), "this check");
        assert_eq!(t.unpin(3), Ok(0), "this check");
        assert!(!t.is_pinned(3), "this check: expected `!t.is_pinned(3)`");
        assert_eq!(t.unpin(3), Err(NotPinned), "this check");
    }

    #[test]
    fn s1f_c1_unpinning_what_was_never_pinned_changes_nothing() {
        let mut t = PinTable::new();
        t.pin(1);
        assert_eq!(t.unpin(2), Err(NotPinned), "this check");
        assert_eq!(t.pinned_pages(), vec![1], "this check");
        assert_eq!(t.count(1), 1, "this check");
    }

    #[test]
    fn s1f_c1_a_page_can_be_pinned_again_after_its_pins_reach_zero() {
        let mut t = PinTable::new();
        t.pin(4);
        assert_eq!(t.unpin(4), Ok(0), "this check");
        assert!(!t.is_pinned(4), "this check: expected `!t.is_pinned(4)`");
        assert_eq!(t.pin(4), 1, "the count starts again from one");
        assert_eq!(t.pinned_pages(), vec![4], "this check");
    }

    #[test]
    fn s1f_c1_pages_are_independent_and_listed_in_order() {
        let mut t = PinTable::new();
        for p in [9, 2, 5, 2] {
            t.pin(p);
        }
        assert_eq!(t.pinned_pages(), vec![2, 5, 9], "this check");
        assert_eq!((t.count(2), t.count(5), t.count(7)), (2, 1, 0), "this check");
        t.unpin(5).unwrap();
        assert_eq!(t.pinned_pages(), vec![2, 9], "a page with no pins is not listed");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a map of counts, with no zero entries.
        #[test]
        fn s1f_c1_property_a_pin_table_matches_a_counting_map(ops in proptest::collection::vec((any::<bool>(), 0u32..5), 0..80)) {
            let mut t = PinTable::new();
            let mut m: BTreeMap<u32, usize> = BTreeMap::new();
            for (pin, p) in ops {
                if pin {
                    *m.entry(p).or_insert(0) += 1;
                    prop_assert_eq!(t.pin(p), m[&p]);
                } else {
                    let want = match m.get_mut(&p) {
                        None => Err(NotPinned),
                        Some(c) => { *c -= 1; let n = *c; if n == 0 { m.remove(&p); } Ok(n) }
                    };
                    prop_assert_eq!(t.unpin(p), want);
                }
                prop_assert_eq!(t.pinned_pages(), m.keys().copied().collect::<Vec<_>>());
            }
        }
    }
}
// @@ challenge 1f-c1 end

// @@ challenge 1f-c2 begin
mod ch_1f_c2 {
    use proptest::prelude::*;

    use bustub::buffer::flush_runs::flush_runs;
    use std::collections::BTreeSet;

    #[test]
    fn s1f_c2_neighbours_merge_and_the_rest_stand_alone() {
        assert_eq!(flush_runs(&[5, 3, 4, 10, 3], 8), vec![(3, 3), (10, 1)], "this check");
    }

    #[test]
    fn s1f_c2_the_cap_splits_a_long_stretch() {
        assert_eq!(flush_runs(&[1, 2, 3, 4, 5], 2), vec![(1, 2), (3, 2), (5, 1)], "this check");
        assert_eq!(flush_runs(&[1, 2, 3], 1), vec![(1, 1), (2, 1), (3, 1)], "this check");
    }

    #[test]
    fn s1f_c2_nothing_in_nothing_out() {
        assert_eq!(flush_runs(&[], 4), vec![], "this check");
        assert_eq!(flush_runs(&[7, 7, 7], 4), vec![(7, 1)], "this check");
    }

    #[test]
    fn s1f_c2_the_largest_page_numbers_do_not_overflow() {
        assert_eq!(flush_runs(&[u32::MAX, u32::MAX - 1], 8), vec![(u32::MAX - 1, 2)], "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: exact coverage, disjoint increasing runs within the cap, the fewest runs, and the same answer for any order.
        #[test]
        fn s1f_c2_property_runs_cover_the_pages_in_the_fewest_writes(pages in proptest::collection::vec(0u32..40, 0..40), max_run in 1u32..8) {
            let runs = flush_runs(&pages, max_run);
            let set: BTreeSet<u32> = pages.iter().copied().collect();
            let covered: Vec<u32> = runs.iter().flat_map(|&(s, l)| s..s + l).collect();
            prop_assert_eq!(covered.clone(), set.iter().copied().collect::<Vec<_>>());
            prop_assert!(runs.iter().all(|&(_, l)| (1..=max_run).contains(&l)));
            // fewest: each maximal consecutive stretch of length n needs ceil(n / max_run) runs
            let mut want = 0u32;
            let mut stretch = 0u32;
            let mut prev: Option<u32> = None;
            for &p in &set {
                if prev == Some(p.wrapping_sub(1)) && prev.is_some() { stretch += 1; } else { want += stretch.div_ceil(max_run); stretch = 1; }
                prev = Some(p);
            }
            want += stretch.div_ceil(max_run);
            prop_assert_eq!(runs.len() as u32, want);
            let mut rev = pages.clone();
            rev.reverse();
            prop_assert_eq!(flush_runs(&rev, max_run), runs);
        }
    }
}
// @@ challenge 1f-c2 end

// @@ challenge 1f-c3 begin
mod ch_1f_c3 {
    use proptest::prelude::*;

    use bustub::buffer::write_behind::WriteBehind;
    use std::collections::BTreeMap;

    #[test]
    fn s1f_c3_pages_dirty_long_enough_are_due_oldest_first() {
        let mut w = WriteBehind::new();
        w.mark_dirty(1, 0);
        w.mark_dirty(3, 5);
        w.mark_dirty(2, 5);
        assert_eq!(w.due(10, 5), vec![1, 2, 3], "ties are by page number");
        assert_eq!(w.due(10, 6), vec![1], "this check");
        assert_eq!(w.due(4, 5), Vec::<u32>::new(), "this check");
    }

    #[test]
    fn s1f_c3_dirtying_a_dirty_page_again_does_not_make_it_younger() {
        let mut w = WriteBehind::new();
        w.mark_dirty(1, 0);
        w.mark_dirty(1, 8);
        assert_eq!(w.oldest(), Some((1, 0)), "this check");
        assert_eq!(w.due(10, 10), vec![1], "this check");
    }

    #[test]
    fn s1f_c3_a_flushed_page_is_clean_and_starts_again_when_dirtied() {
        let mut w = WriteBehind::new();
        w.mark_dirty(1, 0);
        assert!(w.flushed(1), "this check: expected `w.flushed(1)`");
        assert!(!w.flushed(1), "this check: expected `!w.flushed(1)`");
        assert_eq!((w.dirty_count(), w.oldest()), (0, None), "this check");
        w.mark_dirty(1, 20);
        assert_eq!(w.oldest(), Some((1, 20)), "this check");
    }

    #[test]
    fn s1f_c3_a_clock_behind_the_dirty_time_counts_as_age_zero() {
        let mut w = WriteBehind::new();
        w.mark_dirty(1, 100);
        assert_eq!(w.due(50, 0), vec![1], "this check");
        assert_eq!(w.due(50, 1), Vec::<u32>::new(), "this check");
        assert_eq!(w.due(100, 0), vec![1], "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a map of dirty-since times.
        #[test]
        fn s1f_c3_property_due_pages_match_a_model(ops in proptest::collection::vec((0u8..3, 0u32..6, 0u64..50), 0..60), age in 0u64..30) {
            let mut w = WriteBehind::new();
            let mut m: BTreeMap<u32, u64> = BTreeMap::new();
            let mut clock = 0u64;
            for (op, page, dt) in ops {
                clock += dt % 5;
                match op {
                    0 | 1 => { w.mark_dirty(page, clock); m.entry(page).or_insert(clock); }
                    _ => prop_assert_eq!(w.flushed(page), m.remove(&page).is_some()),
                }
                let mut want: Vec<(u64, u32)> = m.iter().filter(|&(_, &s)| clock - s >= age).map(|(&p, &s)| (s, p)).collect();
                want.sort();
                prop_assert_eq!(w.due(clock, age), want.into_iter().map(|(_, p)| p).collect::<Vec<_>>());
                prop_assert_eq!(w.dirty_count(), m.len());
            }
        }
    }
}
// @@ challenge 1f-c3 end

// @@ challenge 1f-c4 begin
mod ch_1f_c4 {
    use proptest::prelude::*;

    use bustub::buffer::seq_detector::SeqDetector;

    #[test]
    fn s1f_c4_nothing_is_suggested_before_the_run_is_long_enough() {
        let mut d = SeqDetector::new(3, 2);
        assert_eq!(d.access(1), Vec::<u32>::new(), "this check");
        assert_eq!(d.access(2), Vec::<u32>::new(), "this check");
        assert_eq!(d.access(3), vec![4, 5], "this check");
    }

    #[test]
    fn s1f_c4_a_continuing_scan_only_gets_the_new_pages() {
        let mut d = SeqDetector::new(3, 2);
        for p in 1..=3 {
            d.access(p);
        }
        assert_eq!(d.access(4), vec![6], "5 was suggested already");
        assert_eq!(d.access(5), vec![7], "this check");
    }

    #[test]
    fn s1f_c4_a_jump_or_a_repeat_starts_over() {
        let mut d = SeqDetector::new(2, 3);
        d.access(10);
        assert_eq!(d.access(11), vec![12, 13, 14], "this check");
        assert_eq!(d.access(30), Vec::<u32>::new(), "a jump resets the run");
        assert_eq!(d.access(31), vec![32, 33, 34], "and the next run can be prefetched afresh");
        assert_eq!(d.access(31), Vec::<u32>::new(), "a repeat is not sequential");
    }

    #[test]
    fn s1f_c4_a_depth_of_zero_never_suggests_and_a_trigger_of_one_suggests_at_once() {
        let mut z = SeqDetector::new(2, 0);
        z.access(1);
        assert_eq!(z.access(2), Vec::<u32>::new(), "this check");
        let mut one = SeqDetector::new(1, 1);
        assert_eq!(one.access(7), vec![8], "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: suggestions are above the accessed page, never repeated within a run, and only made once the run is long enough; a long run
        /// ends with everything up to `last + depth` suggested.
        #[test]
        fn s1f_c4_property_suggestions_follow_the_run(trigger in 1u32..5, depth in 0u32..5, steps in proptest::collection::vec((any::<bool>(), 0u32..50), 0..60)) {
            let mut d = SeqDetector::new(trigger, depth);
            let mut last: Option<u32> = None;
            let mut run = 0u32;
            let mut seen: std::collections::BTreeSet<u32> = Default::default();
            for (seq, jump) in steps {
                let page = match (seq, last) { (true, Some(l)) => l + 1, _ => 1000 + jump * 7 };
                if last.is_some_and(|l| l + 1 == page) { run += 1; } else { run = 1; seen.clear(); }
                last = Some(page);
                let s = d.access(page);
                prop_assert!(s.iter().all(|&x| x > page && x <= page + depth));
                prop_assert!(s.windows(2).all(|w| w[0] < w[1]));
                for x in &s { prop_assert!(seen.insert(*x), "page {} suggested twice in one run", x); }
                if run < trigger { prop_assert!(s.is_empty()); }
                if run >= trigger && depth > 0 {
                    prop_assert!((page + 1..=page + depth).all(|x| seen.contains(&x)), "pages up to {} must have been suggested", page + depth);
                }
            }
        }
    }
}
// @@ challenge 1f-c4 end

// @@ challenge 1f-c5 begin
mod ch_1f_c5 {
    use proptest::prelude::*;

    use bustub::buffer::page_table::PageTable;
    use std::collections::BTreeMap;

    #[test]
    fn s1f_c5_insert_and_look_up_both_ways() {
        let mut t = PageTable::new();
        t.insert(1, 10);
        assert_eq!((t.frame_of(1), t.page_of(10), t.len()), (Some(10), Some(1), 1), "this check");
    }

    #[test]
    fn s1f_c5_a_removed_page_leaves_no_trace_in_either_direction() {
        let mut t = PageTable::new();
        t.insert(1, 10);
        assert_eq!(t.remove_page(1), Some(10), "this check");
        assert_eq!((t.frame_of(1), t.page_of(10), t.len()), (None, None, 0), "the frame no longer claims to hold page 1");
        assert_eq!(t.remove_page(1), None, "this check");
    }

    #[test]
    fn s1f_c5_a_frame_reused_after_its_page_was_removed_holds_only_the_new_page() {
        let mut t = PageTable::new();
        t.insert(1, 10);
        t.remove_page(1);
        t.insert(2, 10);
        assert_eq!((t.page_of(10), t.frame_of(1), t.frame_of(2)), (Some(2), None, Some(10)), "this check");
    }

    #[test]
    fn s1f_c5_moving_a_page_to_another_frame_frees_the_old_one() {
        let mut t = PageTable::new();
        t.insert(1, 10);
        t.insert(1, 11);
        assert_eq!((t.page_of(10), t.page_of(11), t.len()), (None, Some(1), 1), "this check");
    }

    #[test]
    fn s1f_c5_putting_another_page_in_a_used_frame_evicts_the_old_page_from_the_table() {
        let mut t = PageTable::new();
        t.insert(1, 10);
        t.insert(2, 10);
        assert_eq!((t.frame_of(1), t.frame_of(2), t.len()), (None, Some(10), 1), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the table equals a set of (page, frame) pairs with unique columns, and the two directions always mirror each other.
        #[test]
        fn s1f_c5_property_the_two_maps_mirror(ops in proptest::collection::vec((any::<bool>(), 0u32..5, 0u32..5), 0..60)) {
            let mut t = PageTable::new();
            let mut m: BTreeMap<u32, u32> = BTreeMap::new(); // page -> frame
            for (ins, page, frame) in ops {
                if ins {
                    t.insert(page, frame);
                    m.retain(|&p, &mut f| p != page && f != frame);
                    m.insert(page, frame);
                } else {
                    prop_assert_eq!(t.remove_page(page), m.remove(&page));
                }
                prop_assert_eq!(t.len(), m.len());
                for p in 0..5 {
                    prop_assert_eq!(t.frame_of(p), m.get(&p).copied());
                }
                for f in 0..5 {
                    prop_assert_eq!(t.page_of(f), m.iter().find(|&(_, &x)| x == f).map(|(&p, _)| p), "frame {}", f);
                }
            }
        }
    }
}
// @@ challenge 1f-c5 end
