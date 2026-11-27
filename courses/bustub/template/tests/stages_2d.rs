//! Tests for module 2d, B+ tree tombstones. A test name starts with its stage: `s2d_02_…` belongs to stage 2d-02, and `anneal course test`
//! runs just those.
//!
//! A tree with `TOMBS > 0` deletes logically: a leaf keeps the pair where it is and remembers the key in a small buffer of tombstones.
//! The tests see the leaves only through the tree's observers (`leaf_keys`, `leaf_tombstones`, `leaf_sizes`, `depth`), written as text:
//! `[0,1][2,3~2,3][4,5~4]` is three leaves; the keys before a `~` are the pairs stored in the leaf, the keys after it its tombstones,
//! oldest first. Besides exact scenarios (the rules are in the stage page), random operations run against a `BTreeSet` and a checker
//! (`check_shape_t`) verifies from outside that every tombstone belongs to a pair in its leaf, no buffer overflows, leaf sizes and depth
//! are right, and a scan returns exactly the live keys.

mod b_plus_tree_utils;
mod common;

use std::collections::BTreeSet;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use b_plus_tree_utils::*;
use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::PageId;
#[path = "common/pool.rs"]
mod pool;
use pool::{pool_with, Policy};
use proptest::prelude::*;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

fn bpm(frames: usize) -> BufferPoolManager {
    pool_with(Policy::Fifo, frames).0
}

fn assert_no_pins(bpm: &BufferPoolManager) {
    for id in 0..2500 {
        assert!(matches!(bpm.get_pin_count(PageId(id)), None | Some(0)), "page {id} is still pinned: a guard was kept");
    }
}

fn insert_all<const T: usize>(tree: &Tree<T>, keys: impl IntoIterator<Item = i64>) {
    for k in keys {
        assert!(insert(tree, k), "insert {k}");
    }
}

// ---- 2d-01 · Deleting without moving anything ----------------------------------------------------------------------------

#[test]
fn s2d_01_remove_buffers_a_tombstone_instead_of_shifting_pairs() {
    let bpm = bpm(30);
    let tree = new_tree_t::<2>(&bpm, 4, 10);
    insert_all(&tree, 0..6);
    assert_eq!(leaves_t(&tree), "[0,1][2,3][4,5]");
    remove(&tree, 2);
    assert_eq!(leaves_t(&tree), "[0,1][2,3~2][4,5]", "the pair stays in its leaf; its key goes in the buffer");
    remove(&tree, 3);
    assert_eq!(leaves_t(&tree), "[0,1][2,3~2,3][4,5]", "tombstones keep the order they were added in");
    remove(&tree, 4);
    assert_eq!(leaves_t(&tree), "[0,1][2,3~2,3][4,5~4]");
    for k in [2, 3, 4, 9, -1] {
        remove(&tree, k); // already deleted, or not there at all
    }
    assert_eq!(leaves_t(&tree), "[0,1][2,3~2,3][4,5~4]", "removing a deleted or a missing key changes nothing");
    assert_no_pins(&bpm);
}

#[test]
fn s2d_01_deleted_pairs_are_not_found_and_not_scanned() {
    let bpm = bpm(30);
    let tree = new_tree_t::<2>(&bpm, 4, 10);
    insert_all(&tree, 0..8);
    for k in [1, 5] {
        remove(&tree, k);
    }
    for k in 0..8 {
        assert_eq!(get(&tree, k).len(), usize::from(k != 1 && k != 5), "key {k}");
    }
    assert_eq!(keys_by_scan(&tree), vec![0, 2, 3, 4, 6, 7]);
    let from = |k: i64| tree.begin_at(&index_key(k)).map(|(k, _)| k.get_as_integer()).collect::<Vec<_>>();
    assert_eq!(from(1), vec![2, 3, 4, 6, 7], "a scan starting at a deleted key starts at the next live one");
    assert_eq!(from(5), vec![6, 7]);
    assert_eq!(from(7), vec![7]);
    assert!(tree.begin_at(&index_key(8)).is_end());
}

