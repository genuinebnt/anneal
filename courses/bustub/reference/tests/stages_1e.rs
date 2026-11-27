//! Tests for module 1e, the ARC replacer. A test name starts with its stage: `s1e_02_…` belongs to stage 1e-02, and
//! `anneal course test` runs just those.
//!
//! The tests use only `ArcReplacer`'s public methods. ARC is specified by a handful of rules (they are written out in the stage
//! pages), so the main property is an **exact model**: the same rules on four plain `Vec`s, run side by side with your replacer on
//! random operation sequences. Stage 1e-01 checks only what every replacer promises; 1e-02 adds the recency/frequency split; 1e-03
//! adds the ghost lists and the adaptive target.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bustub::buffer::arc_replacer::ArcReplacer;
use bustub::buffer::lru_replacer::LruReplacer;
use bustub::buffer::replacer::Replacer;
use bustub::common::config::{FrameId, PageId};
use proptest::prelude::*;

fn f(n: usize) -> FrameId {
    FrameId(n)
}
fn pg(n: i32) -> PageId {
    PageId(n)
}

fn config() -> ProptestConfig {
    ProptestConfig { cases: 96, max_shrink_iters: 3000, failure_persistence: None, ..ProptestConfig::default() }
}

fn within<T: Send + 'static>(what: &str, limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit)
        .unwrap_or_else(|_| panic!("{what} did not finish in {limit:?}: the cost must not grow with the number of frames"))
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

// ---- The model: ARC as BusTub specifies it, on plain vectors -------------------------------------------------------------

/// Four lists, oldest first, and the target size `p` of `mru`. Frames are on `mru` and `mfu`; pages (without frames) on the ghost lists.
struct ArcModel {
    c: usize,
    p: usize,
    mru: Vec<usize>,
    mfu: Vec<usize>,
    mru_ghost: Vec<i32>,
    mfu_ghost: Vec<i32>,
    /// frame -> (page, evictable) for every live frame.
    live: BTreeMap<usize, (i32, bool)>,
}

impl ArcModel {
    fn new(c: usize) -> ArcModel {
        ArcModel { c, p: 0, mru: vec![], mfu: vec![], mru_ghost: vec![], mfu_ghost: vec![], live: BTreeMap::new() }
    }

    fn access(&mut self, frame: usize, page: i32) {
        if self.live.contains_key(&frame) {
            // a hit: the frame goes to the newest end of mfu
            self.mru.retain(|&x| x != frame);
            self.mfu.retain(|&x| x != frame);
            self.mfu.push(frame);
        } else if let Some(i) = self.mru_ghost.iter().position(|&x| x == page) {
            let delta = if self.mru_ghost.len() >= self.mfu_ghost.len() { 1 } else { self.mfu_ghost.len() / self.mru_ghost.len() };
            self.p = (self.p + delta).min(self.c);
            self.mru_ghost.remove(i);
            self.mfu.push(frame);
            self.live.insert(frame, (page, false));
        } else if let Some(i) = self.mfu_ghost.iter().position(|&x| x == page) {
            let delta = if self.mfu_ghost.len() >= self.mru_ghost.len() { 1 } else { self.mru_ghost.len() / self.mfu_ghost.len() };
            self.p = self.p.saturating_sub(delta);
            self.mfu_ghost.remove(i);
            self.mfu.push(frame);
            self.live.insert(frame, (page, false));
        } else {
            let recent = self.mru.len() + self.mru_ghost.len();
            if recent >= self.c {
                if !self.mru_ghost.is_empty() {
                    self.mru_ghost.remove(0);
                }
            } else if recent + self.mfu.len() + self.mfu_ghost.len() >= 2 * self.c && !self.mfu_ghost.is_empty() {
                self.mfu_ghost.remove(0);
            }
            self.mru.push(frame);
            self.live.insert(frame, (page, false));
        }
    }

    fn set_evictable(&mut self, frame: usize, evictable: bool) {
        if let Some(entry) = self.live.get_mut(&frame) {
            entry.1 = evictable;
        }
    }

