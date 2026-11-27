//! The `Tombs = 3` runs of `test/storage/b_plus_tree_concurrent_test.cpp` and the `NumTombs = 2` run of `SequentialEdgeMixTest` in
//! `b_plus_tree_delete_test.cpp` (BusTub runs each of those tests for a plain tree and for a tree with a tombstone buffer). Port of (BusTub, MIT, Copyright (c) 2015-2025 Carnegie Mellon University Database
//! Group). `LaunchParallelTest` becomes `thread::scope`. The C++ tests run each case for a plain tree and for a tree with a tombstone
//! buffer (`Tombs = 3`); this file is the tombstone half, the plain-tree half is in the other B+ tree test files.

mod b_plus_tree_utils;

use std::sync::Arc;
use std::thread;

use b_plus_tree_utils::*;
use std::collections::BTreeSet;
use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;

const NUM_ITERS: usize = 50;
const MIXTEST_NUM_ITERS: usize = 20;
const BPM_SIZE: usize = 50;

fn new_bpm() -> BufferPoolManager {
    BufferPoolManager::new(BPM_SIZE, Arc::new(DiskManagerUnlimitedMemory::new()))
}

/// Runs `f(thread_itr)` on `num_threads` threads and waits for them (BusTub's `LaunchParallelTest`).
fn launch_parallel_test(num_threads: u64, f: impl Fn(u64) + Sync) {
    thread::scope(|scope| {
        for thread_itr in 0..num_threads {
            let f = &f;
            scope.spawn(move || f(thread_itr));
        }
    });
}

fn insert_helper<const T: usize>(tree: &Tree<T>, keys: &[i64]) {
    for &key in keys {
        tree.insert(&index_key(key), &rid_of(key));
    }
}

/// Inserts only the keys whose value modulo `total_threads` is `thread_itr`.
fn insert_helper_split<const T: usize>(tree: &Tree<T>, keys: &[i64], total_threads: u64, thread_itr: u64) {
    for &key in keys {
        if key as u64 % total_threads == thread_itr {
            tree.insert(&index_key(key), &rid_of(key));
        }
    }
}

fn delete_helper<const T: usize>(tree: &Tree<T>, remove_keys: &[i64]) {
    for &key in remove_keys {
        tree.remove(&index_key(key));
    }
}

fn delete_helper_split<const T: usize>(tree: &Tree<T>, remove_keys: &[i64], total_threads: u64, thread_itr: u64) {
    for &key in remove_keys {
        if key as u64 % total_threads == thread_itr {
            tree.remove(&index_key(key));
        }
    }
}

fn lookup_helper<const T: usize>(tree: &Tree<T>, keys: &[i64]) {
    for &key in keys {
        let result = tree.get_value(&index_key(key));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], rid_of(key));
    }
}

fn insert_test_1_call<const T: usize>() {
    for _ in 0..NUM_ITERS {
        let bpm = new_bpm();
        let tree = new_tree_t::<T>(&bpm, 3, 5);

        // keys to insert
        let keys: Vec<i64> = (1..100).collect();
        launch_parallel_test(2, |_| insert_helper(&tree, &keys));

        for &key in &keys {
            let rids = tree.get_value(&index_key(key));
            assert_eq!(rids.len(), 1);
            assert_eq!(rids[0].slot_num() as i64, key & 0xFFFF_FFFF);
        }

        let mut current_key = 1;
        for (_, location) in tree.begin() {
            assert_eq!(location.page_id().0, 0);
            assert_eq!(location.slot_num() as i64, current_key);
            current_key += 1;
        }
        assert_eq!(current_key, keys.len() as i64 + 1);
    }
}

fn insert_test_2_call<const T: usize>() {
    for _ in 0..NUM_ITERS {
        let bpm = new_bpm();
        let tree = new_tree_t::<T>(&bpm, 3, 5);

        let keys: Vec<i64> = (1..1000).collect();
        launch_parallel_test(2, |thread_itr| insert_helper_split(&tree, &keys, 2, thread_itr));

        for &key in &keys {
            let rids = tree.get_value(&index_key(key));
            assert_eq!(rids.len(), 1);
            assert_eq!(rids[0].slot_num() as i64, key & 0xFFFF_FFFF);
        }

        let mut current_key = 1;
        for (_, location) in tree.begin() {
            assert_eq!(location.page_id().0, 0);
            assert_eq!(location.slot_num() as i64, current_key);
            current_key += 1;
        }
        assert_eq!(current_key, keys.len() as i64 + 1);
    }
}

