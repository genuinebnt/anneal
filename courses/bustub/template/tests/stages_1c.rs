//! Tests for module 1c, the simple replacers. A test name starts with its stage: `s1c_02_…` belongs to stage 1c-02, and
//! `anneal course test` runs just those.
//!
//! The tests use only the public items: `IndexList` and `Handle`, `LruReplacer`, `ClockReplacer` and the `Replacer` trait. They are
//! written against the trait, not the types: the same functions check any replacer. Each policy is compared with a **model**, a few
//! lines of plain code that says what the policy means, and every replacer must also keep the promises that hold for *all*
//! policies. A test that only your design passes would be a bug in the test, so the file also runs a deliberately different LRU
//! (a plain `Vec`, linear time) through the LRU properties.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use bustub::buffer::clock_replacer::ClockReplacer;
use bustub::buffer::lru_replacer::LruReplacer;
use bustub::buffer::replacer::Replacer;
use bustub::common::config::FrameId;
use bustub::common::index_list::{Handle, IndexList};
use proptest::prelude::*;

type Make<'a> = &'a dyn Fn(usize) -> Box<dyn Replacer>;

fn f(n: usize) -> FrameId {
    FrameId(n)
}

fn config() -> ProptestConfig {
    ProptestConfig { cases: 64, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

/// Runs `work` on a thread and fails the test if it takes longer than `limit`: a quadratic design must fail, not hang the run.
fn within<T: Send + 'static>(what: &str, limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit).unwrap_or_else(|_| panic!("{what} did not finish in {limit:?}: the cost must not grow with the number of elements"))
}

/// A tiny deterministic generator for the large runs.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

// ---- 1c-01 · A list you can pull from the middle --------------------------------------------------------------------

fn all<T: Clone>(list: &IndexList<T>) -> Vec<T> {
    list.iter().cloned().collect()
}

#[derive(Clone, Debug)]
enum ListOp {
    Push(i32),
    Pop,
    Remove(usize),
    MoveToBack(usize),
    Get(usize),
}

fn list_ops() -> impl Strategy<Value = Vec<ListOp>> {
    prop::collection::vec(
        prop_oneof![
            3 => any::<i32>().prop_map(ListOp::Push),
            1 => Just(ListOp::Pop),
            2 => (0..64usize).prop_map(ListOp::Remove),
            2 => (0..64usize).prop_map(ListOp::MoveToBack),
            1 => (0..64usize).prop_map(ListOp::Get),
        ],
        1..150,
    )
}

#[test]
fn s1c_01_a_new_list_is_empty() {
    let list: IndexList<u32> = IndexList::new();
    assert!(list.is_empty(), "a new list is empty");
    assert_eq!(list.len(), 0);
    assert_eq!(list.front(), None);
    assert_eq!(all(&list), Vec::<u32>::new());
    let mut list = list;
    assert_eq!(list.pop_front(), None, "popping an empty list gives nothing");
}

#[test]
fn s1c_01_push_back_keeps_order_and_each_handle_finds_its_element() {
    let mut list = IndexList::new();
    let hs: Vec<Handle> = ["a", "b", "c"].into_iter().map(|s| list.push_back(String::from(s))).collect();
    assert_eq!(list.len(), 3);
    assert_eq!(list.iter().map(|s| s.as_str()).collect::<Vec<_>>(), ["a", "b", "c"], "elements are ordered front to back in the order pushed (and need not be Clone)");
    assert_eq!(list.front().map(|s| s.as_str()), Some("a"));
    assert_eq!(hs.iter().map(|h| list.get(*h).unwrap().as_str()).collect::<Vec<_>>(), ["a", "b", "c"], "each handle finds the element it was returned for");
    assert!(hs[0] != hs[1] && hs[1] != hs[2] && hs[0] != hs[2], "handles for different elements differ");
}

#[test]
fn s1c_01_a_removed_handle_never_names_another_element_even_when_the_space_is_reused() {
    let mut list = IndexList::new();
    let a = list.push_back("a");
    assert_eq!(list.remove(a), Some("a"));
    assert_eq!(list.get(a), None, "a removed element is gone");
    let b = list.push_back("b"); // may well be stored where `a` was
    let c = list.push_back("c");
    assert_eq!(list.get(a), None, "the old handle must not find the new element that took its place");
    assert_eq!(list.remove(a), None, "removing through a stale handle does nothing");
    assert!(!list.move_to_back(a), "moving through a stale handle reports failure");
    assert_eq!(all(&list), ["b", "c"], "a stale handle must not disturb the list");
    assert_eq!((list.get(b), list.get(c)), (Some(&"b"), Some(&"c")), "live handles are unaffected");
}

#[test]
fn s1c_01_moving_to_the_back_keeps_the_handle_valid() {
    let mut list = IndexList::new();
    let hs: Vec<Handle> = (0..4).map(|i| list.push_back(i)).collect();
    assert!(list.move_to_back(hs[1]));
    assert_eq!(all(&list), [0, 2, 3, 1]);
    assert!(list.move_to_back(hs[1]), "moving the last element to the back succeeds and changes nothing");
    assert_eq!(all(&list), [0, 2, 3, 1]);
    assert!(list.move_to_back(hs[0]));
    assert_eq!(all(&list), [2, 3, 1, 0]);
    assert_eq!(hs.iter().map(|h| list.get(*h).copied()).collect::<Vec<_>>(), [Some(0), Some(1), Some(2), Some(3)], "every handle still names its own element");
    assert_eq!(list.pop_front(), Some(2));
}

proptest! {
    #![proptest_config(config())]

    /// Whatever happens to it, the list is the same sequence a `Vec` of (id, value) pairs would be, and a handle names its element
    /// until that element is gone.
    #[test]
    fn s1c_01_any_sequence_of_operations_agrees_with_a_vec_model(ops in list_ops()) {
        let mut list: IndexList<i32> = IndexList::new();
        let mut handles: Vec<Handle> = Vec::new();
        let mut value: Vec<i32> = Vec::new();
        let mut live: Vec<bool> = Vec::new();
        let mut order: Vec<usize> = Vec::new(); // ids, front to back
        for op in ops {
            match op {
                ListOp::Push(v) => {
                    handles.push(list.push_back(v));
                    value.push(v);
                    live.push(true);
                    order.push(handles.len() - 1);
                }
                ListOp::Pop => {
                    let got = list.pop_front();
                    if order.is_empty() {
                        prop_assert_eq!(got, None, "popping an empty list");
                    } else {
                        let id = order.remove(0);
                        live[id] = false;
                        prop_assert_eq!(got, Some(value[id]), "pop_front returns the first element");
                    }
                }
                ListOp::Remove(i) if !handles.is_empty() => {
                    let id = i % handles.len();
                    let got = list.remove(handles[id]);
                    prop_assert_eq!(got, live[id].then_some(value[id]), "remove returns the element, or None for a handle that is no longer valid");
                    if live[id] {
                        live[id] = false;
                        order.retain(|&x| x != id);
                    }
                }
                ListOp::MoveToBack(i) if !handles.is_empty() => {
                    let id = i % handles.len();
                    let got = list.move_to_back(handles[id]);
                    prop_assert_eq!(got, live[id], "move_to_back succeeds exactly for live handles");
                    if live[id] {
                        order.retain(|&x| x != id);
                        order.push(id);
                    }
                }
                ListOp::Get(i) if !handles.is_empty() => {
                    let id = i % handles.len();
                    prop_assert_eq!(list.get(handles[id]).copied(), live[id].then_some(value[id]), "get finds live elements and only those");
                }
                _ => {}
            }
            let expected: Vec<i32> = order.iter().map(|&id| value[id]).collect();
            prop_assert_eq!(all(&list), expected.clone(), "the elements, front to back");
            prop_assert_eq!(list.len(), expected.len(), "len");
            prop_assert_eq!(list.is_empty(), expected.is_empty(), "is_empty");
            prop_assert_eq!(list.front().copied(), expected.first().copied(), "front");
        }
    }
}

#[test]
fn s1c_01_removing_from_the_middle_by_handle_takes_constant_time() {
    // 500 000 removals in scrambled order. Searching for the element would take about 500 000 x 250 000 steps and miss the
    // limit by far; handles make each removal a few steps.
    within("removing 500 000 elements by handle", Duration::from_secs(10), || {
        let n = 500_000;
        let mut list = IndexList::new();
        let handles: Vec<Handle> = (0..n).map(|i| list.push_back(i)).collect();
        let mut order: Vec<usize> = (0..n).collect();
        let mut rng = Lcg(7);
        for i in (1..n).rev() {
            order.swap(i, rng.next(i + 1));
        }
        for &i in &order {
            assert_eq!(list.remove(handles[i]), Some(i));
        }
        assert!(list.is_empty());
        // and the space is reused, so churn does not grow the list for ever
        for round in 0..200_000 {
            let h = list.push_back(round);
            assert_eq!(list.remove(h), Some(round));
        }
    });
}

// ---- Replacer models -------------------------------------------------------------------------------------------------------

const FRAMES: usize = 10;

#[derive(Clone, Copy, Debug)]
enum ROp {
    Unpin(usize),
    Pin(usize),
    Victim,
}

fn replacer_ops() -> impl Strategy<Value = Vec<ROp>> {
    prop::collection::vec(
        prop_oneof![
            4 => (0..FRAMES).prop_map(ROp::Unpin),
            2 => (0..FRAMES).prop_map(ROp::Pin),
            2 => Just(ROp::Victim),
        ],
        1..200,
    )
}

/// What LRU means: the least recently unpinned frame goes first; unpinning a frame that is already there changes nothing.
#[derive(Default)]
struct LruModel {
    order: Vec<usize>,
}
impl LruModel {
    fn unpin(&mut self, frame: usize) {
        if !self.order.contains(&frame) {
            self.order.push(frame);
        }
    }
    fn pin(&mut self, frame: usize) {
        self.order.retain(|&x| x != frame);
    }
    fn victim(&mut self) -> Option<usize> {
        (!self.order.is_empty()).then(|| self.order.remove(0))
    }
}

/// What CLOCK means, written as a rotating queue: the front is the frame under the hand. A new frame joins at the back (just
/// behind the hand). A victim search takes the front: with its bit set it goes to the back with the bit cleared, otherwise it leaves.
#[derive(Default)]
struct ClockModel {
    ring: VecDeque<(usize, bool)>,
}
impl ClockModel {
    fn unpin(&mut self, frame: usize) {
        match self.ring.iter_mut().find(|(g, _)| *g == frame) {
            Some(entry) => entry.1 = true,
            None => self.ring.push_back((frame, true)),
        }
    }
    fn pin(&mut self, frame: usize) {
        self.ring.retain(|&(g, _)| g != frame);
    }
    fn victim(&mut self) -> Option<usize> {
        loop {
            let (frame, bit) = self.ring.pop_front()?;
            if bit {
                self.ring.push_back((frame, false));
            } else {
                return Some(frame);
            }
        }
    }
    fn size(&self) -> usize {
        self.ring.len()
    }
}

/// A deliberately different LRU for the sanity check: a plain `Vec` searched linearly.
struct NaiveLru {
    capacity: usize,
    frames: Vec<FrameId>,
}
impl Replacer for NaiveLru {
    fn victim(&mut self) -> Option<FrameId> {
        (!self.frames.is_empty()).then(|| self.frames.remove(0))
    }
    fn pin(&mut self, frame: FrameId) {
        self.frames.retain(|&g| g != frame);
    }
    fn unpin(&mut self, frame: FrameId) {
        if !self.frames.contains(&frame) {
            assert!(self.frames.len() < self.capacity, "the replacer is full");
            self.frames.push(frame);
        }
    }
    fn size(&self) -> usize {
        self.frames.len()
    }
}

fn lru(capacity: usize) -> Box<dyn Replacer> {
    Box::new(LruReplacer::new(capacity))
}
fn clock(capacity: usize) -> Box<dyn Replacer> {
    Box::new(ClockReplacer::new(capacity))
}

/// The policy is exactly the model's.
fn lru_agrees_with_the_model(make: Make, ops: &[ROp]) -> Result<(), TestCaseError> {
    let (mut r, mut m) = (make(FRAMES), LruModel::default());
    for (step, op) in ops.iter().enumerate() {
        match *op {
            ROp::Unpin(x) => {
                r.unpin(f(x));
                m.unpin(x);
            }
            ROp::Pin(x) => {
                r.pin(f(x));
                m.pin(x);
            }
            ROp::Victim => prop_assert_eq!(r.victim(), m.victim().map(f), "step {}: the victim must be the least recently used frame", step),
        }
        prop_assert_eq!(r.size(), m.order.len(), "step {}: size is the number of frames that may be evicted", step);
    }
    Ok(())
}

fn clock_agrees_with_the_model(make: Make, ops: &[ROp]) -> Result<(), TestCaseError> {
    let (mut r, mut m) = (make(FRAMES), ClockModel::default());
    for (step, op) in ops.iter().enumerate() {
        match *op {
            ROp::Unpin(x) => {
                r.unpin(f(x));
                m.unpin(x);
            }
            ROp::Pin(x) => {
                r.pin(f(x));
                m.pin(x);
            }
            ROp::Victim => prop_assert_eq!(r.victim(), m.victim().map(f), "step {}: the victim must be the frame the clock hand finds", step),
        }
        prop_assert_eq!(r.size(), m.size(), "step {}: size is the number of frames on the ring", step);
    }
    Ok(())
}

/// What every replacer promises whatever its policy: the set of evictable frames is what unpin and pin make it, `victim` takes one
/// of them out, and draining gives each exactly once.
fn satisfies_the_general_contract(make: Make, ops: &[ROp]) -> Result<(), TestCaseError> {
    let mut r = make(FRAMES);
    let mut evictable: BTreeSet<usize> = BTreeSet::new();
    for (step, op) in ops.iter().enumerate() {
        match *op {
            ROp::Unpin(x) => {
                r.unpin(f(x));
                evictable.insert(x);
            }
            ROp::Pin(x) => {
                r.pin(f(x));
                evictable.remove(&x);
            }
            ROp::Victim => match r.victim() {
                None => prop_assert!(evictable.is_empty(), "step {}: victim said nothing can be evicted, but {:?} can", step, evictable),
                Some(FrameId(v)) => prop_assert!(evictable.remove(&v), "step {}: victim returned frame {} which was not evictable (evictable: {:?})", step, v, evictable),
            },
        }
        prop_assert_eq!(r.size(), evictable.len(), "step {}: size is the number of evictable frames", step);
    }
    let mut drained = BTreeSet::new();
    while let Some(FrameId(v)) = r.victim() {
        prop_assert!(drained.insert(v), "frame {} was returned twice while draining", v);
    }
    prop_assert_eq!(drained, evictable, "draining must give every evictable frame exactly once");
    prop_assert_eq!(r.size(), 0, "a drained replacer is empty");
    Ok(())
}

// ---- 1c-02 · The LRU replacer ----------------------------------------------------------------------------------------------

#[test]
fn s1c_02_frames_leave_in_the_order_they_were_unpinned() {
    let mut r = LruReplacer::new(8);
    for x in [3, 1, 4, 2] {
        r.unpin(f(x));
    }
    assert_eq!(r.size(), 4);
    assert_eq!([r.victim(), r.victim(), r.victim(), r.victim(), r.victim()], [Some(f(3)), Some(f(1)), Some(f(4)), Some(f(2)), None], "least recently unpinned first, then nothing");
    assert_eq!(r.size(), 0);
}

#[test]
fn s1c_02_unpinning_a_frame_that_is_already_there_changes_nothing() {
    let mut r = LruReplacer::new(8);
    for x in [1, 2, 3, 1, 1] {
        r.unpin(f(x));
    }
    assert_eq!(r.size(), 3, "frame 1 counts once");
    assert_eq!(r.victim(), Some(f(1)), "a second unpin does not make frame 1 more recent");
}

#[test]
fn s1c_02_pinning_removes_a_frame_and_a_later_unpin_makes_it_the_most_recent() {
    let mut r = LruReplacer::new(8);
    for x in [1, 2, 3] {
        r.unpin(f(x));
    }
    r.pin(f(1));
    r.pin(f(9)); // never there: nothing happens
    assert_eq!(r.size(), 2);
    r.unpin(f(1));
    assert_eq!([r.victim(), r.victim(), r.victim()], [Some(f(2)), Some(f(3)), Some(f(1))], "frame 1 came back last, so it leaves last");
}

#[test]
fn s1c_02_a_replacer_given_more_frames_than_its_capacity_panics() {
    let result = std::panic::catch_unwind(|| {
        let mut r = LruReplacer::new(2);
        for x in 0..3 {
            r.unpin(f(x));
        }
    });
    assert!(result.is_err(), "unpinning a third distinct frame in a replacer of capacity 2 is a bug in the caller: it must panic");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1c_02_lru_agrees_with_the_model_for_any_operations(ops in replacer_ops()) {
        lru_agrees_with_the_model(&lru, &ops)?;
    }

    #[test]
    fn s1c_02_lru_satisfies_the_general_replacer_contract(ops in replacer_ops()) {
        satisfies_the_general_contract(&lru, &ops)?;
    }

    /// The tests are about behaviour: a different design passes the same LRU properties. (Not a stage test: it needs no code of yours.)
    #[test]
    fn sanity_a_naive_lru_passes_the_same_lru_properties(ops in replacer_ops()) {
        let naive = |c: usize| -> Box<dyn Replacer> { Box::new(NaiveLru { capacity: c, frames: Vec::new() }) };
        lru_agrees_with_the_model(&naive, &ops)?;
        satisfies_the_general_contract(&naive, &ops)?;
    }
}

#[test]
fn s1c_02_every_operation_stays_fast_with_four_hundred_thousand_frames() {
    // 3 000 000 operations on 400 000 frames. A design that searches for a frame costs about 400 000 steps per operation.
    within("3 000 000 operations on a replacer of 400 000 frames", Duration::from_secs(10), || {
        let n = 400_000;
        let mut r = LruReplacer::new(n);
        for x in 0..n {
            r.unpin(f(x));
        }
        assert_eq!(r.size(), n);
        let mut rng = Lcg(42);
        let start = Instant::now();
        for _ in 0..3_000_000 {
            let x = f(rng.next(n));
            match rng.next(3) {
                0 => r.pin(x),
                1 => r.unpin(x),
                _ => {
                    let _ = r.victim();
                }
            }
        }
        assert!(r.size() <= n);
        eprintln!("3 000 000 operations took {:?}", start.elapsed());
    });
}

// ---- 1c-03 · The CLOCK replacer --------------------------------------------------------------------------------------------

#[test]
fn s1c_03_a_frame_whose_bit_was_set_again_gets_a_second_chance() {
    let mut r = ClockReplacer::new(8);
    for x in [1, 2, 3] {
        r.unpin(f(x));
    }
    assert_eq!(r.victim(), Some(f(1)), "all bits are set, so the hand clears them all and comes back to the first frame");
    r.unpin(f(2)); // sets frame 2's bit again
    assert_eq!(r.victim(), Some(f(3)), "frame 2 has its bit set, so the hand passes it (clearing the bit) and takes frame 3");
    assert_eq!(r.victim(), Some(f(2)));
    assert_eq!(r.victim(), None);
}

#[test]
fn s1c_03_unpinning_a_frame_on_the_ring_does_not_change_its_place_or_the_size() {
    let mut r = ClockReplacer::new(8);
    for x in [1, 2, 3, 2, 2] {
        r.unpin(f(x));
    }
    assert_eq!(r.size(), 3);
    assert_eq!([r.victim(), r.victim(), r.victim()], [Some(f(1)), Some(f(2)), Some(f(3))]);
}

#[test]
fn s1c_03_pinning_a_frame_behind_the_hand_leaves_the_hand_on_the_same_frame() {
    let mut r = ClockReplacer::new(8);
    for x in [1, 2, 3] {
        r.unpin(f(x));
    }
    assert_eq!(r.victim(), Some(f(1))); // the hand now points at frame 2, whose bit is clear
    r.unpin(f(4)); // joins just behind the hand
    r.pin(f(4)); // and leaves again
    assert_eq!(r.victim(), Some(f(2)), "the hand still points at frame 2");
    assert_eq!(r.victim(), Some(f(3)));
}

#[test]
fn s1c_03_pinning_the_frame_under_the_hand_moves_the_hand_to_the_next() {
    let mut r = ClockReplacer::new(8);
    for x in [1, 2, 3] {
        r.unpin(f(x));
    }
    assert_eq!(r.victim(), Some(f(1))); // the hand points at frame 2
    r.pin(f(2));
    assert_eq!(r.victim(), Some(f(3)), "frame 2 left, so the hand is on frame 3");
    assert_eq!(r.victim(), None);
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1c_03_clock_agrees_with_the_model_for_any_operations(ops in replacer_ops()) {
        clock_agrees_with_the_model(&clock, &ops)?;
    }

    #[test]
    fn s1c_03_clock_satisfies_the_general_replacer_contract(ops in replacer_ops()) {
        satisfies_the_general_contract(&clock, &ops)?;
    }
}

// ---- 1c-04 · Boss ----------------------------------------------------------------------------------------------------------

#[test]
fn s1c_04_long_runs_on_many_frames_agree_with_the_models() {
    let n = 300;
    let mut rng = Lcg(2024);
    let (mut l, mut lm) = (LruReplacer::new(n), LruModel::default());
    let (mut c, mut cm) = (ClockReplacer::new(n), ClockModel::default());
    for step in 0..20_000 {
        let x = rng.next(n);
        match rng.next(5) {
            0 | 1 => {
                l.unpin(f(x));
                lm.unpin(x);
                c.unpin(f(x));
                cm.unpin(x);
            }
            2 => {
                l.pin(f(x));
                lm.pin(x);
                c.pin(f(x));
                cm.pin(x);
            }
            _ => {
                assert_eq!(l.victim(), lm.victim().map(f), "LRU victim at step {step}");
                assert_eq!(c.victim(), cm.victim().map(f), "CLOCK victim at step {step}");
            }
        }
        assert_eq!((l.size(), c.size()), (lm.order.len(), cm.size()), "sizes at step {step}");
    }
}

/// Replays a reference string through a pool of `frames` frames, using only the replacer to choose what to evict. Returns the hit
/// rate and the pages evicted, in order.
fn replay(mut r: Box<dyn Replacer>, frames: usize, accesses: &[usize]) -> (f64, Vec<usize>) {
    let mut page_in: HashMap<usize, usize> = HashMap::new(); // page -> frame
    let mut frame_holds: Vec<Option<usize>> = vec![None; frames];
    let mut free: Vec<usize> = (0..frames).rev().collect();
    let mut evicted = Vec::new();
    let mut hits = 0;
    for &page in accesses {
        if let Some(&frame) = page_in.get(&page) {
            hits += 1;
            r.pin(f(frame)); // in use ...
            r.unpin(f(frame)); // ... and done
        } else {
            let frame = free.pop().unwrap_or_else(|| r.victim().expect("a pool whose pages are all unpinned always has a victim").0);
            if let Some(old) = frame_holds[frame] {
                page_in.remove(&old);
                evicted.push(old);
            }
            frame_holds[frame] = Some(page);
            page_in.insert(page, frame);
            r.unpin(f(frame));
        }
    }
    (hits as f64 / accesses.len() as f64, evicted)
}

/// A buffer pool pins a frame when it uses a page and unpins it when done. Used like that, every access moves a frame to the
/// back of the ring with its bit set, and CLOCK behaves exactly as LRU does. The two only differ when frames are unpinned
/// without having been pinned in between.
#[test]
fn s1c_04_when_every_use_is_a_pin_and_an_unpin_clock_chooses_the_same_victims_as_lru() {
    // 60% of accesses go to 24 hot pages and 40% to 90 cold ones; the pool has 32 frames, so recency decides what stays.
    let mut rng = Lcg(99);
    let accesses: Vec<usize> = (0..40_000).map(|_| if rng.next(10) < 6 { rng.next(24) } else { 24 + rng.next(90) }).collect();
    let ((l, l_evicted), (c, c_evicted)) = (replay(lru(32), 32, &accesses), replay(clock(32), 32, &accesses));
    eprintln!("hit rate: LRU {l:.3}, CLOCK {c:.3}");
    assert!(l > 0.4, "the hot set should mostly stay in memory: hit rate {l:.3}");
    assert_eq!(c_evicted, l_evicted, "with pin-then-unpin on every use, CLOCK must evict the same pages as LRU, in the same order");
}

// ---- 1c-c1 and 1c-c2: challenges ------------------------------------------------------------------------------------------------------------

use bustub::buffer::fifo_replacer::FifoReplacer;
use bustub::buffer::lfu_replacer::LfuReplacer;
use bustub::buffer::replacer::FrameReplacer;
use bustub::common::config::PageId;

fn chpg() -> PageId {
    PageId(0)
}

#[derive(Clone, Debug)]
enum ChOp {
    Access(usize),
    SetEvictable(usize, bool),
    Evict,
    Remove(usize),
}

fn challenge_ops() -> impl Strategy<Value = Vec<ChOp>> {
    proptest::collection::vec(
        prop_oneof![
            5 => (0..6usize).prop_map(ChOp::Access),
            4 => (0..6usize, any::<bool>()).prop_map(|(f, e)| ChOp::SetEvictable(f, e)),
            3 => Just(ChOp::Evict),
            1 => (0..6usize).prop_map(ChOp::Remove),
        ],
        1..80,
    )
}

#[test]
fn s1c_c1_the_least_used_evictable_frame_goes_first() {
    let mut r = LfuReplacer::new();
    for fr in 0..3 {
        r.record_access(f(fr), chpg());
    }
    for _ in 0..2 {
        r.record_access(f(0), chpg());
    }
    r.record_access(f(1), chpg());
    for fr in 0..3 {
        r.set_evictable(f(fr), true);
    }
    // counts: frame 0 -> 3, frame 1 -> 2, frame 2 -> 1
    assert_eq!(r.evict(), Some(f(2)));
    assert_eq!(r.evict(), Some(f(1)));
    assert_eq!(r.evict(), Some(f(0)));
    assert_eq!(r.evict(), None);
}

#[test]
fn s1c_c1_among_equals_the_oldest_last_access_goes_first() {
    let mut r = LfuReplacer::new();
    r.record_access(f(1), chpg());
    r.record_access(f(2), chpg());
    r.record_access(f(3), chpg());
    for fr in 1..=3 {
        r.set_evictable(f(fr), true);
    }
    r.record_access(f(1), chpg()); // frame 1 now has 2 accesses, the others 1
    assert_eq!(r.evict(), Some(f(2)), "frames 2 and 3 are tied on one access; 2 was used first");
    assert_eq!(r.evict(), Some(f(3)));
    assert_eq!(r.evict(), Some(f(1)));
}

#[test]
fn s1c_c1_a_new_frame_is_not_evictable_until_it_is_set() {
    let mut r = LfuReplacer::new();
    r.record_access(f(5), chpg());
    assert_eq!((r.size(), r.evict()), (0, None));
    r.set_evictable(f(5), true);
    assert_eq!(r.size(), 1);
    r.set_evictable(f(5), false);
    assert_eq!((r.size(), r.evict()), (0, None), "pinned again");
    r.set_evictable(f(9), true);
    assert_eq!(r.size(), 0, "a frame the replacer has not met is ignored");
}

#[test]
fn s1c_c1_an_evicted_frame_starts_again_from_one_access() {
    let mut r = LfuReplacer::new();
    for _ in 0..5 {
        r.record_access(f(1), chpg());
    }
    r.record_access(f(2), chpg());
    r.record_access(f(2), chpg());
    r.set_evictable(f(1), true);
    r.set_evictable(f(2), true);
    assert_eq!(r.evict(), Some(f(2)));
    assert_eq!(r.evict(), Some(f(1)));
    r.record_access(f(1), chpg());
    r.record_access(f(2), chpg());
    r.record_access(f(2), chpg());
    r.set_evictable(f(1), true);
    r.set_evictable(f(2), true);
    assert_eq!(r.evict(), Some(f(1)), "frame 1 forgot its five accesses when it was evicted");
}

#[test]
fn s1c_c1_remove_forgets_an_evictable_frame_and_refuses_a_pinned_one() {
    let mut r = LfuReplacer::new();
    r.record_access(f(1), chpg());
    r.set_evictable(f(1), true);
    r.remove(f(1));
    assert_eq!(r.size(), 0);
    r.remove(f(7)); // unknown: nothing happens
    r.record_access(f(2), chpg());
    let pinned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| r.remove(f(2))));
    assert!(pinned.is_err(), "removing a frame that is in use is a bug of the caller and panics");
}