    fn evict(&mut self) -> Option<usize> {
        let mru_first = self.mru.len() >= self.p;
        let from_mru = self.mru.iter().position(|x| self.live[x].1);
        let from_mfu = self.mfu.iter().position(|x| self.live[x].1);
        let (in_mru, i) = match (mru_first, from_mru, from_mfu) {
            (true, Some(i), _) | (false, Some(i), None) => (true, i),
            (true, None, Some(j)) | (false, _, Some(j)) => (false, j),
            _ => return None,
        };
        let frame = if in_mru { self.mru.remove(i) } else { self.mfu.remove(i) };
        let (page, _) = self.live.remove(&frame).unwrap();
        if in_mru {
            self.mru_ghost.push(page);
        } else {
            self.mfu_ghost.push(page);
        }
        Some(frame)
    }

    /// An eviction that the replacer under test made on its own; only the set of live frames matters afterwards.
    fn forget(&mut self, frame: usize) {
        self.live.remove(&frame);
        self.mru.retain(|&x| x != frame);
        self.mfu.retain(|&x| x != frame);
    }

    fn remove(&mut self, frame: usize) {
        self.forget(frame);
    }

    fn size(&self) -> usize {
        self.live.values().filter(|(_, e)| *e).count()
    }
}

// ---- Random runs ----------------------------------------------------------------------------------------------------------

const PAGES: i32 = 12;

#[derive(Clone, Copy, Debug)]
enum Op {
    Access(usize, i32),
    SetEvictable(usize, bool),
    Evict,
    Remove(usize),
}

fn ops(frames: usize) -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop_oneof![
            5 => (0..frames, 0..PAGES).prop_map(|(x, p)| Op::Access(x, p)),
            4 => (0..frames, any::<bool>()).prop_map(|(x, b)| Op::SetEvictable(x, b)),
            3 => Just(Op::Evict),
            1 => (0..frames).prop_map(Op::Remove),
        ],
        1..250,
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Check {
    /// Any evictable frame will do.
    AnyEvictable,
    /// Exactly the frame ARC picks.
    Arc,
}

#[derive(Clone, Copy, PartialEq)]
enum Pages {
    /// Pages come from the strategy: pages evicted earlier come back, so ghost lists matter.
    Recurring,
    /// Every new use of a frame brings a page never seen before: no ghost is ever hit.
    Fresh,
}

fn run(c: usize, ops: &[Op], check: Check, pages: Pages) -> Result<(), TestCaseError> {
    let mut r = ArcReplacer::new(c);
    let mut m = ArcModel::new(c);
    let mut fresh = 1_000;
    for (step, op) in ops.iter().enumerate() {
        match *op {
            Op::Access(x, wanted) => {
                if x >= c {
                    continue;
                }
                let page = match m.live.get(&x) {
                    Some(&(page, _)) => page, // a frame that is live is accessed again as the same page
                    None if pages == Pages::Fresh => {
                        fresh += 1;
                        fresh
                    }
                    None => (0..PAGES)
                        .map(|d| (wanted + d) % PAGES)
                        .find(|p| m.live.values().all(|(q, _)| q != p))
                        .expect("fewer frames than pages"),
                };
                r.record_access(f(x), pg(page));
                m.access(x, page);
            }
            Op::SetEvictable(x, b) => {
                r.set_evictable(f(x), b);
                m.set_evictable(x, b);
            }
            Op::Remove(x) => {
                if m.live.get(&x).is_some_and(|&(_, evictable)| !evictable) {
                    continue; // a caller bug: it panics, and has its own test
                }
                r.remove(f(x));
                m.remove(x);
            }
            Op::Evict => {
                let got = r.evict();
                match check {
                    Check::Arc => {
                        let want = m.evict();
                        prop_assert_eq!(
                            got,
                            want.map(f),
                            "step {}: evict must return the frame ARC chooses (p = {}, mru {:?}, mfu {:?}, ghosts {:?} / {:?})",
                            step,
                            m.p,
                            m.mru,
                            m.mfu,
                            m.mru_ghost,
                            m.mfu_ghost
                        );
                    }
                    Check::AnyEvictable => match got {
                        None => prop_assert!(m.size() == 0, "step {}: evict returned None but a frame is evictable", step),
                        Some(FrameId(v)) => {
                            prop_assert!(
                                m.live.get(&v).is_some_and(|&(_, e)| e),
                                "step {}: evict returned frame {} which is not an evictable live frame",
                                step,
                                v
                            );
                            m.forget(v);
                        }
                    },
                }
            }
        }
        prop_assert_eq!(r.size(), m.size(), "step {}: size is the number of evictable frames", step);
    }
    Ok(())
}

