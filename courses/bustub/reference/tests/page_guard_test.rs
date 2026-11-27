//! Port of `test/storage/page_guard_test.cpp` (BusTub, MIT, Copyright (c) 2015-2025 Carnegie Mellon University Database Group).
//! C++ move semantics leave a moved-from guard behind (hence its `is_valid_` flag and the "invalid guard" cases of `MoveTest`).
//! Rust moves leave nothing behind, so `std::move(a)` is just `a`, and assigning over a guard drops the old one.

use std::sync::Arc;

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::PageId;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;
use bustub::storage::page::page_guard::WritePageGuard;

const FRAMES: usize = 10;

fn new_bpm() -> (BufferPoolManager, Arc<DiskManagerUnlimitedMemory>) {
    let disk_manager = Arc::new(DiskManagerUnlimitedMemory::new());
    (BufferPoolManager::new(FRAMES, disk_manager.clone()), disk_manager)
}

#[test]
fn drop_test() {
    let (bpm, _disk_manager) = new_bpm();

    {
        let pid0 = bpm.new_page();
        let mut page0 = bpm.write_page(pid0);

        // The page should be pinned.
        assert_eq!(Some(1), bpm.get_pin_count(pid0));

        // A drop should unpin the page.
        page0.release();
        assert_eq!(Some(0), bpm.get_pin_count(pid0));

        // Another drop should have no effect.
        page0.release();
        assert_eq!(Some(0), bpm.get_pin_count(pid0));
    } // Destructor should be called. Useless but should not cause issues.

    let pid1 = bpm.new_page();
    let pid2 = bpm.new_page();

    {
        let mut read_guarded_page = bpm.read_page(pid1);
        let mut write_guarded_page = bpm.write_page(pid2);

        assert_eq!(Some(1), bpm.get_pin_count(pid1));
        assert_eq!(Some(1), bpm.get_pin_count(pid2));

        // Dropping should unpin the pages.
        read_guarded_page.release();
        write_guarded_page.release();
        assert_eq!(Some(0), bpm.get_pin_count(pid1));
        assert_eq!(Some(0), bpm.get_pin_count(pid2));

        // Another drop should have no effect.
        read_guarded_page.release();
        write_guarded_page.release();
        assert_eq!(Some(0), bpm.get_pin_count(pid1));
        assert_eq!(Some(0), bpm.get_pin_count(pid2));
    } // Destructor should be called. Useless but should not cause issues.

    // This will hang if the latches were not unlocked correctly in the destructors.
    {
        let _write_test1 = bpm.write_page(pid1);
        let _write_test2 = bpm.write_page(pid2);
    }

    let mut page_ids: Vec<PageId> = Vec::new();
    {
        // Fill up the BPM.
        let mut guards: Vec<WritePageGuard> = Vec::new();
        for _ in 0..FRAMES {
            let new_pid = bpm.new_page();
            guards.push(bpm.write_page(new_pid));
            assert_eq!(Some(1), bpm.get_pin_count(new_pid));
            page_ids.push(new_pid);
        }
    } // This drops all of the guards.

    for pid in &page_ids {
        assert_eq!(Some(0), bpm.get_pin_count(*pid));
    }

    // Get a new write page and edit it. We will retrieve it later
    let mutable_page_id = bpm.new_page();
    let mut mutable_guard = bpm.write_page(mutable_page_id);
    mutable_guard.get_data_mut()[..5].copy_from_slice(b"data\0");
    mutable_guard.release();

    {
        // Fill up the BPM again.
        let mut guards: Vec<WritePageGuard> = Vec::new();
        for _ in 0..FRAMES {
            let new_pid = bpm.new_page();
            guards.push(bpm.write_page(new_pid));
            assert_eq!(Some(1), bpm.get_pin_count(new_pid));
        }
    }

    // Fetching the flushed page should result in seeing the changed value.
    let immutable_guard = bpm.read_page(mutable_page_id);
    assert_eq!(&immutable_guard.get_data()[..5], b"data\0");
}

#[test]
#[allow(unused_assignments)] // the reassignments are the point: each drops the guard it replaces
fn move_test() {
    let (bpm, _disk_manager) = new_bpm();

    let pid0 = bpm.new_page();
    let pid1 = bpm.new_page();
    let pid2 = bpm.new_page();
    let pid3 = bpm.new_page();
    let pid4 = bpm.new_page();
    let pid5 = bpm.new_page();

    let mut guard0 = bpm.read_page(pid0);
    let guard1 = bpm.read_page(pid1);
    assert_eq!(Some(1), bpm.get_pin_count(pid0));
    assert_eq!(Some(1), bpm.get_pin_count(pid1));

    // (C++ moves a guard onto itself here and checks the pin count doesn't change. Rust can't write that.)

    // Invalidate the old guard0 by move assignment.
    guard0 = guard1;
    assert_eq!(Some(0), bpm.get_pin_count(pid0));
    assert_eq!(Some(1), bpm.get_pin_count(pid1));

    // Invalidate the old guard0 by move construction.
    let guard0a = guard0;
    assert_eq!(Some(0), bpm.get_pin_count(pid0));
    assert_eq!(Some(1), bpm.get_pin_count(pid1));

    let mut guard2 = bpm.read_page(pid2);
    let guard3 = bpm.read_page(pid3);
    assert_eq!(Some(1), bpm.get_pin_count(pid2));
    assert_eq!(Some(1), bpm.get_pin_count(pid3));

    // Invalidate the old guard2 by move assignment.
    guard2 = guard3;
    assert_eq!(Some(0), bpm.get_pin_count(pid2));
    assert_eq!(Some(1), bpm.get_pin_count(pid3));

    // Invalidate the old guard2 by move construction.
    let guard2a = guard2;
    assert_eq!(Some(0), bpm.get_pin_count(pid2));
    assert_eq!(Some(1), bpm.get_pin_count(pid3));

    // This will hang if page 2 was not unlatched correctly.
    {
        let _temp_guard2 = bpm.write_page(pid2);
    }

    let mut guard4 = bpm.write_page(pid4);
    let guard5 = bpm.write_page(pid5);
    assert_eq!(Some(1), bpm.get_pin_count(pid4));
    assert_eq!(Some(1), bpm.get_pin_count(pid5));

    // Invalidate the old guard4 by move assignment.
    guard4 = guard5;
    assert_eq!(Some(0), bpm.get_pin_count(pid4));
    assert_eq!(Some(1), bpm.get_pin_count(pid5));

    // Invalidate the old guard4 by move construction.
    let guard4a = guard4;
    assert_eq!(Some(0), bpm.get_pin_count(pid4));
    assert_eq!(Some(1), bpm.get_pin_count(pid5));

    // This will hang if page 4 was not unlatched correctly.
    {
        let _temp_guard4 = bpm.read_page(pid4);
    }

    // The guards that survived still hold their pages.
    assert_eq!(Some(1), bpm.get_pin_count(guard0a.get_page_id()));
    assert_eq!(Some(1), bpm.get_pin_count(guard2a.get_page_id()));
    assert_eq!(Some(1), bpm.get_pin_count(guard4a.get_page_id()));

    // Move assignment over an existing guard: the guard that was there is dropped and its page unpinned.
    {
        let pid = bpm.new_page();
        let mut read = bpm.read_page(pid);
        assert_eq!(Some(1), bpm.get_pin_count(pid));
        let replacement = bpm.read_page(pid1);
        read = replacement;
        assert_eq!(Some(0), bpm.get_pin_count(pid));
        let _ = read;
    }
}
