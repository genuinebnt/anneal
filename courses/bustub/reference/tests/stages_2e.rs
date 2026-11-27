//! Tests for module 2e, the concurrent B+ tree. A test name starts with its stage: `s2e_02_…` belongs to stage 2e-02, and `anneal course
//! test` runs just those.
//!
//! What a tree latches is seen from outside in two ways. The traced pool (`tree.bpm`) counts the read and the write latches an operation
//! takes. And a test can hold read latches on pages itself, through the plain pool: a writer that reaches a page the test holds waits
//! there, and the test then asks whether something else (a lookup, the header page) is still free. Anything that could hang runs under a
//! timeout, so a deadlock fails with a message instead of stopping the run.

mod b_plus_tree_utils;
mod common;

use std::collections::BTreeSet;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use b_plus_tree_utils::*;
use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::PageId;
use bustub::storage::index::b_plus_tree::BPlusTree;
use bustub::storage::index::generic_key::GenericComparator;
use bustub::storage::page::page_guard::ReadPageGuard;
#[path = "common/pool.rs"]
mod pool;
use pool::{pool_with, Policy};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

fn new_pool(frames: usize) -> BufferPoolManager {
    pool_with(Policy::Fifo, frames).0
}

/// Runs `work` on a thread; fails the test if it does not finish in time (a deadlock must fail, not hang).
fn within<T: Send + 'static>(what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let _ = tx.send(work());
    });
    match rx.recv_timeout(Duration::from_secs(90)) {
        Ok(v) => v,
        // a worker that panicked (a failed assert, a todo!) ends early: show its message, not a deadlock
        Err(mpsc::RecvTimeoutError::Disconnected) => std::panic::resume_unwind(handle.join().unwrap_err()),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("timed out: {what} (a latch that is never released?)"),
    }
}

fn assert_nothing_pinned(bpm: &BufferPoolManager) {
    for id in 0..2500 {
        assert!(matches!(bpm.get_pin_count(PageId(id)), None | Some(0)), "page {id} is still pinned between operations: a guard was kept");
    }
}

fn filled<'a>(bpm: &'a BufferPoolManager, leaf_max: u32, internal_max: u32, keys: &[i64]) -> Tree<'a> {
    let tree = new_tree(bpm, leaf_max, internal_max);
    for &k in keys {
        insert(&tree, k);
    }
    tree
}

fn scan(it: impl Iterator<Item = (Key, bustub::common::rid::Rid)>) -> Vec<i64> {
    it.map(|(k, _)| k.get_as_integer()).collect()
}

/// A tree of two levels: the even keys 0 to 118 in leaves of at most 3 pairs under a root with room for 200 children, so the root is
/// safe for an insert and for a remove. Returns the tree and its header page.
fn two_levels<const T: usize>(bpm: &BufferPoolManager) -> (Tree<'_, T>, PageId) {
    let header = bpm.new_page();
    let tree: Tree<T> = BPlusTree::new("t", header, bpm, GenericComparator::<8>, 4, 200);
    for k in 0..60 {
        assert!(insert(&tree, 2 * k), "building the tree: insert {}", 2 * k);
    }
    assert_eq!(tree.depth(), 2, "a root over leaves");
    (tree, header)
}

/// Read-latches every page of the pool except the header and the root (through the plain pool, so the tree's counters do not move): a
/// writer that reaches a leaf waits there until the guards are dropped.
fn hold_every_leaf<'a, const T: usize>(bpm: &'a BufferPoolManager, tree: &Tree<T>, header: PageId) -> Vec<ReadPageGuard<'a>> {
    let root = tree.get_root_page_id();
    let fence = bpm.new_page();
    (0..fence.0).map(PageId).filter(|&id| id != header && id != root).map(|id| bpm.read_page(id)).collect()
}