#[test]
fn s2d_01_a_full_buffer_makes_room_by_really_removing_the_oldest() {
    let bpm = bpm(30);
    let tree = new_tree_t::<2>(&bpm, 6, 10);
    insert_all(&tree, 0..5);
    assert_eq!(leaves_t(&tree), "[0,1,2,3,4]");
    remove(&tree, 1);
    remove(&tree, 2);
    remove(&tree, 3); // the buffer is full: 1 goes for real, 3 is the newest tombstone
    assert_eq!(leaves_t(&tree), "[0,2,3,4~2,3]");
    assert!(get(&tree, 1).is_empty());
    assert_eq!(keys_by_scan(&tree), vec![0, 4]);
}

#[test]
fn s2d_01_inserting_a_deleted_key_again_brings_it_back_with_the_new_value() {
    let bpm = bpm(30);
    let tree = new_tree_t::<3>(&bpm, 5, 10);
    insert_all(&tree, 0..4);
    remove(&tree, 2);
    assert!(tree.insert(&index_key(2), &rid_of(2000)), "a deleted key can be inserted again");
    assert_eq!(leaves_t(&tree), "[0,1,2,3]", "it takes its old slot and its tombstone goes");
    assert_eq!(get(&tree, 2), vec![rid_of(2000)]);
    assert!(!tree.insert(&index_key(2), &rid_of(1)), "now it is live again: a duplicate");
}

#[test]
fn s2d_01_a_tree_with_every_pair_deleted_is_empty_to_a_scan_but_still_has_its_page() {
    let bpm = bpm(30);
    let tree = new_tree_t::<4>(&bpm, 6, 10);
    insert_all(&tree, 0..3);
    for k in 0..3 {
        remove(&tree, k);
    }
    assert!(tree.begin().is_end());
    assert!(get(&tree, 1).is_empty());
    assert!(tree.get_root_page_id().is_valid(), "the leaf is still there, holding three tombstones");
    assert_eq!(leaves_t(&tree), "[0,1,2~0,1,2]");
    assert!(insert(&tree, 1));
    assert_eq!(keys_by_scan(&tree), vec![1]);
}

#[test]
fn s2d_01_a_delete_that_fits_still_write_latches_only_the_leaf() {
    let bpm = bpm(30);
    let tree = new_tree_t::<2>(&bpm, 4, 10);
    insert_all(&tree, 0..6);
    let (reads, writes) = (tree.bpm.get_reads(), tree.bpm.get_writes());
    remove(&tree, 2);
    assert!(tree.bpm.get_reads() > reads);
    assert_eq!(tree.bpm.get_writes() - writes, 1);
    let writes = tree.bpm.get_writes();
    assert!(insert(&tree, 2), "bringing a pair back is also a single-leaf write");
    assert_eq!(tree.bpm.get_writes() - writes, 1);
}

#[test]
fn s2d_01_a_tree_without_tombstones_has_none_to_report() {
    let bpm = bpm(30);
    let tree = new_tree(&bpm, 4, 4);
    insert_all(&tree, 0..10);
    remove(&tree, 3);
    assert!(tree.leaf_tombstones().iter().all(|t| t.is_empty()), "with TOMBS = 0 a delete is physical");
    assert_eq!(tree.leaf_keys().concat().len(), 9);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    /// On one big leaf (nothing splits) any operations agree with a set, and a leaf holds at most `T` tombstones.
    #[test]
    fn s2d_01_logical_deletes_agree_with_a_set(t in 1usize..4, ops in prop::collection::vec((0i64..30, any::<bool>()), 1..120)) {
        fn run<const T: usize>(ops: &[(i64, bool)]) -> Result<(), String> {
            let bpm = bpm(10);
            let tree = new_tree_t::<T>(&bpm, 60, 4);
            let mut live = BTreeSet::new();
            for &(k, is_insert) in ops {
                if is_insert {
                    if insert(&tree, k) != live.insert(k) { return Err(format!("insert({k}) answered wrongly")); }
                } else {
                    remove(&tree, k);
                    live.remove(&k);
                }
                check_shape_t::<T>(&tree, &live, 60, 4, true)?;
            }
            Ok(())
        }
        let r = match t { 1 => run::<1>(&ops), 2 => run::<2>(&ops), _ => run::<3>(&ops) };
        prop_assert!(r.is_ok(), "{:?}", r);
    }
}