// ---- 1e-01 · Frames and the contract ---------------------------------------------------------------------------------------

#[test]
fn s1e_01_a_new_frame_is_not_evictable_until_it_is_marked_so() {
    let mut r = ArcReplacer::new(4);
    r.record_access(f(0), pg(10));
    r.record_access(f(1), pg(11));
    assert_eq!((r.size(), r.evict()), (0, None), "new frames are not evictable");
    r.set_evictable(f(0), true);
    r.set_evictable(f(0), true);
    assert_eq!(r.size(), 1, "each evictable frame counts once");
    r.set_evictable(f(0), false);
    assert_eq!(r.size(), 0, "a new frame is not evictable until it is marked so");
}

#[test]
fn s1e_01_accessing_a_live_frame_again_keeps_it_live_and_keeps_its_flag() {
    let mut r = ArcReplacer::new(4);
    r.record_access(f(0), pg(10));
    r.set_evictable(f(0), true);
    r.record_access(f(0), pg(10)); // a hit
    assert_eq!(r.size(), 1, "a hit does not change whether the frame is evictable");
    assert_eq!(r.evict(), Some(f(0)), "accessing a live frame again keeps it live and keeps its flag");
    assert_eq!(r.evict(), None, "and the frame is on the lists once, not twice");
}

#[test]
fn s1e_01_evicting_forgets_the_frame() {
    let mut r = ArcReplacer::new(4);
    r.record_access(f(2), pg(5));
    r.set_evictable(f(2), true);
    assert_eq!(r.evict(), Some(f(2)), "evicting forgets the frame");
    assert_eq!(r.size(), 0, "evicting forgets the frame");
    r.set_evictable(f(2), true); // not live any more: ignored
    assert_eq!(r.size(), 0, "marking an evicted frame evictable does nothing");
}

#[test]
fn s1e_01_remove_forgets_an_evictable_frame_ignores_unknown_ones_and_panics_for_pinned_ones() {
    let mut r = ArcReplacer::new(4);
    r.remove(f(3)); // unknown: nothing happens
    for x in 0..2 {
        r.record_access(f(x), pg(x as i32));
        r.set_evictable(f(x), true);
    }
    r.remove(f(0));
    assert_eq!(
        (r.size(), r.evict(), r.evict()),
        (1, Some(f(1)), None),
        "remove forgets an evictable frame ignores unknown ones and panics for pinned ones"
    );
    let result = std::panic::catch_unwind(|| {
        let mut r = ArcReplacer::new(4);
        r.record_access(f(0), pg(0));
        r.remove(f(0)); // live but not evictable
    });
    assert!(result.is_err(), "removing a frame that is not evictable is a bug in the caller: panic");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1e_01_the_replacer_contract_holds_for_any_operations(c in 2usize..6, ops in ops(5)) {
        run(c.max(5), &ops, Check::AnyEvictable, Pages::Fresh)?;
    }
}

// ---- 1e-02 · Recency and frequency -----------------------------------------------------------------------------------------

/// Marks every frame in `frames` evictable.
fn evictable(r: &mut ArcReplacer, frames: &[usize]) {
    for &x in frames {
        r.set_evictable(f(x), true);
    }
}

fn drain(r: &mut ArcReplacer) -> Vec<usize> {
    std::iter::from_fn(|| r.evict().map(|FrameId(v)| v)).collect()
}

