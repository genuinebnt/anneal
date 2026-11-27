//! Tests for module 1d, the LRU-K replacer. A test name starts with its stage: `s1d_02_…` belongs to stage 1d-02, and
//! `anneal course test` runs just those.
//!
//! The tests use only `LruKReplacer`'s public methods. Random sequences of accesses, evictability changes, evictions and removals run
//! on your replacer and on a **model**: a plain map from frame to its full list of access times, with the LRU-K rule applied by
//! brute force. Stage 1d-01 checks what every replacer promises (a victim is always an evictable frame, the count is right, an evicted
//! frame is forgotten); 1d-02 adds the LRU-K choice of victim; 1d-03 adds a time limit.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use bustub::buffer::lru_k_replacer::LruKReplacer;
use bustub::common::config::FrameId;
use proptest::prelude::*;

fn f(n: usize) -> FrameId {
    FrameId(n)
}

fn config() -> ProptestConfig {
    ProptestConfig { cases: 64, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
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

const FRAMES: usize = 8;

#[derive(Clone, Copy, Debug)]
enum Op {
    Access(usize),
    SetEvictable(usize, bool),
    Evict,
    Remove(usize),
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop_oneof![
            5 => (0..FRAMES).prop_map(Op::Access),
            3 => (0..FRAMES, any::<bool>()).prop_map(|(x, b)| Op::SetEvictable(x, b)),
            2 => Just(Op::Evict),
            1 => (0..FRAMES).prop_map(Op::Remove),
        ],
        1..200,
    )
}

/// What the replacer knows about a tracked frame, kept in full.
struct Tracked {
    evictable: bool,
    /// Every access since the frame was last forgotten.
    times: Vec<usize>,
}

/// The LRU-K rule by brute force. Frames with fewer than `k` accesses have infinite backward distance and go first, oldest first
/// access first; then frames with `k` accesses, by the time of their k-th most recent access (older first).
fn model_victim(model: &BTreeMap<usize, Tracked>, k: usize) -> Option<usize> {
    model
        .iter()
        .filter(|(_, t)| t.evictable)
        .min_by_key(|(_, t)| if t.times.len() < k { (0, t.times[0]) } else { (1, t.times[t.times.len() - k]) })
        .map(|(&frame, _)| frame)
}

/// How a run decides what an eviction must return.
#[derive(Clone, Copy)]
enum Check {
    /// Any evictable frame.
    AnyEvictable,
    /// Exactly the frame LRU-K would pick.
    LruK,
}

fn run(k: usize, ops: &[Op], check: Check) -> Result<(), TestCaseError> {
    let mut r = LruKReplacer::new(FRAMES, k);
    let mut model: BTreeMap<usize, Tracked> = BTreeMap::new();
    let mut clock = 0;
    for (step, op) in ops.iter().enumerate() {
        match *op {
            Op::Access(x) => {
                r.record_access(f(x));
                model.entry(x).or_insert(Tracked { evictable: false, times: Vec::new() }).times.push(clock);
                clock += 1;
            }
            Op::SetEvictable(x, b) => {
                r.set_evictable(f(x), b);
                if let Some(t) = model.get_mut(&x) {
                    t.evictable = b;
                }
            }
            Op::Remove(x) => match model.get(&x) {
                Some(t) if !t.evictable => {} // a caller bug: it panics, and has its own test
                _ => {
                    r.remove(f(x));
                    model.remove(&x);
                }
            },
            Op::Evict => {
                let got = r.evict();
                let want = model_victim(&model, k);
                match check {
                    Check::LruK => prop_assert_eq!(
                        got,
                        want.map(f),
                        "step {}: evict must return the frame with the largest backward k-distance (k = {})",
                        step,
                        k
                    ),
                    Check::AnyEvictable => match got {
                        None => prop_assert!(want.is_none(), "step {}: evict returned None but a frame is evictable", step),
                        Some(FrameId(v)) => prop_assert!(
                            model.get(&v).is_some_and(|t| t.evictable),
                            "step {}: evict returned frame {} which is not an evictable frame",
                            step,
                            v
                        ),
                    },
                }
                if let Some(FrameId(v)) = got {
                    model.remove(&v);
                }
            }
        }
        let evictable = model.values().filter(|t| t.evictable).count();
        prop_assert_eq!(r.size(), evictable, "step {}: size is the number of evictable frames", step);
    }
    Ok(())
}

// ---- 1d-01 · Frames, accesses and the count --------------------------------------------------------------------------

#[test]
fn s1d_01_a_frame_is_not_evictable_until_it_is_marked_so() {
    let mut r = LruKReplacer::new(4, 2);
    r.record_access(f(1));
    r.record_access(f(2));
    assert_eq!(r.size(), 0, "new frames are not evictable");
    assert_eq!(r.evict(), None, "nothing is evictable yet");
    r.set_evictable(f(1), true);
    r.set_evictable(f(1), true); // setting it again changes nothing
    assert_eq!(r.size(), 1, "a frame is not evictable until it is marked so");
    r.set_evictable(f(2), true);
    assert_eq!(r.size(), 2, "a frame is not evictable until it is marked so");
    r.set_evictable(f(1), false);
    r.set_evictable(f(1), false);
    assert_eq!(r.size(), 1, "size counts evictable frames, each once");
}

#[test]
fn s1d_01_evicting_forgets_the_frame() {
    let mut r = LruKReplacer::new(4, 2);
    r.record_access(f(0));
    r.set_evictable(f(0), true);
    assert_eq!(r.evict(), Some(f(0)), "evicting forgets the frame");
    assert_eq!(r.size(), 0, "evicting forgets the frame");
    assert_eq!(r.evict(), None, "the only frame is gone");
    r.set_evictable(f(0), true); // not tracked any more: ignored
    assert_eq!(r.size(), 0, "an evicted frame is forgotten, so marking it evictable does nothing");
    r.record_access(f(0)); // tracked again, as a new frame
    assert_eq!(r.size(), 0, "and it starts out not evictable");
}

#[test]
fn s1d_01_unknown_frames_are_ignored_by_set_evictable_and_remove() {
    let mut r = LruKReplacer::new(4, 2);
    r.set_evictable(f(3), true);
    r.remove(f(3));
    assert_eq!(r.size(), 0, "unknown frames are ignored by set evictable and remove");
    assert_eq!(r.evict(), None, "unknown frames are ignored by set evictable and remove");
}

#[test]
fn s1d_01_remove_takes_out_an_evictable_frame_whatever_its_distance() {
    let mut r = LruKReplacer::new(4, 2);
    for x in [0, 1, 2] {
        r.record_access(f(x));
        r.set_evictable(f(x), true);
    }
    r.remove(f(1));
    assert_eq!(r.size(), 2, "remove takes out an evictable frame whatever its distance");
    let mut left = vec![r.evict().unwrap(), r.evict().unwrap()];
    left.sort();
    assert_eq!(left, [f(0), f(2)], "frame 1 is gone");
}

#[test]
fn s1d_01_misuse_panics() {
    assert!(std::panic::catch_unwind(|| LruKReplacer::new(4, 0)).is_err(), "k = 0 is meaningless: panic");
    assert!(
        std::panic::catch_unwind(|| LruKReplacer::new(4, 2).record_access(f(4))).is_err(),
        "frame 4 is out of range for 4 frames: panic"
    );
    let result = std::panic::catch_unwind(|| {
        let mut r = LruKReplacer::new(4, 2);
        r.record_access(f(0));
        r.remove(f(0)); // tracked but not evictable
    });
    assert!(result.is_err(), "removing a frame that is not evictable is a bug in the caller: panic");
}

proptest! {
    #![proptest_config(config())]

    /// What every replacer promises, whatever its policy: size is the number of evictable frames, a victim is one of them and is
    /// forgotten, remove forgets a frame.
    #[test]
    fn s1d_01_the_replacer_contract_holds_for_any_operations(k in 1usize..5, ops in ops()) {
        run(k, &ops, Check::AnyEvictable)?;
    }
}

// ---- 1d-02 · Backward k-distance -------------------------------------------------------------------------------------------

#[test]
fn s1d_02_a_frame_with_fewer_than_k_accesses_goes_before_one_with_k() {
    let mut r = LruKReplacer::new(8, 2);
    r.record_access(f(0)); // hot: two accesses
    r.record_access(f(0));
    for x in 1..=4 {
        r.record_access(f(x)); // a scan: one access each
    }
    for x in 0..=4 {
        r.set_evictable(f(x), true);
    }
    assert_eq!(
        [r.evict(), r.evict(), r.evict(), r.evict()],
        [Some(f(1)), Some(f(2)), Some(f(3)), Some(f(4))],
        "the scanned frames go first, oldest first"
    );
    assert_eq!(r.evict(), Some(f(0)), "the frame accessed twice survives the scan");
}

#[test]
fn s1d_02_among_frames_with_k_accesses_the_oldest_kth_access_goes_first() {
    let mut r = LruKReplacer::new(8, 2);
    for x in [0, 1, 0, 1, 1] {
        r.record_access(f(x)); // times: 0@0, 1@1, 0@2, 1@3, 1@4
    }
    r.set_evictable(f(0), true);
    r.set_evictable(f(1), true);
    // frame 0's 2nd most recent access is time 0, frame 1's is time 3: frame 0 has the larger backward distance
    assert_eq!(r.evict(), Some(f(0)), "among frames with k accesses the oldest kth access goes first");
    assert_eq!(r.evict(), Some(f(1)), "among frames with k accesses the oldest kth access goes first");
}

#[test]
fn s1d_02_the_distance_is_measured_to_the_kth_most_recent_access_not_the_first() {
    let mut r = LruKReplacer::new(8, 2);
    for x in [0, 1, 1, 0] {
        r.record_access(f(x)); // 0@0, 1@1, 1@2, 0@3
    }
    r.set_evictable(f(0), true);
    r.set_evictable(f(1), true);
    // frame 0's 2nd most recent access is time 0, frame 1's is time 1: frame 0 goes first although it was accessed last
    assert_eq!(r.evict(), Some(f(0)), "k-th most recent access, not the most recent one");
}

#[test]
fn s1d_02_marking_a_frame_non_evictable_keeps_its_history() {
    let mut r = LruKReplacer::new(8, 2);
    for x in [0, 0, 1, 1] {
        r.record_access(f(x));
    }
    r.set_evictable(f(0), true);
    r.set_evictable(f(1), true);
    r.set_evictable(f(0), false); // pinned for a while
    assert_eq!(r.evict(), Some(f(1)), "marking a frame non evictable keeps its history");
    r.set_evictable(f(0), true);
    assert_eq!(r.evict(), Some(f(0)), "marking a frame non evictable keeps its history");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1d_02_victims_follow_the_backward_k_distance_for_any_operations(k in 1usize..5, ops in ops()) {
        run(k, &ops, Check::LruK)?;
    }

    /// With k = 1 the k-th most recent access is the latest one, so LRU-K is plain LRU by last access.
    #[test]
    fn s1d_02_with_k_equal_to_one_the_victim_is_the_least_recently_accessed(ops in ops()) {
        let mut r = LruKReplacer::new(FRAMES, 1);
        let mut order: Vec<usize> = Vec::new(); // tracked frames, least recently accessed first
        let mut evictable = [false; FRAMES];
        for (step, op) in ops.iter().enumerate() {
            match *op {
                Op::Access(x) => { r.record_access(f(x)); order.retain(|&y| y != x); order.push(x); }
                Op::SetEvictable(x, b) => { r.set_evictable(f(x), b); if order.contains(&x) { evictable[x] = b; } }
                Op::Remove(x) => if !order.contains(&x) || evictable[x] { r.remove(f(x)); order.retain(|&y| y != x); evictable[x] = false; },
                Op::Evict => {
                    let want = order.iter().copied().find(|&y| evictable[y]);
                    prop_assert_eq!(r.evict(), want.map(f), "step {}: with k = 1 the victim is the least recently accessed evictable frame", step);
                    if let Some(v) = want { order.retain(|&y| y != v); evictable[v] = false; }
                }
            }
        }
    }

    /// A scan cannot push out a frame that has been accessed k times: no frame with k accesses is evicted while a frame with fewer is evictable.
    #[test]
    fn s1d_02_a_frame_with_k_accesses_is_never_evicted_while_a_scanned_frame_is_evictable(k in 2usize..5, ops in ops()) {
        let mut r = LruKReplacer::new(FRAMES, k);
        let mut count = [0usize; FRAMES]; // accesses since tracked
        let mut tracked = [false; FRAMES];
        let mut evictable = [false; FRAMES];
        for (step, op) in ops.iter().enumerate() {
            match *op {
                Op::Access(x) => { r.record_access(f(x)); tracked[x] = true; count[x] += 1; }
                Op::SetEvictable(x, b) => { r.set_evictable(f(x), b); if tracked[x] { evictable[x] = b; } }
                Op::Remove(x) => if !tracked[x] || evictable[x] { r.remove(f(x)); tracked[x] = false; evictable[x] = false; count[x] = 0; },
                Op::Evict => {
                    let scanned_exists = (0..FRAMES).any(|y| evictable[y] && count[y] < k);
                    if let Some(FrameId(v)) = r.evict() {
                        prop_assert!(!(scanned_exists && count[v] >= k), "step {}: frame {} with {} accesses was evicted while a frame with fewer than k = {} was evictable", step, v, count[v], k);
                        tracked[v] = false; evictable[v] = false; count[v] = 0;
                    }
                }
            }
        }
    }

    /// With no frame reaching k accesses every distance is infinite, and the order is first access first.
    #[test]
    fn s1d_02_when_no_frame_has_k_accesses_the_order_is_by_first_access(k in 2usize..5, accesses in prop::collection::vec(0..FRAMES, 1..40)) {
        let mut r = LruKReplacer::new(FRAMES, k);
        let mut count = [0usize; FRAMES];
        let mut first_seen: Vec<usize> = Vec::new();
        for x in accesses {
            if count[x] + 1 >= k { continue; } // keep every frame below k accesses
            count[x] += 1;
            if !first_seen.contains(&x) { first_seen.push(x); }
            r.record_access(f(x));
        }
        for &x in &first_seen { r.set_evictable(f(x), true); }
        let evicted: Vec<usize> = std::iter::from_fn(|| r.evict().map(|FrameId(v)| v)).collect();
        prop_assert_eq!(evicted, first_seen, "oldest first access goes first");
    }
}

// ---- 1d-03 · Fast eviction -------------------------------------------------------------------------------------------------

#[test]
fn s1d_03_eviction_stays_fast_with_a_hundred_thousand_frames() {
    // Evicting by scanning all frames costs about 100 000 steps each; 100 000 evictions would take minutes.
    within("100 000 evictions among 100 000 frames", Duration::from_secs(10), || {
        let n = 100_000;
        let mut r = LruKReplacer::new(n, 2);
        for x in 0..n {
            r.record_access(f(x));
            if x % 2 == 0 {
                r.record_access(f(x));
            }
            r.set_evictable(f(x), true);
        }
        assert_eq!(r.size(), n, "eviction stays fast with a hundred thousand frames");
        let mut rng = Lcg(5);
        let start = Instant::now();
        for _ in 0..n {
            let victim = r.evict().expect("the replacer is full of evictable frames");
            // the pool reuses the frame for another page: one access, then it is evictable again
            r.record_access(victim);
            r.set_evictable(victim, rng.next(10) != 0);
        }
        eprintln!("100 000 evictions took {:?}", start.elapsed());
    });
}

#[test]
fn s1d_03_an_access_to_an_evictable_frame_moves_it_in_the_eviction_order() {
    let mut r = LruKReplacer::new(8, 2);
    for x in [0, 0, 1, 1, 2, 2] {
        r.record_access(f(x));
    }
    for x in 0..3 {
        r.set_evictable(f(x), true);
    }
    r.record_access(f(0));
    r.record_access(f(0)); // frame 0's two latest accesses are now the most recent ones
    assert_eq!(
        [r.evict(), r.evict(), r.evict()],
        [Some(f(1)), Some(f(2)), Some(f(0))],
        "the order must change when an evictable frame is accessed"
    );
}

#[test]
fn s1d_03_a_frame_that_stops_being_evictable_is_no_longer_a_candidate() {
    let mut r = LruKReplacer::new(8, 2);
    for x in [0, 1, 2] {
        r.record_access(f(x));
        r.set_evictable(f(x), true);
    }
    r.set_evictable(f(0), false); // the best victim is pinned
    assert_eq!(r.evict(), Some(f(1)), "a frame that stops being evictable is no longer a candidate");
    r.record_access(f(0)); // accessing a pinned frame must not make it a candidate
    assert_eq!(r.evict(), Some(f(2)), "a frame that stops being evictable is no longer a candidate");
    assert_eq!(r.evict(), None, "frame 0 is still not evictable");
}

#[test]
fn s1d_03_removing_the_best_victim_leaves_the_next_best() {
    let mut r = LruKReplacer::new(8, 2);
    for x in [0, 1, 2] {
        r.record_access(f(x));
        r.set_evictable(f(x), true);
    }
    r.remove(f(0));
    assert_eq!([r.evict(), r.evict(), r.evict()], [Some(f(1)), Some(f(2)), None], "removing the best victim leaves the next best");
}

#[test]
fn s1d_03_a_larger_pool_still_agrees_with_the_model() {
    let n = 1_000;
    let mut rng = Lcg(31);
    let mut r = LruKReplacer::new(n, 3);
    let mut model: BTreeMap<usize, Tracked> = BTreeMap::new();
    let mut clock = 0;
    for step in 0..30_000 {
        let x = rng.next(n);
        match rng.next(6) {
            0..=2 => {
                r.record_access(f(x));
                model.entry(x).or_insert(Tracked { evictable: false, times: Vec::new() }).times.push(clock);
                clock += 1;
            }
            3 | 4 => {
                r.set_evictable(f(x), true);
                if let Some(t) = model.get_mut(&x) {
                    t.evictable = true;
                }
            }
            _ => {
                let want = model_victim(&model, 3);
                assert_eq!(r.evict(), want.map(f), "step {step}: the victim");
                if let Some(v) = want {
                    model.remove(&v);
                }
            }
        }
    }
}

// ---- 1d-04 · Boss ----------------------------------------------------------------------------------------------------------

#[test]
fn s1d_04_long_runs_on_many_frames_agree_with_the_model() {
    for k in [1, 2, 3, 5] {
        let n = 150;
        let mut rng = Lcg(77 + k as u64);
        let mut r = LruKReplacer::new(n, k);
        let mut model: BTreeMap<usize, Tracked> = BTreeMap::new();
        let mut clock = 0;
        for step in 0..15_000 {
            let x = rng.next(n);
            match rng.next(8) {
                0..=3 => {
                    r.record_access(f(x));
                    model.entry(x).or_insert(Tracked { evictable: false, times: Vec::new() }).times.push(clock);
                    clock += 1;
                }
                4 | 5 => {
                    let b = rng.next(3) != 0;
                    r.set_evictable(f(x), b);
                    if let Some(t) = model.get_mut(&x) {
                        t.evictable = b;
                    }
                }
                6 => {
                    let want = model_victim(&model, k);
                    assert_eq!(r.evict(), want.map(f), "k = {k}, step {step}: the victim");
                    if let Some(v) = want {
                        model.remove(&v);
                    }
                }
                _ => {
                    if model.get(&x).is_none_or(|t| t.evictable) {
                        r.remove(f(x));
                        model.remove(&x);
                    }
                }
            }
            assert_eq!(r.size(), model.values().filter(|t| t.evictable).count(), "k = {k}, step {step}: size");
        }
    }
}

// ---- 1d-c1 and 1d-c2: challenges ------------------------------------------------------------------------------------------------------------

use bustub::buffer::cache_sim::{belady_hits, simulate};
use bustub::buffer::fifo_replacer::FifoReplacer;
use bustub::buffer::k_history::KHistory;
use bustub::buffer::lfu_replacer::LfuReplacer;
use bustub::buffer::replacer::FrameReplacer;

/// A least-recently-used policy written for the test, so that the simulator is checked against a policy that is not the learner's.
struct TestLru {
    order: Vec<FrameId>,
    evictable: Vec<FrameId>,
}

impl FrameReplacer for TestLru {
    fn record_access(&mut self, frame: FrameId, _page: bustub::common::config::PageId) {
        self.order.retain(|x| *x != frame);
        self.order.push(frame);
    }
    fn set_evictable(&mut self, frame: FrameId, evictable: bool) {
        self.evictable.retain(|x| *x != frame);
        if evictable && self.order.contains(&frame) {
            self.evictable.push(frame);
        }
    }
    fn evict(&mut self) -> Option<FrameId> {
        let at = self.order.iter().position(|x| self.evictable.contains(x))?;
        let frame = self.order.remove(at);
        self.evictable.retain(|x| *x != frame);
        Some(frame)
    }
    fn remove(&mut self, frame: FrameId) {
        self.order.retain(|x| *x != frame);
        self.evictable.retain(|x| *x != frame);
    }
    fn size(&self) -> usize {
        self.evictable.len()
    }
}

fn lru() -> Box<dyn FrameReplacer> {
    Box::new(TestLru { order: Vec::new(), evictable: Vec::new() })
}

#[test]
fn s1d_c1_a_pool_big_enough_for_every_page_only_misses_the_first_time() {
    let trace = [1, 2, 3, 1, 2, 3, 1, 2, 3];
    assert_eq!(simulate(&trace, 3, lru()), 6, "three misses, six hits");
    assert_eq!(simulate(&trace, 10, lru()), 6, "s1d c1 a pool big enough for every page only misses the first time");
    assert_eq!(belady_hits(&trace, 3), 6, "s1d c1 a pool big enough for every page only misses the first time");
}

#[test]
fn s1d_c1_a_pool_of_one_frame_hits_only_on_immediate_repeats() {
    assert_eq!(simulate(&[1, 1, 2, 2, 2, 1, 3], 1, lru()), 3, "the repeats of 1, 2 and 2");
    assert_eq!(belady_hits(&[1, 1, 2, 2, 2, 1, 3], 1), 3, "s1d c1 a pool of one frame hits only on immediate repeats");
}

#[test]
fn s1d_c1_no_frames_means_no_hits_and_an_empty_trace_has_none() {
    assert_eq!(simulate(&[1, 2, 1], 0, lru()), 0, "s1d c1 no frames means no hits and an empty trace has none");
    assert_eq!(belady_hits(&[1, 2, 1], 0), 0, "s1d c1 no frames means no hits and an empty trace has none");
    assert_eq!(simulate(&[], 4, lru()), 0, "s1d c1 no frames means no hits and an empty trace has none");
    assert_eq!(belady_hits(&[], 4), 0, "s1d c1 no frames means no hits and an empty trace has none");
}

#[test]
fn s1d_c1_a_scan_is_what_separates_lru_from_least_frequently_used() {
    // a hot page 0 used between a long scan of cold pages 100.. in a pool of 3 frames
    let mut trace = Vec::new();
    for i in 0..60u32 {
        trace.push(0);
        trace.push(100 + i);
    }
    let lru_hits = simulate(&trace, 3, lru());
    let lfu_hits = simulate(&trace, 3, Box::new(LfuReplacer::new()));
    assert!(lfu_hits >= 59, "the hot page is never evicted by a frequency policy: {lfu_hits}");
    assert!(lfu_hits >= lru_hits, "on this trace LFU is at least as good as LRU");
    assert!(
        belady_hits(&trace, 3) >= lfu_hits,
        "s1d c1 a scan is what separates lru from least frequently used: expected `belady_hits(&trace, 3) >= lfu_hits`"
    );
}

#[test]
fn s1d_c1_belady_on_a_trace_worked_by_hand() {
    // capacity 2: 1 2 3 1 2 -> evict 2 at the miss of 3 (next use of 2 is further than 1's), then 1 hits, 2 misses
    assert_eq!(belady_hits(&[1, 2, 3, 1, 2], 2), 1, "s1d c1 belady on a trace worked by hand");
    // FIFO on the same trace: evicts 1 for 3, then 1 misses (evicting 2), then 2 misses: no hits at all
    assert_eq!(simulate(&[1, 2, 3, 1, 2], 2, Box::new(FifoReplacer::new())), 0, "s1d c1 belady on a trace worked by hand");
}

proptest! {
    #![proptest_config(config())]

    /// Property: no policy beats the optimum, and the optimum never gets worse with a bigger pool.
    #[test]
    fn s1d_c1_property_no_policy_beats_belady(trace in proptest::collection::vec(0u32..8, 0..80), capacity in 1usize..6) {
        let best = belady_hits(&trace, capacity);
        for (name, r) in [("lru", lru()), ("fifo", Box::new(FifoReplacer::new()) as Box<dyn FrameReplacer>), ("lfu", Box::new(LfuReplacer::new()))] {
            let hits = simulate(&trace, capacity, r);
            prop_assert!(hits <= best, "{} got {} hits, more than the optimum {}", name, hits, best);
        }
        prop_assert!(belady_hits(&trace, capacity + 1) >= best, "a bigger pool cannot do worse under the optimal policy");
        let distinct = trace.iter().collect::<std::collections::BTreeSet<_>>().len();
        prop_assert!(best <= trace.len().saturating_sub(distinct.min(capacity)), "at least the first use of {} pages must miss", distinct.min(capacity));
    }

    /// Property: the simulator against a one-line model of LRU.
    #[test]
    fn s1d_c1_property_simulating_lru_matches_a_model(trace in proptest::collection::vec(0u32..6, 0..80), capacity in 1usize..5) {
        let mut pool: Vec<u32> = Vec::new(); // most recent last
        let mut want = 0;
        for &p in &trace {
            if let Some(at) = pool.iter().position(|&x| x == p) {
                want += 1;
                pool.remove(at);
            } else if pool.len() == capacity {
                pool.remove(0);
            }
            pool.push(p);
        }
        prop_assert_eq!(simulate(&trace, capacity, lru()), want);
    }
}

#[test]
fn s1d_c2_a_page_with_fewer_than_k_accesses_has_no_distance() {
    let mut h = KHistory::new(3);
    assert_eq!(h.k_distance(10), None, "s1d c2 a page with fewer than k accesses has no distance");
    h.record(1);
    h.record(4);
    assert_eq!(h.k_distance(10), None, "two accesses of three");
}

#[test]
fn s1d_c2_the_distance_is_measured_to_the_kth_most_recent_access() {
    let mut h = KHistory::new(2);
    h.record(1);
    h.record(5);
    assert_eq!(h.k_distance(10), Some(9), "the 2nd most recent access is the one at time 1");
    h.record(9);
    assert_eq!(
        h.k_distance(10),
        Some(5),
        "now the 2nd most recent is the one at time 5: the access at time 1 is no longer among the last two"
    );
}

#[test]
fn s1d_c2_with_k_equal_to_one_it_is_plain_recency() {
    let mut h = KHistory::new(1);
    h.record(3);
    assert_eq!(h.k_distance(10), Some(7), "s1d c2 with k equal to one it is plain recency");
    h.record(8);
    assert_eq!(h.k_distance(10), Some(2), "s1d c2 with k equal to one it is plain recency");
}

#[test]
fn s1d_c2_only_the_last_k_accesses_are_remembered() {
    let mut h = KHistory::new(3);
    for t in 1..=100 {
        h.record(t);
    }
    assert_eq!(h.k_distance(100), Some(2), "the 3rd most recent access is at time 98");
}

#[test]
fn s1d_c2_a_longer_history_never_makes_the_distance_shorter() {
    let mut h = KHistory::new(2);
    h.record(10);
    h.record(20);
    let before = h.k_distance(30).unwrap();
    h.record(25);
    assert!(h.k_distance(30).unwrap() <= before, "a more recent access can only bring the k-th most recent access closer");
}

proptest! {
    #![proptest_config(config())]

    /// Property: against the definition, a list of every access time with the k-th from the end picked out.
    #[test]
    fn s1d_c2_property_k_distance_matches_the_definition(k in 1usize..5, gaps in proptest::collection::vec(0u64..5, 0..30), tail in 0u64..5) {
        let mut h = KHistory::new(k);
        let mut all = Vec::new();
        let mut t = 0u64;
        for g in gaps {
            t += g;
            h.record(t);
            all.push(t);
            let now = t + tail;
            let want = if all.len() < k { None } else { Some(now - all[all.len() - k]) };
            prop_assert_eq!(h.k_distance(now), want, "after {} accesses, k = {}", all.len(), k);
        }
    }
}