// ---- 2d-02 · Tombstones through splits, borrows and merges ---------------------------------------------------------------

#[test]
fn s2d_02_a_split_sends_each_tombstone_with_its_pair() {
    let bpm = bpm(30);
    let tree = new_tree_t::<3>(&bpm, 5, 4);
    insert_all(&tree, 0..4);
    for k in [3, 2, 0] {
        remove(&tree, k);
    }
    assert_eq!(leaves_t(&tree), "[0,1,2,3~3,2,0]");
    insert(&tree, 4); // the leaf reaches max_size 5: split
    assert_eq!(leaves_t(&tree), "[0,1,2~2,0][3,4~3]", "the buffer is divided by key; the order is kept");
}

#[test]
fn s2d_02_a_short_leaf_purges_its_own_tombstones_and_merges() {
    // BusTub's TombstoneBorrowTest scenario: the left leaf is at min_size, so the third remove makes it short
    let bpm = bpm(30);
    let tree = new_tree_t::<1>(&bpm, 4, 4);
    insert_all(&tree, 0..5);
    assert_eq!(leaves_t(&tree), "[0,1][2,3,4]");
    remove(&tree, 2); // the right leaf buffers 2
    remove(&tree, 1); // the left leaf buffers 1
    assert_eq!(leaves_t(&tree), "[0,1~1][2,3,4~2]");
    remove(&tree, 0); // the buffer is full: 1 goes for real; the leaf (now [0], below min 2) purges 0 as well, then must merge
    assert_eq!(leaves_t(&tree), "[2,3,4~2]", "one leaf: the right leaf's tombstone survived, the short leaf's did not");
    assert_eq!(keys_by_scan(&tree), vec![3, 4]);
}

#[test]
fn s2d_02_a_merge_keeps_the_tombstones_of_the_page_that_stays() {
    // BusTub's TombstoneCoalesceTest scenario
    let bpm = bpm(30);
    let tree = new_tree_t::<2>(&bpm, 6, 6);
    insert_all(&tree, 0..7);
    assert_eq!(leaves_t(&tree), "[0,1,2][3,4,5,6]");
    for k in [3, 0, 4, 1, 5, 2] {
        remove(&tree, k);
    }
    assert_eq!(leaves_t(&tree), "[4,5,6~4,5]");
    assert_eq!(keys_by_scan(&tree), vec![6]);
}

#[test]
fn s2d_02_a_leaf_that_borrows_a_deleted_pair_takes_its_tombstone_along() {
    let bpm = bpm(30);
    let tree = new_tree_t::<1>(&bpm, 6, 10);
    insert_all(&tree, 0..7);
    assert_eq!(leaves_t(&tree), "[0,1,2][3,4,5,6]");
    remove(&tree, 3); // the right leaf buffers 3: a deleted pair at its front
    remove(&tree, 1); // the left leaf, at min_size 3, buffers 1
    assert_eq!(leaves_t(&tree), "[0,1,2~1][3,4,5,6~3]");
    remove(&tree, 0); // the buffer is full: 1 goes for real; [0,2] is short, purges 0 too, and borrows 3 (deleted) and its tombstone
    assert_eq!(leaves_t(&tree), "[2,3,4,5,6~3]", "the left leaf is still short, the right has nothing more to spare: they merge");
    assert_eq!(keys_by_scan(&tree), vec![2, 4, 5, 6]);
}

#[test]
fn s2d_02_deletes_that_the_buffers_can_hold_never_change_the_shape_of_the_tree() {
    let bpm = bpm(60);
    let tree = new_tree_t::<2>(&bpm, 4, 3);
    insert_all(&tree, 0..100); // leaves of two pairs each: a leaf can never be asked for a third tombstone
    let (depth, leaves) = (tree.depth(), tree.leaf_sizes());
    for k in 0..99 {
        remove(&tree, k);
    }
    assert_eq!(keys_by_scan(&tree), vec![99]);
    assert_eq!((tree.depth(), tree.leaf_sizes()), (depth, leaves), "every delete fitted in a buffer, so no pair moved and no page merged");
    assert_eq!(tree.leaf_tombstones().concat().len(), 99);
    assert_no_pins(&bpm);
}