#[test]
fn s1e_02_frames_seen_once_leave_before_frames_seen_twice() {
    let mut r = ArcReplacer::new(4);
    for x in [1, 2] {
        r.record_access(f(x), pg(x as i32));
        r.record_access(f(x), pg(x as i32)); // seen twice: mfu
    }
    for x in [3, 4] {
        r.record_access(f(x), pg(x as i32)); // seen once: mru
    }
    evictable(&mut r, &[1, 2, 3, 4]);
    assert_eq!(
        drain(&mut r),
        [3, 4, 1, 2],
        "mru first (oldest first), then mfu (oldest first): a scan cannot push out the frames seen twice"
    );
}

#[test]
fn s1e_02_a_hit_moves_the_frame_to_the_newest_end_of_mfu() {
    let mut r = ArcReplacer::new(4);
    for x in [1, 2] {
        r.record_access(f(x), pg(x as i32));
        r.record_access(f(x), pg(x as i32));
    }
    r.record_access(f(1), pg(1)); // frame 1 is newest on mfu again
    evictable(&mut r, &[1, 2]);
    assert_eq!(drain(&mut r), [2, 1], "a hit moves the frame to the newest end of mfu");
}

#[test]
fn s1e_02_a_frame_that_is_not_evictable_is_skipped_not_dropped() {
    let mut r = ArcReplacer::new(4);
    for x in [1, 2, 3] {
        r.record_access(f(x), pg(x as i32));
    }
    evictable(&mut r, &[2, 3]); // frame 1, the oldest, is in use
    assert_eq!(r.evict(), Some(f(2)), "a frame that is not evictable is skipped not dropped");
    assert_eq!(r.evict(), Some(f(3)), "a frame that is not evictable is skipped not dropped");
    assert_eq!(r.evict(), None, "a frame that is not evictable is skipped not dropped");
    r.set_evictable(f(1), true);
    assert_eq!(r.evict(), Some(f(1)), "the pinned frame was only skipped");
}

#[test]
fn s1e_02_removing_a_frame_takes_it_off_its_list() {
    let mut r = ArcReplacer::new(4);
    for x in [1, 2, 3] {
        r.record_access(f(x), pg(x as i32));
    }
    evictable(&mut r, &[1, 2, 3]);
    r.remove(f(2));
    assert_eq!(drain(&mut r), [1, 3], "removing a frame takes it off its list");
}

proptest! {
    #![proptest_config(config())]

    /// No page comes back after it was evicted, so ghosts are never hit and the target stays 0: only the two live lists matter.
    #[test]
    fn s1e_02_victims_follow_mru_then_mfu_for_any_operations(ops in ops(5)) {
        run(5, &ops, Check::Arc, Pages::Fresh)?;
    }
}

#[test]
fn s1e_02_a_hit_on_a_very_long_list_stays_fast() {
    // 100 000 frames on mfu and 400 000 hits in the middle of it. A list walked from the end per hit costs 50 000 steps each.
    within("400 000 hits on a replacer of 100 000 frames", Duration::from_secs(10), || {
        let n = 100_000;
        let mut r = ArcReplacer::new(n);
        for x in 0..n {
            r.record_access(f(x), pg(x as i32));
            r.record_access(f(x), pg(x as i32));
            r.set_evictable(f(x), true);
        }
        let mut at = n / 2;
        for _ in 0..4 * n {
            r.record_access(f(at), pg(at as i32));
            at = (at + 1) % n;
        }
        assert_eq!(r.size(), n, "a hit on a very long list stays fast");
    });
}

// ---- 1e-03 · Ghosts and the adaptive target --------------------------------------------------------------------------------

#[test]
fn s1e_03_a_ghost_hit_on_mru_ghost_raises_the_target_so_mfu_is_evicted_from_first() {
    let mut r = ArcReplacer::new(4);
    for x in 0..4 {
        r.record_access(f(x), pg(x as i32));
    }
    evictable(&mut r, &[0, 1, 2, 3]);
    assert_eq!([r.evict(), r.evict()], [Some(f(0)), Some(f(1))], "mru goes first; pages 0 and 1 become ghosts");
    r.record_access(f(0), pg(0)); // a ghost hit: the target grows to 1, and page 0 returns on mfu
    r.record_access(f(1), pg(1)); // another: the target is now 2
    evictable(&mut r, &[0, 1]);
    // mru holds 2 frames (2 and 3) and the target is 2, so mru still gives the first victim; then it holds 1, below the target,
    // so the next victims come from mfu until it is empty.
    assert_eq!(drain(&mut r), [2, 0, 1, 3], "after two ghost hits the pool keeps mru frames and evicts from mfu");
}