fn delete_test_1_call<const T: usize>() {
    for _ in 0..NUM_ITERS {
        let bpm = new_bpm();
        let tree = new_tree_t::<T>(&bpm, 3, 5);

        // sequential insert
        insert_helper(&tree, &[1, 2, 3, 4, 5]);

        let remove_keys = [1, 5, 3, 4];
        launch_parallel_test(2, |_| delete_helper(&tree, &remove_keys));

        let mut current_key = 2;
        let mut size = 0;
        for (_, location) in tree.begin() {
            assert_eq!(location.page_id().0, 0);
            assert_eq!(location.slot_num() as i64, current_key);
            current_key += 1;
            size += 1;
        }
        assert_eq!(size, 1);
    }
}

fn delete_test_2_call<const T: usize>() {
    for _ in 0..NUM_ITERS {
        let bpm = new_bpm();
        let tree = new_tree_t::<T>(&bpm, 3, 5);

        // sequential insert
        insert_helper(&tree, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        let remove_keys = [1, 4, 3, 2, 5, 6];
        launch_parallel_test(2, |thread_itr| delete_helper_split(&tree, &remove_keys, 2, thread_itr));

        let mut current_key = 7;
        let mut size = 0;
        for (_, location) in tree.begin() {
            assert_eq!(location.page_id().0, 0);
            assert_eq!(location.slot_num() as i64, current_key);
            current_key += 1;
            size += 1;
        }
        assert_eq!(size, 4);
    }
}

fn mix_test_1_call<const T: usize>() {
    for _ in 0..MIXTEST_NUM_ITERS {
        let bpm = new_bpm();
        let tree = new_tree_t::<T>(&bpm, 3, 5);

        // first, populate the index
        let sieve = 2;
        let total_keys = 1000;
        let for_insert: Vec<i64> = (1..=total_keys).filter(|i| i % sieve == 0).collect();
        let for_delete: Vec<i64> = (1..=total_keys).filter(|i| i % sieve != 0).collect();
        // insert all the keys to delete
        insert_helper(&tree, &for_delete);

        let num_threads = 10;
        launch_parallel_test(num_threads, |tid| {
            if tid % 2 == 0 {
                insert_helper(&tree, &for_insert);
            } else {
                delete_helper(&tree, &for_delete);
            }
        });

        let mut size = 0;
        for (key, _) in tree.begin() {
            assert_eq!(key.get_as_integer(), for_insert[size]);
            size += 1;
        }
        assert_eq!(size, for_insert.len());
    }
}

fn mix_test_2_call<const T: usize>() {
    for _ in 0..MIXTEST_NUM_ITERS {
        let bpm = new_bpm();
        let tree = new_tree_t::<T>(&bpm, Tree::<T>::default_leaf_max_size(), Tree::<T>::default_internal_max_size());

        // add the preserved keys
        let total_keys = 1000;
        let sieve = 10;
        let preserved_keys: Vec<i64> = (1..=total_keys).filter(|i| i % sieve == 0).collect();
        let dynamic_keys: Vec<i64> = (1..=total_keys).filter(|i| i % sieve != 0).collect();
        insert_helper(&tree, &preserved_keys);

        let num_threads = 6;
        launch_parallel_test(num_threads, |tid| match tid % 3 {
            0 => insert_helper(&tree, &dynamic_keys),
            1 => delete_helper(&tree, &dynamic_keys),
            _ => lookup_helper(&tree, &preserved_keys),
        });

        // check that all the preserved keys exist
        let size = tree.begin().filter(|(key, _)| key.get_as_integer() % sieve == 0).count();
        assert_eq!(size, preserved_keys.len());
    }
}

#[test]
fn insert_test_1_with_tombstones() {
    insert_test_1_call::<3>();
}

#[test]
fn insert_test_2_with_tombstones() {
    insert_test_2_call::<3>();
}

#[test]
fn delete_test_1_with_tombstones() {
    delete_test_1_call::<3>();
}

#[test]
fn delete_test_2_with_tombstones() {
    delete_test_2_call::<3>();
}

#[test]
fn mix_test_1_with_tombstones() {
    mix_test_1_call::<3>();
}

#[test]
fn mix_test_2_with_tombstones() {
    mix_test_2_call::<3>();
}

#[test]
fn sequential_edge_mix_with_tombstones() {
    let bpm = new_bpm();
    for leaf_max_size in 2..=5 {
        let tree = new_tree_t::<2>(&bpm, leaf_max_size, 3);
        let mut inserted: Vec<i64> = vec![];
        let mut deleted: Vec<i64> = vec![];
        for key in [1, 5, 15, 20, 25, 2, -1, -2, 6, 14, 4] {
            tree.insert(&index_key(key), &rid_of(key));
            inserted.push(key);
            assert!(tree_values_match(&tree, &inserted, &deleted));
        }
        tree.remove(&index_key(1));
        deleted.push(1);
        inserted.retain(|&k| k != 1);
        assert!(tree_values_match(&tree, &inserted, &deleted));
        tree.insert(&index_key(3), &rid_of(3));
        inserted.push(3);
        assert!(tree_values_match(&tree, &inserted, &deleted));
        for key in [4, 14, 6, 2, 15, -2, -1, 3, 5, 25, 20] {
            tree.remove(&index_key(key));
            deleted.push(key);
            inserted.retain(|&k| k != key);
            assert!(tree_values_match(&tree, &inserted, &deleted));
        }
    }
}

#[test]
fn optimistic_insert_and_delete_with_tombstones_still_latch_one_page() {
    let bpm = new_bpm();
    let tree = new_tree_t::<2>(&bpm, 4, 3);
    for i in 0..5 {
        tree.insert(&index_key(2 * i), &rid_of(i));
    }
    let (reads, writes) = (tree.bpm.get_reads(), tree.bpm.get_writes());
    assert!(tree.insert(&index_key(1), &rid_of(1)));
    assert!(tree.bpm.get_reads() - reads > 0);
    assert_eq!(tree.bpm.get_writes() - writes, 1);
    let (reads, writes) = (tree.bpm.get_reads(), tree.bpm.get_writes());
    tree.remove(&index_key(1)); // a logical delete: the leaf is not touched beyond its own latch
    assert!(tree.bpm.get_reads() - reads > 0);
    assert_eq!(tree.bpm.get_writes() - writes, 1);
}

fn random_workload<const T: usize>(leaf: u32, internal: u32, seed: u64) {
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % n
        }
    }
    let bpm = new_bpm();
    let tree = new_tree_t::<T>(&bpm, leaf, internal);
    let mut model = BTreeSet::new();
    let mut rng = Lcg(seed);
    for step in 0..1500 {
        let key = rng.next(80) as i64;
        if rng.next(5) < 3 {
            assert_eq!(insert(&tree, key), model.insert(key), "T={T} ({leaf},{internal}) seed {seed} step {step}: insert {key}");
        } else {
            model.remove(&key);
            remove(&tree, key);
        }
        // the shape check walks the whole tree: every fifth step (and the last) finds a broken rule within five operations
        if step % 5 == 0 || step == 1499 {
            check_shape_t::<T>(&tree, &model, leaf, internal, step % 25 == 0).unwrap_or_else(|e| panic!("T={T} ({leaf},{internal}) seed {seed} step {step}: {e}\n{}", leaves_t(&tree)));
        }
    }
    for k in 0..80 {
        assert_eq!(get(&tree, k).len(), model.contains(&k) as usize, "key {k}");
    }
}

#[test]
fn random_inserts_and_removes_agree_with_a_model_for_every_buffer_size() {
    for (leaf, internal) in [(2, 3), (3, 3), (4, 4), (5, 4), (6, 7)] {
        for seed in 0..3 {
            random_workload::<1>(leaf, internal, seed);
            random_workload::<2>(leaf, internal, seed + 10);
            random_workload::<3>(leaf, internal, seed + 20);
        }
    }
}