/// Starts `write` on a thread; once it is waiting at a leaf the test holds, runs `probe` on another thread and says whether the probe
/// finished within three seconds. Then lets everything finish.
fn probe_finishes_while_a_writer_waits<const T: usize>(
    tree: &Tree<T>,
    held: Vec<ReadPageGuard<'_>>,
    write: impl FnOnce(&Tree<T>) + Send,
    probe: impl FnOnce(&Tree<T>) + Send,
) -> bool {
    let writes = tree.bpm.get_writes();
    thread::scope(|scope| {
        scope.spawn(|| write(tree));
        // the writer has asked for its first write latch; a moment later it is waiting at the leaf
        let start = Instant::now();
        while tree.bpm.get_writes() == writes && start.elapsed() < Duration::from_secs(5) {
            thread::yield_now();
        }
        thread::sleep(Duration::from_millis(150));
        let (tx, rx) = mpsc::channel();
        scope.spawn(move || {
            probe(tree);
            let _ = tx.send(());
        });
        let finished = rx.recv_timeout(Duration::from_secs(3)).is_ok();
        drop(held); // the writer goes on, and with it the probe
        finished
    })
}

// ---- 2e-01 · Latch crabbing ----------------------------------------------------------------------------------------------

#[test]
fn s2e_01_an_insert_below_a_safe_root_lets_go_of_the_header() {
    let bpm = new_pool(100);
    let (tree, header) = two_levels::<0>(&bpm);
    let held = hold_every_leaf(&bpm, &tree, header);
    let free = probe_finishes_while_a_writer_waits(
        &tree,
        held,
        |t| assert!(insert(t, 1), "insert 1"),
        |t| assert!(!t.is_empty(), "the tree is not empty"),
    );
    assert!(free, "an insert waiting at a leaf still held the header page: the root had room, so nothing above it could change");
    assert_eq!(get(&tree, 1), vec![rid_of(1)], "the insert finished once the leaf was free");
}

#[test]
fn s2e_01_a_remove_below_a_safe_root_lets_go_of_the_header() {
    let bpm = new_pool(100);
    let (tree, header) = two_levels::<0>(&bpm);
    let held = hold_every_leaf(&bpm, &tree, header);
    let free = probe_finishes_while_a_writer_waits(&tree, held, |t| remove(t, 0), |t| assert!(!t.is_empty(), "the tree is not empty"));
    assert!(free, "a remove waiting at a leaf still held the header page: the root had more than two children, so it could not shrink");
    assert!(get(&tree, 0).is_empty(), "the remove finished once the leaf was free");
}

#[test]
fn s2e_01_with_tombstones_a_remove_below_a_safe_root_lets_go_of_the_header_too() {
    let bpm = new_pool(100);
    let (tree, header) = two_levels::<2>(&bpm);
    let held = hold_every_leaf(&bpm, &tree, header);
    let free = probe_finishes_while_a_writer_waits(&tree, held, |t| remove(t, 4), |t| assert!(!t.is_empty(), "the tree is not empty"));
    assert!(free, "a logical delete waiting at a leaf still held the header page");
    assert!(get(&tree, 4).is_empty(), "the delete finished once the leaf was free");
}

#[test]
fn s2e_01_threads_inserting_disjoint_ranges_lose_nothing() {
    within("4 inserting threads", || {
        let bpm = new_pool(80);
        let tree = new_tree(&bpm, 3, 4);
        thread::scope(|scope| {
            for t in 0..4i64 {
                let tree = &tree;
                scope.spawn(move || {
                    for i in 0..250 {
                        assert!(insert(tree, i * 4 + t), "thread {t}: insert {}", i * 4 + t);
                    }
                });
            }
        });
        let live: BTreeSet<i64> = (0..1000).collect();
        check_shape(&tree, &live, 3, 4, true).unwrap_or_else(|e| panic!("{e}"));
        assert_nothing_pinned(&bpm);
    });
}