#[test]
fn s1e_03_without_ghost_hits_the_target_stays_zero_and_mru_always_goes_first() {
    let mut r = ArcReplacer::new(4);
    for x in 0..4 {
        r.record_access(f(x), pg(x as i32));
    }
    evictable(&mut r, &[0, 1, 2, 3]);
    assert_eq!([r.evict(), r.evict()], [Some(f(0)), Some(f(1))], "without ghost hits the target stays zero and mru always goes first");
    r.record_access(f(0), pg(10)); // new pages, not ghosts
    r.record_access(f(1), pg(11));
    evictable(&mut r, &[0, 1]);
    assert_eq!(drain(&mut r), [2, 3, 0, 1], "all four are on mru, so oldest first");
}

#[test]
fn s1e_03_a_removed_frame_leaves_no_ghost() {
    let mut r = ArcReplacer::new(4);
    for x in 0..4 {
        r.record_access(f(x), pg(x as i32));
    }
    evictable(&mut r, &[0, 1, 2, 3]);
    r.remove(f(0));
    r.remove(f(1));
    r.record_access(f(0), pg(0)); // page 0 was deleted, not evicted: not a ghost, so no adaptation
    r.record_access(f(1), pg(1));
    evictable(&mut r, &[0, 1]);
    assert_eq!(drain(&mut r), [2, 3, 0, 1], "pages brought back after a remove are new pages");
}

#[test]
fn s1e_03_a_ghost_that_has_been_pushed_out_of_its_list_is_forgotten() {
    // c = 2: mru + mru_ghost may hold two entries, so page 0's ghost is dropped as soon as two newer pages arrive.
    let mut r = ArcReplacer::new(2);
    r.record_access(f(0), pg(0));
    r.set_evictable(f(0), true);
    assert_eq!(r.evict(), Some(f(0)), "a ghost that has been pushed out of its list is forgotten"); // page 0 is a ghost
    r.record_access(f(0), pg(1));
    r.set_evictable(f(0), true);
    r.record_access(f(1), pg(2)); // mru + mru_ghost would exceed 2: the oldest ghost, page 0, is dropped
    r.set_evictable(f(1), true);
    r.record_access(f(0), pg(1)); // hit: promotes frame 0 to mfu
    r.evict(); // frame 1 (page 2) from mru, becomes a ghost
    r.record_access(f(1), pg(0)); // page 0 again: a new page now, not a ghost hit
    r.set_evictable(f(1), true);
    // if page 0 had still been a ghost, the target would have grown to 1 and mfu would be evicted from first only below that
    assert_eq!(drain(&mut r), [1, 0], "page 0 was forgotten, so it joined mru as a new page and is evicted first");
}

proptest! {
    #![proptest_config(config())]

    /// ARC against its model on a small pool with pages that come back, so ghost hits, target changes and ghost-list limits all happen.
    #[test]
    fn s1e_03_victims_agree_with_the_model_for_any_operations(c in 2usize..6, ops in ops(5)) {
        run(c.max(5), &ops, Check::Arc, Pages::Recurring)?;
    }

    /// The same on the smallest pools, where the limits on the ghost lists bite after a few operations.
    #[test]
    fn s1e_03_the_ghost_list_limits_hold_on_tiny_pools(ops in prop::collection::vec(prop_oneof![
            5 => (0..3usize, 0..PAGES).prop_map(|(x, p)| Op::Access(x, p)),
            4 => (0..3usize, any::<bool>()).prop_map(|(x, b)| Op::SetEvictable(x, b)),
            3 => Just(Op::Evict),
        ], 1..300)) {
        run(3, &ops, Check::Arc, Pages::Recurring)?;
    }
}

// ---- 1e-04 · Boss ----------------------------------------------------------------------------------------------------------

