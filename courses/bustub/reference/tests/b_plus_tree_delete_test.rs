//! Port of `test/storage/b_plus_tree_delete_test.cpp` (BusTub, MIT, Copyright (c) 2015-2025 Carnegie Mellon University Database
//! Group). The C++ test runs `SequentialEdgeMixTest` on a tree with a tombstone buffer (`NumTombs = 2`); the tombstone buffer is
//! module 2d, so here it is the plain tree.
//!
//! `optimistic_delete_test` counts the latches of the optimistic fast path, which module 2e adds: the course runs it with that module's tests.

mod b_plus_tree_utils;

use std::sync::Arc;

use b_plus_tree_utils::*;
use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::PageId;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;

fn new_bpm(frames: usize) -> BufferPoolManager {
    BufferPoolManager::new(frames, Arc::new(DiskManagerUnlimitedMemory::new()))
}

#[test]
fn delete_test_no_iterator() {
    let bpm = new_bpm(50);
    let tree = new_tree(&bpm, 2, 3);

    let keys = [1, 2, 3, 4, 5];
    for key in keys {
        tree.insert(&index_key(key), &rid_of(key));
    }

    for key in keys {
        let rids = tree.get_value(&index_key(key));
        assert_eq!(rids.len(), 1);
        assert_eq!(rids[0].slot_num() as i64, key & 0xFFFF_FFFF);
    }

    let remove_keys = [1, 5, 3, 4];
    for key in remove_keys {
        tree.remove(&index_key(key));
    }

    let mut size = 0;
    for key in keys {
        let rids = tree.get_value(&index_key(key));
        let is_present = !rids.is_empty();

        if !is_present {
            assert!(remove_keys.contains(&key));
        } else {
            assert_eq!(rids.len(), 1);
            assert_eq!(rids[0].page_id().0, 0);
            assert_eq!(rids[0].slot_num() as i64, key);
            size += 1;
        }
    }
    assert_eq!(size, 1);

    // Remove the remaining key
    tree.remove(&index_key(2));
    let root_page_id = tree.get_root_page_id();
    assert_eq!(root_page_id, PageId::INVALID);
}

#[test]
fn optimistic_delete_test() {
    let bpm = new_bpm(50);
    let tree = new_tree(&bpm, 4, 3);

    let num_keys = 25;
    for i in 0..num_keys {
        tree.insert(&index_key(i), &rid_of(i));
    }

    // a leaf that stays at least half full after losing a key: its size > min_size = max_size / 2
    let mut to_delete = num_keys + 1;
    let firsts = leaf_first_keys(&tree);
    for (leaf, size) in tree.leaf_sizes().into_iter().enumerate() {
        if (size as u32) > 4 / 2 {
            to_delete = firsts[leaf];
        }
    }

    let base_reads = tree.bpm.get_reads();
    let base_writes = tree.bpm.get_writes();

    tree.remove(&index_key(to_delete));

    let new_reads = tree.bpm.get_reads();
    let new_writes = tree.bpm.get_writes();

    assert!(new_reads - base_reads > 0);
    assert_eq!(new_writes - base_writes, 1);
}

#[test]
fn sequential_edge_mix_test() {
    let bpm = new_bpm(50);

    for leaf_max_size in 2..=5 {
        // a fresh tree (and header page) for each leaf size
        let tree = new_tree(&bpm, leaf_max_size, 3);

        let mut keys = vec![1, 5, 15, 20, 25, 2, -1, -2, 6, 14, 4];
        let mut inserted: Vec<i64> = vec![];
        let mut deleted: Vec<i64> = vec![];
        for &key in &keys {
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

        keys = vec![4, 14, 6, 2, 15, -2, -1, 3, 5, 25, 20];
        for &key in &keys {
            tree.remove(&index_key(key));
            deleted.push(key);
            inserted.retain(|&k| k != key);
            assert!(tree_values_match(&tree, &inserted, &deleted));
        }
    }
}