#[test]
fn s2e_01_threads_that_insert_remove_and_read_at_once_agree_with_their_models() {
    within("mixed threads", || {
        let bpm = new_pool(80);
        let tree = new_tree(&bpm, 3, 4);
        for k in 10_000..10_150 {
            assert!(insert(&tree, k), "threads that insert remove and read at once agree with their models: expected `insert(&tree, k)`");
            // stable keys the readers expect throughout
        }
        let models: Vec<BTreeSet<i64>> = thread::scope(|scope| {
            let writers: Vec<_> = (0..3i64)
                .map(|t| {
                    let tree = &tree;
                    scope.spawn(move || {
                        let mut model = BTreeSet::new();
                        let mut rng = Lcg(11 + t as u64);
                        for _ in 0..1200 {
                            let k = t * 1000 + rng.next(150) as i64;
                            if rng.next(2) == 0 {
                                assert_eq!(insert(tree, k), model.insert(k), "thread {t}: insert {k}");
                            } else {
                                remove(tree, k);
                                model.remove(&k);
                            }
                        }
                        model
                    })
                })
                .collect();
            for _ in 0..2 {
                let tree = &tree;
                scope.spawn(move || {
                    for round in 0..15 {
                        for k in 10_000..10_150 {
                            assert_eq!(get(tree, k).len(), 1, "reader, round {round}: the stable key {k} must always be found");
                        }
                    }
                });
            }
            writers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        let mut live: BTreeSet<i64> = (10_000..10_150).collect();
        for m in models {
            live.extend(m);
        }
        check_shape(&tree, &live, 3, 4, true).unwrap_or_else(|e| panic!("{e}"));
        assert_nothing_pinned(&bpm);
    });
}

// ---- 2e-02 · The optimistic fast path --------------------------------------------------------------------------------------

#[test]
fn s2e_02_an_insert_into_a_leaf_with_room_write_latches_only_that_leaf() {
    let bpm = new_pool(60);
    let tree = filled(&bpm, 4, 3, &(0..40).map(|i| i * 2).collect::<Vec<_>>());
    let firsts = leaf_first_keys(&tree);
    let roomy = tree.leaf_sizes().iter().position(|&s| (s as u32) + 1 < 4).expect("a leaf with room");
    let (r0, w0) = (tree.bpm.get_reads(), tree.bpm.get_writes());
    assert!(
        insert(&tree, firsts[roomy] + 1),
        "an insert into a leaf with room write latches only that leaf: expected `insert(&tree, firsts[roomy] + 1)`"
    );
    assert!(tree.bpm.get_reads() > r0, "the descent read-latches the path");
    assert_eq!(tree.bpm.get_writes() - w0, 1, "only the leaf is write-latched when it cannot split");
}

#[test]
fn s2e_02_a_remove_from_a_leaf_above_its_minimum_write_latches_only_that_leaf() {
    let bpm = new_pool(60);
    let tree = filled(&bpm, 4, 3, &(0..40).map(|i| i * 2).collect::<Vec<_>>());
    for k in [1, 3, 5] {
        insert(&tree, k); // fill some leaves above their minimum of 2
    }
    let firsts = leaf_first_keys(&tree);
    let fat = tree.leaf_sizes().iter().position(|&s| (s as u32) > 4 / 2).expect("a leaf above its minimum");
    let w0 = tree.bpm.get_writes();
    remove(&tree, firsts[fat]);
    assert_eq!(tree.bpm.get_writes() - w0, 1, "only the leaf is write-latched when it cannot underflow");
}

#[test]
fn s2e_02_an_insert_that_splits_holds_more_than_the_leaf() {
    let bpm = new_pool(60);
    let tree = filled(&bpm, 4, 3, &[1, 2, 3, 4, 5, 6, 7, 8]);
    let w0 = tree.bpm.get_writes();
    // the tree is in the middle of its life: some insert must split a leaf
    let mut splits = 0;
    for k in 100..110 {
        let before = tree.leaf_sizes().len();
        let w = tree.bpm.get_writes();
        insert(&tree, k);
        if tree.leaf_sizes().len() > before {
            assert!(tree.bpm.get_writes() - w >= 2, "a split changes the leaf and its parent: at least two write latches");
            splits += 1;
        }
    }
    assert!(
        splits > 0 && tree.bpm.get_writes() > w0,
        "an insert that splits holds more than the leaf: expected `splits > 0 && tree.bpm.get_writes() > w0`"
    );
}

#[test]
fn s2e_02_lookups_and_scans_take_no_write_latch() {
    let bpm = new_pool(60);
    let tree = filled(&bpm, 4, 3, &(0..60).collect::<Vec<_>>());
    let w0 = tree.bpm.get_writes();
    for k in 0..60 {
        get(&tree, k);
    }
    assert_eq!(keys_by_scan(&tree).len(), 60, "lookups and scans take no write latch");
    assert_eq!(tree.bpm.get_writes(), w0, "reads must not write-latch");
}

#[test]
fn s2e_02_with_tombstones_a_delete_that_fits_and_a_revival_write_latch_only_the_leaf() {
    let bpm = new_pool(30);
    let tree = new_tree_t::<2>(&bpm, 4, 10);
    for k in 0..6 {
        assert!(insert(&tree, k), "insert {k}");
    }
    let (reads, writes) = (tree.bpm.get_reads(), tree.bpm.get_writes());
    remove(&tree, 2);
    assert!(tree.bpm.get_reads() > reads, "the descent read-latches the path: expected `tree.bpm.get_reads() > reads`");
    assert_eq!(tree.bpm.get_writes() - writes, 1, "a delete that fits in the buffer write-latches only the leaf");
    let writes = tree.bpm.get_writes();
    assert!(insert(&tree, 2), "bringing a pair back is also a single-leaf write");
    assert_eq!(tree.bpm.get_writes() - writes, 1, "bringing a deleted pair back write-latches only the leaf");
}

#[test]
fn s2e_02_a_lookup_elsewhere_is_not_blocked_while_a_writer_waits_for_its_leaf() {
    let bpm = new_pool(100);
    let (tree, header) = two_levels::<0>(&bpm);
    let held = hold_every_leaf(&bpm, &tree, header);
    let free = probe_finishes_while_a_writer_waits(
        &tree,
        held,
        |t| assert!(insert(t, 1), "insert 1"),
        |t| assert_eq!(get(t, 100), vec![rid_of(100)], "key 100 is in another leaf"),
    );
    assert!(free, "a lookup in another leaf waited for an insert into a leaf with room: the insert write-latched more than its leaf");
}

// ---- 2e-03 · Scans that survive writers ------------------------------------------------------------------------------------

#[test]
fn s2e_03_a_split_under_an_open_scan_never_repeats_a_key() {
    let bpm = new_pool(30);
    let tree = filled(&bpm, 6, 200, &[1, 2, 3, 4, 5]);
    let mut it = tree.begin();
    let seen: Vec<i64> = scan(it.by_ref().take(4));
    assert_eq!(seen, vec![1, 2, 3, 4], "the scan starts in order");
    insert(&tree, 6); // the leaf reaches 6 pairs and splits: 4, 5 and 6 move to a new leaf to its right
    let rest = scan(it);
    assert!(rest.iter().all(|&k| k > 4 && k <= 6), "after the split the scan returned {rest:?}: 4 was returned already");
    assert!(rest.windows(2).all(|w| w[0] < w[1]), "and the rest is in order: {rest:?}");
}

#[test]
fn s2e_03_an_insert_before_the_position_never_makes_the_scan_repeat_a_key() {
    let bpm = new_pool(30);
    let tree = filled(&bpm, 8, 200, &[2, 4, 6]);
    let mut it = tree.begin();
    assert_eq!(scan(it.by_ref().take(2)), vec![2, 4], "the scan starts in order");
    insert(&tree, 3); // every pair from 3 on shifts one slot right, under the scan's position
    let rest = scan(it);
    assert!(
        rest.iter().all(|&k| k == 6),
        "after an insert before its position the scan returned {rest:?}: 3 is behind it and 4 was returned"
    );
}

#[test]
fn s2e_03_a_borrow_that_moves_a_returned_key_ahead_of_the_scan_never_repeats_it() {
    let bpm = new_pool(30);
    let tree = filled(&bpm, 4, 200, &[0, 1, 2, 3, 4, -1]);
    assert_eq!(tree.leaf_sizes(), vec![3, 3], "two leaves: [-1,0,1] and [2,3,4]");
    let mut it = tree.begin();
    assert_eq!(scan(it.by_ref().take(3)), vec![-1, 0, 1], "the scan has read the whole first leaf");
    remove(&tree, 2);
    remove(&tree, 3); // [4] is short and borrows 1 from its left neighbour: the next leaf now starts with a key the scan returned
    let rest = scan(it);
    assert!(rest.iter().all(|&k| k == 4), "after the borrow the scan returned {rest:?}: 1 was returned already and 2, 3 are gone");
}

#[test]
fn s2e_03_an_open_scan_survives_its_leaf_shrinking_or_merging_away() {
    // Removes under an open iterator shrink the leaf it is on, and merges free it. The scan must not read a slot or a page that is
    // gone: it finishes, and every key it returns is real and in order.
    let bpm = new_pool(80);
    let tree = new_tree(&bpm, 3, 4);
    for k in 0..60 {
        insert(&tree, k);
    }
    let mut it = tree.begin();
    let mut seen = vec![];
    for _ in 0..20 {
        seen.extend(it.next().map(|(k, _)| k.get_as_integer()));
    }
    for k in 15..45 {
        remove(&tree, k); // the iterator's leaf and its neighbours shrink and merge
    }
    seen.extend(it.map(|(k, _)| k.get_as_integer()));
    assert!(seen.iter().all(|k| (0..60).contains(k)), "the scan returns only keys that were inserted: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "in order, each at most once: {seen:?}");
    assert_eq!(keys_by_scan(&tree), (0..15).chain(45..60).collect::<Vec<_>>(), "a fresh scan afterwards sees exactly the keys left");
}

#[test]
fn s2e_03_scans_that_run_while_writers_work_return_real_keys_in_order_at_most_once() {
    within("scans and writers", || {
        let bpm = new_pool(80);
        let tree = new_tree(&bpm, 3, 4);
        let stable: BTreeSet<i64> = (0..600).step_by(3).collect();
        for &k in &stable {
            insert(&tree, k); // the stable keys: multiples of 3
        }
        thread::scope(|scope| {
            for t in 0..2i64 {
                let tree = &tree;
                scope.spawn(move || {
                    // insert 200 keys, then remove them again: leaves split while the scans run, then shrink and merge under them
                    for i in 0..400 {
                        let k = (i % 200) * 3 + 1 + t; // never a multiple of 3, and each writer has its own keys
                        if i < 200 {
                            insert(tree, k);
                        } else {
                            remove(tree, k);
                        }
                    }
                });
            }
            let tree = &tree;
            scope.spawn(move || {
                // A scan holds no latch between two pairs, so it may miss a key that moved, but never returns one twice or out of order
                for round in 0..20 {
                    let keys = keys_by_scan(tree);
                    assert!(keys.iter().all(|k| (0..600).contains(k)), "round {round}: a scan returned a key that was never inserted");
                    assert!(keys.windows(2).all(|w| w[0] < w[1]), "round {round}: a scan returned a key twice or out of order");
                }
            });
        });
        // Each writer removed every key it inserted, so only the stable keys are left, however they interleaved.
        check_shape(&tree, &stable, 3, 4, true).unwrap_or_else(|e| panic!("{e}"));
        assert_nothing_pinned(&bpm);
    });
}

// ---- 2e-04 · Boss --------------------------------------------------------------------------------------------------------

#[test]
fn s2e_04_threads_deleting_and_inserting_with_tombstones_keep_every_live_key() {
    within("six threads on a tree with tombstones", || {
        for _ in 0..5 {
            let bpm = new_pool(50);
            let tree = new_tree_t::<3>(&bpm, 4, 5);
            let preserved: Vec<i64> = (1..=400).filter(|k| k % 10 == 0).collect();
            let dynamic: Vec<i64> = (1..=400).filter(|k| k % 10 != 0).collect();
            for &k in &preserved {
                assert!(insert(&tree, k), "insert {k}");
            }
            thread::scope(|scope| {
                for tid in 0..6 {
                    let (tree, preserved, dynamic) = (&tree, &preserved, &dynamic);
                    scope.spawn(move || match tid % 3 {
                        0 => dynamic.iter().for_each(|&k| {
                            insert(tree, k);
                        }),
                        1 => dynamic.iter().for_each(|&k| remove(tree, k)),
                        _ => {
                            for &k in preserved {
                                assert_eq!(get(tree, k), vec![rid_of(k)], "preserved key {k} went missing");
                            }
                        }
                    });
                }
            });
            let scanned: BTreeSet<i64> = keys_by_scan(&tree).into_iter().collect();
            assert!(preserved.iter().all(|k| scanned.contains(k)), "a preserved key is missing from the scan");
            // whatever the dynamic keys ended as, the structure must be sound for the keys that are live
            check_shape_t::<3>(&tree, &scanned, 4, 5, true).unwrap_or_else(|e| panic!("{e}"));
            assert_nothing_pinned(&bpm);
        }
    });
}