#[test]
fn s1e_04_long_runs_on_a_larger_pool_agree_with_the_model() {
    let (c, pages) = (64, 200);
    let mut rng = Lcg(2025);
    let mut r = ArcReplacer::new(c);
    let mut m = ArcModel::new(c);
    for step in 0..30_000 {
        let x = rng.next(c);
        match rng.next(10) {
            0..=4 => {
                let page = match m.live.get(&x) {
                    Some(&(p, _)) => p,
                    None => loop {
                        let p = rng.next(pages) as i32;
                        if m.live.values().all(|(q, _)| *q != p) {
                            break p;
                        }
                    },
                };
                r.record_access(f(x), pg(page));
                m.access(x, page);
            }
            5 | 6 => {
                let b = rng.next(4) != 0;
                r.set_evictable(f(x), b);
                m.set_evictable(x, b);
            }
            7 | 8 => assert_eq!(r.evict(), m.evict().map(f), "step {step}: the victim"),
            _ => {
                if m.live.get(&x).is_none_or(|&(_, e)| e) {
                    r.remove(f(x));
                    m.remove(x);
                }
            }
        }
        assert_eq!(r.size(), m.size(), "step {step}: size");
    }
}

/// Replays a trace through a pool of `frames` frames. `access` is called for a page that is in memory, `load` for one that is not
/// (with the frame to put it in, found by `victim` when none is free).
fn arc_hit_rate(frames: usize, trace: &[i32]) -> f64 {
    let mut r = ArcReplacer::new(frames);
    let mut where_is: std::collections::HashMap<i32, usize> = Default::default();
    let mut holds: Vec<Option<i32>> = vec![None; frames];
    let mut free: Vec<usize> = (0..frames).rev().collect();
    let mut hits = 0;
    for &page in trace {
        if let Some(&frame) = where_is.get(&page) {
            hits += 1;
            r.set_evictable(f(frame), false); // in use
            r.record_access(f(frame), pg(page));
            r.set_evictable(f(frame), true); // and done
        } else {
            let frame = free.pop().unwrap_or_else(|| r.evict().expect("every page is unpinned").0);
            if let Some(old) = holds[frame] {
                where_is.remove(&old);
            }
            holds[frame] = Some(page);
            where_is.insert(page, frame);
            r.record_access(f(frame), pg(page));
            r.set_evictable(f(frame), true);
        }
    }
    hits as f64 / trace.len() as f64
}

fn lru_hit_rate(frames: usize, trace: &[i32]) -> f64 {
    let mut r = LruReplacer::new(frames);
    let mut where_is: std::collections::HashMap<i32, usize> = Default::default();
    let mut holds: Vec<Option<i32>> = vec![None; frames];
    let mut free: Vec<usize> = (0..frames).rev().collect();
    let mut hits = 0;
    for &page in trace {
        if let Some(&frame) = where_is.get(&page) {
            hits += 1;
            r.pin(f(frame));
            r.unpin(f(frame));
        } else {
            let frame = free.pop().unwrap_or_else(|| r.victim().expect("every page is unpinned").0);
            if let Some(old) = holds[frame] {
                where_is.remove(&old);
            }
            holds[frame] = Some(page);
            where_is.insert(page, frame);
            r.unpin(f(frame));
        }
    }
    hits as f64 / trace.len() as f64
}

/// The reason ARC exists: 24 hot pages accessed all the time, and now and then a scan of 300 pages that are never used again.
#[test]
fn s1e_04_arc_keeps_the_hot_pages_through_scans_better_than_lru() {
    let mut rng = Lcg(11);
    let mut trace = Vec::new();
    let mut scan_page = 1_000;
    for round in 0..200 {
        for _ in 0..200 {
            trace.push(rng.next(24) as i32);
        }
        if round % 4 == 3 {
            for _ in 0..60 {
                trace.push(scan_page);
                scan_page += 1;
            }
        }
    }
    let (arc, lru) = (arc_hit_rate(32, &trace), lru_hit_rate(32, &trace));
    eprintln!("hit rate: ARC {arc:.3}, LRU {lru:.3}");
    assert!(arc > lru + 0.02, "a scan should hurt ARC less than LRU: ARC {arc:.3} against LRU {lru:.3}");
}
