//! Port of `test/storage/b_plus_tree_insert_test.cpp` (BusTub, MIT, Copyright (c) 2015-2025 Carnegie Mellon University Database
//! Group). `GetValue(key, &result)` returning a bool and filling a vector becomes `get_value(&key) -> Vec<V>`; the C++ iterator
//! loop `for (iter = tree.Begin(); iter != tree.End(); ++iter)` becomes a `for` over the Rust iterator.
//!
//! `optimistic_insert_test` counts the latches of the optimistic fast path, which module 2e adds: the course runs it with that module's tests.

mod b_plus_tree_utils;

use std::sync::Arc;

use b_plus_tree_utils::*;
use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;

fn new_bpm(frames: usize) -> BufferPoolManager {
    BufferPoolManager::new(frames, Arc::new(DiskManagerUnlimitedMemory::new()))
}

#[test]
fn basic_insert_test() {
    let bpm = new_bpm(50);
    let tree = new_tree(&bpm, 2, 3);

    let key = 42;
    tree.insert(&index_key(key), &rid_of(key));

    // the root is a leaf holding the one key
    assert_eq!(tree.depth(), 1);
    assert_eq!(tree.leaf_sizes(), vec![1]);
    assert_eq!(keys_by_scan(&tree), vec![key]);
}

#[test]
fn optimistic_insert_test() {
    let bpm = new_bpm(50);
    let tree = new_tree(&bpm, 4, 3);

    // Inserting 5 keys ensures there is at least one leaf page with at most 2 keys. This allows reliably testing for optimistic
    // insertions across any combination of design decisions such as when a leaf page is considered to have overflowed, how keys are
    // distributed on splits, etc.
    let num_keys = 5;
    for i in 0..num_keys {
        tree.insert(&index_key(2 * i), &rid_of(i));
    }

    // a leaf with room for one more: its size + 1 < max_size; the key to insert is one more than its first key (which lands there)
    let mut to_insert = 2 * num_keys;
    let firsts = leaf_first_keys(&tree);
    for (leaf, size) in tree.leaf_sizes().into_iter().enumerate() {
        if (size as u32) + 1 < 4 {
            to_insert = firsts[leaf] + 1;
        }
    }
    assert_ne!(to_insert, 2 * num_keys);

    let base_reads = tree.bpm.get_reads();
    let base_writes = tree.bpm.get_writes();

    assert!(tree.insert(&index_key(to_insert), &rid_of(to_insert)));

    let new_reads = tree.bpm.get_reads();
    let new_writes = tree.bpm.get_writes();

    assert!(new_reads - base_reads > 0);
    assert_eq!(new_writes - base_writes, 1);
}

#[test]
fn insert_test_1_no_iterator() {
    let bpm = new_bpm(50);
    let tree = new_tree(&bpm, 2, 3);

    let keys = [1, 2, 3, 4, 5];
    for key in keys {
        tree.insert(&index_key(key), &rid_of(key));
    }

    for key in keys {
        let rids = tree.get_value(&index_key(key));
        let is_present = !rids.is_empty();

        assert!(is_present);
        assert_eq!(rids.len(), 1);
        assert_eq!(rids[0].page_id().0, 0);
        assert_eq!(rids[0].slot_num() as i64, key & 0xFFFF_FFFF);
    }
}

#[test]
fn insert_test_2() {
    let bpm = new_bpm(50);
    let tree = new_tree(&bpm, 2, 3);

    let keys = [5, 4, 3, 2, 1];
    for key in keys {
        tree.insert(&index_key(key), &rid_of(key));
    }

    for key in keys {
        let rids = tree.get_value(&index_key(key));
        assert_eq!(rids.len(), 1);
        assert_eq!(rids[0].slot_num() as i64, key & 0xFFFF_FFFF);
    }

    let start_key = 1;
    let mut current_key = start_key;
    for (_, location) in tree.begin() {
        assert_eq!(location.page_id().0, 0);
        assert_eq!(location.slot_num() as i64, current_key);
        current_key += 1;
    }

    assert_eq!(current_key, keys.len() as i64 + 1);

    let start_key = 3;
    let mut current_key = start_key;
    let mut iterator = tree.begin_at(&index_key(start_key));
    while !iterator.is_end() {
        let (_, location) = iterator.next().unwrap();
        assert_eq!(location.page_id().0, 0);
        assert_eq!(location.slot_num() as i64, current_key);
        current_key += 1;
    }
}