#[test]
fn s2d_02_once_the_buffers_overflow_the_dead_pairs_go_for_real_and_leaves_merge_away() {
    let bpm = bpm(60);
    let tree = new_tree_t::<1>(&bpm, 6, 3);
    insert_all(&tree, 0..100);
    let leaves = tree.leaf_sizes().len();
    for k in 0..99 {
        remove(&tree, k);
    }
    assert_eq!(keys_by_scan(&tree), vec![99]);
    assert!(tree.leaf_sizes().len() < leaves / 3, "one tombstone per leaf cannot hold 99 deletes: {} of {leaves} leaves are left", tree.leaf_sizes().len());
    assert!(tree.leaf_tombstones().concat().len() <= tree.leaf_sizes().len());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() })]

    /// Trees of random shape and buffer size, random inserts and removes: after every step the structure rules of `check_shape_t` hold.
    #[test]
    fn s2d_02_random_operations_keep_every_rule_for_every_buffer_size(t in 1usize..4, leaf_max in 2u32..8, internal_max in 3u32..6, ops in prop::collection::vec((-40i64..40, any::<bool>()), 1..140)) {
        fn run<const T: usize>(leaf_max: u32, internal_max: u32, ops: &[(i64, bool)]) -> Result<(), String> {
            let bpm = bpm(60);
            let tree = new_tree_t::<T>(&bpm, leaf_max, internal_max);
            let mut live = BTreeSet::new();
            for (i, &(k, is_insert)) in ops.iter().enumerate() {
                if is_insert {
                    if insert(&tree, k) != live.insert(k) { return Err(format!("step {i}: insert({k}) answered wrongly")); }
                } else {
                    remove(&tree, k);
                    live.remove(&k);
                }
                check_shape_t::<T>(&tree, &live, leaf_max, internal_max, true).map_err(|e| format!("step {i}, {}({k}), T={T} leaf {leaf_max} internal {internal_max}: {e}\n{}", if is_insert { "insert" } else { "remove" }, leaves_t(&tree)))?;
            }
            assert_no_pins(&bpm);
            Ok(())
        }
        let r = match t { 1 => run::<1>(leaf_max, internal_max, &ops), 2 => run::<2>(leaf_max, internal_max, &ops), _ => run::<3>(leaf_max, internal_max, &ops) };
        prop_assert!(r.is_ok(), "{:?}", r);
    }
}

// ---- 2d-03 · Boss ----------------------------------------------------------------------------------------------------------

#[test]
fn s2d_03_threads_deleting_and_inserting_keep_every_live_key() {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for _ in 0..5 {
            let bpm = bpm(50);
            let tree = new_tree_t::<3>(&bpm, 4, 5);
            let preserved: Vec<i64> = (1..=400).filter(|k| k % 10 == 0).collect();
            let dynamic: Vec<i64> = (1..=400).filter(|k| k % 10 != 0).collect();
            insert_all(&tree, preserved.iter().copied());
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
            assert_no_pins(&bpm);
        }
        tx.send(()).unwrap();
    });
    match rx.recv_timeout(Duration::from_secs(90)) {
        Ok(()) => handle.join().unwrap(),
        // a worker that panicked (a failed assert, a todo!) ends the scope early: show its message, not a deadlock
        Err(mpsc::RecvTimeoutError::Disconnected) => std::panic::resume_unwind(handle.join().unwrap_err()),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("the workload did not finish in 90 s: a deadlock?"),
    }
}