proptest! {
    #![proptest_config(config())]

    /// Property: against a model of the policy (fewest accesses, then oldest last access), for any sequence of calls.
    #[test]
    fn s1c_c1_property_lfu_matches_its_model(ops in challenge_ops()) {
        let mut r = LfuReplacer::new();
        let mut model: Vec<(usize, u64, u64, bool)> = Vec::new();
        let mut tick = 0u64;
        for op in ops {
            match op {
                ChOp::Access(fr) => {
                    tick += 1;
                    r.record_access(f(fr), chpg());
                    match model.iter_mut().find(|e| e.0 == fr) {
                        Some(e) => { e.1 += 1; e.2 = tick; }
                        None => model.push((fr, 1, tick, false)),
                    }
                }
                ChOp::SetEvictable(fr, e) => {
                    r.set_evictable(f(fr), e);
                    if let Some(x) = model.iter_mut().find(|x| x.0 == fr) { x.3 = e; }
                }
                ChOp::Evict => {
                    let want = model.iter().filter(|e| e.3).min_by_key(|e| (e.1, e.2)).map(|e| e.0);
                    prop_assert_eq!(r.evict(), want.map(f));
                    if let Some(w) = want { model.retain(|e| e.0 != w); }
                }
                ChOp::Remove(fr) => {
                    match model.iter().find(|e| e.0 == fr) {
                        Some(e) if !e.3 => continue,
                        _ => {}
                    }
                    r.remove(f(fr));
                    model.retain(|e| e.0 != fr);
                }
            }
            prop_assert_eq!(r.size(), model.iter().filter(|e| e.3).count());
        }
    }
}