#[test]
fn s2d_03_long_runs_for_every_buffer_size_agree_with_a_set() {
    fn run<const T: usize>(leaf: u32, internal: u32, seed: u64) {
        let bpm = bpm(60);
        let tree = new_tree_t::<T>(&bpm, leaf, internal);
        let mut live = BTreeSet::new();
        let mut rng = Lcg(seed);
        for step in 0..1500 {
            let key = rng.next(80) as i64;
            if rng.next(5) < 3 {
                assert_eq!(insert(&tree, key), live.insert(key), "T={T} ({leaf},{internal}) seed {seed} step {step}: insert {key}");
            } else {
                live.remove(&key);
                remove(&tree, key);
            }
            if step % 25 == 0 {
                check_shape_t::<T>(&tree, &live, leaf, internal, true).unwrap_or_else(|e| panic!("T={T} ({leaf},{internal}) seed {seed} step {step}: {e}\n{}", leaves_t(&tree)));
            }
        }
    }
    for (leaf, internal) in [(2, 3), (3, 3), (4, 4), (5, 4), (6, 7)] {
        for seed in 0..3 {
            run::<1>(leaf, internal, seed);
            run::<2>(leaf, internal, seed + 10);
            run::<3>(leaf, internal, seed + 20);
        }
    }
}

// ---- 2d-c1: challenge ----------------------------------------------------------------------------------------------------------------------

use bustub::storage::index::live_iter::{LiveKeys, Slot};

fn ch_leaves(spec: &[&[Option<i64>]]) -> Vec<Vec<Slot>> {
    spec.iter().map(|l| l.iter().map(|s| s.map_or(Slot::Tombstone, Slot::Live)).collect()).collect()
}

#[test]
fn s2d_c1_live_keys_come_out_in_order_and_tombstones_are_skipped() {
    let leaves = ch_leaves(&[&[Some(1), None, Some(3)], &[Some(4), Some(5), None]]);
    assert_eq!(LiveKeys::new(&leaves).collect::<Vec<_>>(), vec![1, 3, 4, 5]);
}

#[test]
fn s2d_c1_a_leaf_of_only_tombstones_does_not_end_the_walk() {
    let leaves = ch_leaves(&[&[Some(1)], &[None, None, None], &[Some(9)]]);
    assert_eq!(LiveKeys::new(&leaves).collect::<Vec<_>>(), vec![1, 9], "the dead leaf is in the middle: keys after it are still live");
}

#[test]
fn s2d_c1_several_dead_leaves_in_a_row_and_dead_leaves_at_the_ends() {
    let leaves = ch_leaves(&[&[None], &[None, None], &[Some(5)], &[None], &[None], &[Some(6), None], &[None]]);
    assert_eq!(LiveKeys::new(&leaves).collect::<Vec<_>>(), vec![5, 6]);
}