#[test]
fn s1c_c2_the_oldest_arrival_goes_first_however_often_frames_are_used() {
    let mut r = FifoReplacer::new();
    for fr in 0..3 {
        r.record_access(f(fr), chpg());
        r.set_evictable(f(fr), true);
    }
    for _ in 0..10 {
        r.record_access(f(0), chpg());
    }
    assert_eq!(r.evict(), Some(f(0)), "using frame 0 again does not move it: this is first in, first out");
    assert_eq!(r.evict(), Some(f(1)));
}

#[test]
fn s1c_c2_a_pinned_frame_is_skipped_and_keeps_its_place() {
    let mut r = FifoReplacer::new();
    for fr in 0..3 {
        r.record_access(f(fr), chpg());
        r.set_evictable(f(fr), true);
    }
    r.set_evictable(f(0), false);
    assert_eq!(r.evict(), Some(f(1)), "frame 0 is in use");
    r.set_evictable(f(0), true);
    assert_eq!(r.evict(), Some(f(0)), "and it is still first in line once it may go");
}

#[test]
fn s1c_c2_size_counts_the_evictable_frames_through_every_change() {
    let mut r = FifoReplacer::new();
    r.record_access(f(1), chpg());
    r.record_access(f(2), chpg());
    assert_eq!(r.size(), 0);
    r.set_evictable(f(1), true);
    r.set_evictable(f(2), true);
    assert_eq!(r.size(), 2);
    r.set_evictable(f(1), false);
    assert_eq!(r.size(), 1, "a frame that is in use again is not counted");
    r.set_evictable(f(1), false);
    assert_eq!(r.size(), 1, "saying it twice changes nothing");
    r.set_evictable(f(2), true);
    assert_eq!(r.size(), 1, "nor does saying 'evictable' twice");
}

#[test]
fn s1c_c2_nothing_to_evict_is_none_and_unknown_frames_are_ignored() {
    let mut r = FifoReplacer::new();
    assert_eq!(r.evict(), None);
    r.set_evictable(f(4), true);
    assert_eq!((r.size(), r.evict()), (0, None));
    r.record_access(f(4), chpg());
    assert_eq!(r.evict(), None, "known but not evictable");
}

#[test]
fn s1c_c2_remove_forgets_a_frame_and_its_count() {
    let mut r = FifoReplacer::new();
    r.record_access(f(1), chpg());
    r.set_evictable(f(1), true);
    r.remove(f(1));
    assert_eq!((r.size(), r.evict()), (0, None));
}

proptest! {
    #![proptest_config(config())]

    /// Property: against a model written as plainly as possible, for any sequence of calls.
    #[test]
    fn s1c_c2_property_fifo_matches_its_model(ops in challenge_ops()) {
        let mut r = FifoReplacer::new();
        let mut model: Vec<(usize, bool)> = Vec::new(); // in arrival order
        for op in ops {
            match op {
                ChOp::Access(fr) => {
                    r.record_access(f(fr), chpg());
                    if !model.iter().any(|e| e.0 == fr) { model.push((fr, false)); }
                }
                ChOp::SetEvictable(fr, e) => {
                    r.set_evictable(f(fr), e);
                    if let Some(x) = model.iter_mut().find(|x| x.0 == fr) { x.1 = e; }
                }
                ChOp::Evict => {
                    let want = model.iter().find(|e| e.1).map(|e| e.0);
                    prop_assert_eq!(r.evict(), want.map(f));
                    if let Some(w) = want { model.retain(|e| e.0 != w); }
                }
                ChOp::Remove(fr) => {
                    match model.iter().find(|e| e.0 == fr) {
                        Some(e) if !e.1 => continue,
                        _ => {}
                    }
                    r.remove(f(fr));
                    model.retain(|e| e.0 != fr);
                }
            }
            prop_assert_eq!(r.size(), model.iter().filter(|e| e.1).count(), "size after {:?}", op);
        }
    }
}