#[test]
fn s2d_c1_empty_input_and_empty_leaves() {
    assert_eq!(LiveKeys::new(&[]).count(), 0);
    let leaves = ch_leaves(&[&[], &[Some(2)], &[]]);
    assert_eq!(LiveKeys::new(&leaves).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn s2d_c1_the_iterator_stays_finished_once_it_is_finished() {
    let leaves = ch_leaves(&[&[Some(1)]]);
    let mut it = LiveKeys::new(&leaves);
    assert_eq!(it.next(), Some(1));
    assert_eq!((it.next(), it.next()), (None, None));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: the iterator yields exactly the live keys, in order, whatever pattern of dead slots and dead leaves.
    #[test]
    fn s2d_c1_property_live_keys_match_a_flat_filter(shape in proptest::collection::vec(proptest::collection::vec(proptest::option::weighted(0.4, 0i64..100), 0..6), 0..8)) {
        let leaves: Vec<Vec<Slot>> = shape.iter().map(|l| l.iter().map(|s| s.map_or(Slot::Tombstone, Slot::Live)).collect()).collect();
        let want: Vec<i64> = shape.iter().flatten().flatten().copied().collect();
        prop_assert_eq!(LiveKeys::new(&leaves).collect::<Vec<_>>(), want);
    }
}

// @@ challenge 2d-c2 begin
mod ch_2d_c2 {
    use proptest::prelude::*;

    use bustub::storage::index::tombstone_leaf::TombstoneLeaf;
    use std::collections::BTreeMap;

    #[test]
    fn s2d_c2_removing_leaves_a_tombstone_and_inserting_revives_it_in_place() {
        let mut l = TombstoneLeaf::new();
        for k in [3, 1, 2] {
            assert_eq!(l.insert(k, k as u64 * 10), None);
        }
        assert_eq!(l.remove(2), Some(20));
        assert_eq!((l.slots(), l.live_len(), l.tombstones()), (3, 2, 1));
        assert_eq!(l.get(2), None);
        assert_eq!(l.insert(2, 99), None, "the key was dead, so there is no old value");
        assert_eq!((l.slots(), l.live_len(), l.tombstones()), (3, 3, 0), "revived in its own slot, no new slot");
        assert_eq!(l.get(2), Some(99));
        assert_eq!(l.keys(), vec![1, 2, 3]);
    }

    #[test]
    fn s2d_c2_inserting_a_live_key_replaces_and_returns_the_old_value() {
        let mut l = TombstoneLeaf::new();
        l.insert(5, 1);
        assert_eq!(l.insert(5, 2), Some(1));
        assert_eq!((l.get(5), l.slots()), (Some(2), 1));
    }

    #[test]
    fn s2d_c2_removing_a_missing_or_dead_key_says_none() {
        let mut l = TombstoneLeaf::new();
        l.insert(1, 1);
        assert_eq!(l.remove(9), None);
        assert_eq!(l.remove(1), Some(1));
        assert_eq!(l.remove(1), None);
    }

    #[test]
    fn s2d_c2_compaction_rule_and_effect() {
        let mut l = TombstoneLeaf::new();
        for k in 0..6 {
            l.insert(k, k as u64);
        }
        for k in 0..3 {
            l.remove(k);
        }
        assert!(!l.should_compact(), "3 of 6 is exactly half, not more than half");
        l.remove(3);
        assert!(l.should_compact());
        l.compact();
        assert_eq!((l.slots(), l.tombstones(), l.should_compact()), (2, 0, false));
        assert_eq!(l.keys(), vec![4, 5]);
        assert_eq!(l.get(4), Some(4));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a `BTreeMap`, with a key never occupying two slots, and compaction changing no answer.
        #[test]
        fn s2d_c2_property_a_leaf_matches_a_map(ops in proptest::collection::vec((0u8..4, 0i64..8, any::<u64>()), 0..80)) {
            let mut l = TombstoneLeaf::new();
            let mut m: BTreeMap<i64, u64> = BTreeMap::new();
            let mut ever: std::collections::BTreeSet<i64> = Default::default();
            for (op, k, v) in ops {
                match op {
                    0 | 1 => { prop_assert_eq!(l.insert(k, v), m.insert(k, v)); ever.insert(k); }
                    2 => prop_assert_eq!(l.remove(k), m.remove(&k)),
                    _ => { l.compact(); ever = m.keys().copied().collect(); }
                }
                prop_assert_eq!(l.live_len(), m.len());
                prop_assert_eq!(l.slots(), ever.len(), "a key must never occupy two slots");
                prop_assert_eq!(l.keys(), m.keys().copied().collect::<Vec<_>>());
                for key in 0..8 { prop_assert_eq!(l.get(key), m.get(&key).copied()); }
            }
        }
    }
}
// @@ challenge 2d-c2 end

// @@ challenge 2d-c3 begin
mod ch_2d_c3 {
    use proptest::prelude::*;

    use bustub::storage::index::live_counts::LiveCounts;

    #[test]
    fn s2d_c3_prefix_sums_and_ranges() {
        let c = LiveCounts::new(&[2, 0, 3, 1]);
        assert_eq!((c.prefix(0), c.prefix(1), c.prefix(2), c.prefix(3), c.prefix(4)), (0, 2, 2, 5, 6));
        assert_eq!((c.range(1, 3), c.range(2, 2), c.range(3, 1), c.total()), (3, 0, 0, 6));
        assert_eq!(c.prefix(100), 6, "past the end is the total");
    }

    #[test]
    fn s2d_c3_find_the_leaf_of_the_kth_live_key_skipping_empty_leaves() {
        let c = LiveCounts::new(&[2, 0, 3]);
        assert_eq!(c.find(0), Some((0, 0)));
        assert_eq!(c.find(1), Some((0, 1)));
        assert_eq!(c.find(2), Some((2, 0)), "leaf 1 is empty and is skipped");
        assert_eq!(c.find(4), Some((2, 2)));
        assert_eq!(c.find(5), None);
    }

    #[test]
    fn s2d_c3_updates_change_later_prefixes_only() {
        let mut c = LiveCounts::new(&[2, 0, 3]);
        c.add(1, 2);
        assert_eq!((c.prefix(1), c.prefix(2), c.prefix(3)), (2, 4, 7));
        assert_eq!(c.find(2), Some((1, 0)));
        c.add(0, -5);
        assert_eq!(c.count(0), 0, "a count never goes below zero");
        assert_eq!(c.total(), 5);
    }

    #[test]
    fn s2d_c3_no_leaves_and_all_empty_leaves() {
        let c = LiveCounts::new(&[]);
        assert_eq!((c.total(), c.find(0), c.leaves()), (0, None, 0));
        let c = LiveCounts::new(&[0, 0, 0]);
        assert_eq!(c.find(0), None);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against sums recomputed from scratch, after any updates; `find` is the inverse of `prefix`.
        #[test]
        fn s2d_c3_property_counts_match_recomputed_sums(init in proptest::collection::vec(0usize..5, 0..12), ops in proptest::collection::vec((0usize..12, -4i64..5), 0..40)) {
            let mut c = LiveCounts::new(&init);
            let mut m: Vec<i64> = init.iter().map(|&x| x as i64).collect();
            for (i, d) in ops {
                if m.is_empty() { break; }
                let i = i % m.len();
                c.add(i, d);
                m[i] = (m[i] + d).max(0);
                for j in 0..=m.len() {
                    prop_assert_eq!(c.prefix(j) as i64, m[..j].iter().sum::<i64>());
                }
                let total: i64 = m.iter().sum();
                for k in 0..total as usize + 1 {
                    let want = { let mut rem = k as i64; m.iter().position(|&n| { if rem < n { true } else { rem -= n; false } }).map(|leaf| (leaf, (k as i64 - m[..leaf].iter().sum::<i64>()) as usize)) };
                    prop_assert_eq!(c.find(k), want, "find({})", k);
                }
            }
        }
    }
}
// @@ challenge 2d-c3 end

// @@ challenge 2d-c4 begin
mod ch_2d_c4 {
    use proptest::prelude::*;

    use bustub::storage::index::dead_slots::DeadSlots;
    use std::collections::BTreeSet;

    #[test]
    fn s2d_c4_a_removed_key_can_be_inserted_again_and_is_found_once() {
        let mut d = DeadSlots::new();
        assert!(d.insert(5));
        assert!(d.remove(5));
        assert!(d.insert(5));
        assert!(d.contains(5));
        assert_eq!((d.len(), d.keys()), (1, vec![5]));
    }

    #[test]
    fn s2d_c4_reviving_repeatedly_never_duplicates() {
        let mut d = DeadSlots::new();
        for _ in 0..5 {
            assert!(d.insert(1));
            assert!(d.remove(1));
        }
        assert!(d.insert(1));
        assert_eq!((d.len(), d.keys()), (1, vec![1]));
        assert!(!d.insert(1), "already live");
    }

    #[test]
    fn s2d_c4_other_keys_are_not_disturbed() {
        let mut d = DeadSlots::new();
        for k in [3, 1, 2] {
            d.insert(k);
        }
        d.remove(2);
        d.insert(2);
        d.remove(1);
        assert_eq!(d.keys(), vec![2, 3]);
        assert!(!d.contains(1));
    }

    #[test]
    fn s2d_c4_removing_what_is_not_live_is_false() {
        let mut d = DeadSlots::new();
        assert!(!d.remove(9));
        d.insert(9);
        d.remove(9);
        assert!(!d.remove(9));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a `BTreeSet`, with `len` and `keys` agreeing.
        #[test]
        fn s2d_c4_property_a_dead_slot_list_is_a_set(ops in proptest::collection::vec((any::<bool>(), 0i64..6), 0..60)) {
            let mut d = DeadSlots::new();
            let mut m = BTreeSet::new();
            for (ins, k) in ops {
                if ins { prop_assert_eq!(d.insert(k), m.insert(k)); } else { prop_assert_eq!(d.remove(k), m.remove(&k)); }
                prop_assert_eq!(d.len(), m.len());
                prop_assert_eq!(d.keys(), m.iter().copied().collect::<Vec<_>>());
                for key in 0..6 { prop_assert_eq!(d.contains(key), m.contains(&key)); }
            }
        }
    }
}
// @@ challenge 2d-c4 end

// @@ challenge 2d-c5 begin
mod ch_2d_c5 {
    use proptest::prelude::*;

    use bustub::storage::index::leaf_reclaim::{insert_into_leaf, Entry, Inserted};
    use std::collections::BTreeSet;

    fn live(v: &[Entry]) -> BTreeSet<i32> {
        v.iter().filter(|e| e.1).map(|e| e.0).collect()
    }

    #[test]
    fn s2d_c5_an_existing_key_becomes_live_and_nothing_else_changes() {
        let leaf = vec![(1, true), (2, false), (3, true)];
        assert_eq!(insert_into_leaf(&leaf, 3, 2), Inserted::Done(vec![(1, true), (2, true), (3, true)]));
        assert_eq!(insert_into_leaf(&leaf, 3, 3), Inserted::Done(leaf.clone()), "already live: unchanged, and no split though full");
    }

    #[test]
    fn s2d_c5_a_leaf_with_room_takes_the_key_in_order() {
        let leaf = vec![(1, true), (5, false)];
        assert_eq!(insert_into_leaf(&leaf, 4, 3), Inserted::Done(vec![(1, true), (3, true), (5, false)]), "tombstones stay while there is room");
        assert_eq!(insert_into_leaf(&[], 2, 9), Inserted::Done(vec![(9, true)]));
    }

    #[test]
    fn s2d_c5_a_full_leaf_with_tombstones_is_cleaned_not_split() {
        let leaf = vec![(1, true), (2, false), (3, true)];
        assert_eq!(insert_into_leaf(&leaf, 3, 4), Inserted::Done(vec![(1, true), (3, true), (4, true)]));
        let all_dead = vec![(1, false), (2, false)];
        assert_eq!(insert_into_leaf(&all_dead, 2, 0), Inserted::Done(vec![(0, true)]));
    }

    #[test]
    fn s2d_c5_a_full_clean_leaf_splits_in_the_middle() {
        let leaf = vec![(1, true), (2, true), (3, true)];
        assert_eq!(insert_into_leaf(&leaf, 3, 4), Inserted::Split(vec![(1, true), (2, true)], vec![(3, true), (4, true)]));
        let leaf = vec![(10, true), (20, true), (30, true), (40, true)];
        match insert_into_leaf(&leaf, 4, 25) {
            Inserted::Split(l, r) => assert_eq!((l.len(), r.len()), (3, 2), "capacity + 1 entries, (capacity + 2) / 2 on the left"),
            other => panic!("expected a split, got {other:?}"),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: live keys are conserved plus the new one; halves are sorted and ordered; a split only for a full clean leaf.
        #[test]
        fn s2d_c5_property_live_keys_are_conserved(raw in proptest::collection::btree_map(0i32..30, any::<bool>(), 0..8), extra in 0usize..3, key in 0i32..30) {
            let leaf: Vec<Entry> = raw.into_iter().collect();
            let capacity = leaf.len() + extra;
            prop_assume!(capacity >= 1);
            let mut want = live(&leaf);
            want.insert(key);
            match insert_into_leaf(&leaf, capacity, key) {
                Inserted::Done(v) => {
                    prop_assert!(v.len() <= capacity);
                    prop_assert!(v.windows(2).all(|w| w[0].0 < w[1].0));
                    prop_assert_eq!(live(&v), want);
                }
                Inserted::Split(l, r) => {
                    prop_assert!(leaf.len() == capacity && leaf.iter().all(|e| e.1) && !leaf.iter().any(|e| e.0 == key));
                    prop_assert!(l.windows(2).all(|w| w[0].0 < w[1].0) && r.windows(2).all(|w| w[0].0 < w[1].0));
                    prop_assert!(l.last().unwrap().0 < r[0].0);
                    prop_assert_eq!(l.len() + r.len(), capacity + 1);
                    let mut both = live(&l);
                    both.extend(live(&r));
                    prop_assert_eq!(both, want);
                }
            }
        }
    }
}
// @@ challenge 2d-c5 end
