//! Tests for module 1a, the disk manager. A test name starts with its stage: `s1a_02_…` belongs to stage 1a-02, and
//! `anneal course test` runs just those.
//!
//! These tests use only the public API (`DiskManager`, `DiskIo` and the memory disks). They never look at how you store
//! pages. Most of them are *properties*: a random sequence of operations is run against your code and against a plain
//! `HashMap` (the model); every answer must agree with the model. When a property fails, the test prints the shortest
//! failing sequence it could find: read it from the top, it is a recipe for the bug.

mod common;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use bustub::common::config::{PageData, PageId, BUSTUB_PAGE_SIZE, DEFAULT_DB_IO_SIZE};
use bustub::storage::disk::disk_manager::{copy_page, DiskIo, DiskManager};
use bustub::storage::disk::disk_manager_memory::{DiskManagerMemory, DiskManagerUnlimitedMemory};
use common::{pattern, TempDir};
use proptest::prelude::*;

const PS: usize = BUSTUB_PAGE_SIZE;
const ZERO: PageData = [0u8; PS];

fn config() -> ProptestConfig {
    ProptestConfig { cases: 48, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

fn new_dm(dir: &TempDir) -> DiskManager {
    DiskManager::new(dir.path("test.bustub")).unwrap()
}

/// One operation on a disk. A page's contents are `pattern(seed)`, so a page read from the wrong place is noticed.
#[derive(Clone, Debug)]
enum Op {
    Write(i32, u16),
    Read(i32),
    Delete(i32),
}

/// Random operation sequences over page ids `0..pages`.
fn ops(pages: i32, deletes: bool) -> BoxedStrategy<Vec<Op>> {
    let id = 0..pages;
    let write = (id.clone(), any::<u16>()).prop_map(|(p, s)| Op::Write(p, s));
    let read = id.clone().prop_map(Op::Read);
    let delete = id.prop_map(Op::Delete);
    if deletes {
        prop::collection::vec(prop_oneof![4 => write, 3 => read, 2 => delete], 1..60).boxed()
    } else {
        prop::collection::vec(prop_oneof![4 => write, 3 => read], 1..60).boxed()
    }
}

/// What the model test needs from a disk. It is implemented for `DiskManager`'s own methods (so stages 1 to 3 do not
/// depend on the `DiskIo` trait of stage 4) and for anything behind `&dyn DiskIo`.
trait Store {
    fn put(&self, id: PageId, data: &PageData);
    fn get(&self, id: PageId, buf: &mut PageData);
    fn del(&self, id: PageId);
}

impl Store for DiskManager {
    fn put(&self, id: PageId, data: &PageData) {
        self.write_page(id, data).unwrap()
    }
    fn get(&self, id: PageId, buf: &mut PageData) {
        self.read_page(id, buf).unwrap()
    }
    fn del(&self, id: PageId) {
        self.delete_page(id)
    }
}

struct Dyn<'a>(&'a dyn DiskIo);

impl Store for Dyn<'_> {
    fn put(&self, id: PageId, data: &PageData) {
        self.0.write_page(id, data).unwrap()
    }
    fn get(&self, id: PageId, buf: &mut PageData) {
        self.0.read_page(id, buf).unwrap()
    }
    fn del(&self, id: PageId) {
        self.0.delete_page(id)
    }
}

/// Runs `ops` on `disk` and checks every read against the model. After a delete, a read of that page is unspecified
/// until it is written again, so such reads are not checked.
fn agrees_with_the_model(disk: &impl Store, ops: &[Op]) -> Result<(), TestCaseError> {
    let mut model: HashMap<i32, u16> = HashMap::new();
    let mut unspecified: HashSet<i32> = HashSet::new();
    let mut buf = ZERO;
    for (step, op) in ops.iter().enumerate() {
        match *op {
            Op::Write(p, s) => {
                disk.put(PageId(p), &pattern(s as usize));
                model.insert(p, s);
                unspecified.remove(&p);
            }
            Op::Delete(p) => {
                disk.del(PageId(p));
                model.remove(&p);
                unspecified.insert(p);
            }
            Op::Read(p) => {
                buf.fill(0xEE);
                disk.get(PageId(p), &mut buf);
                if unspecified.contains(&p) {
                    continue;
                }
                let want = model.get(&p).map(|&s| pattern(s as usize)).unwrap_or(ZERO);
                prop_assert!(buf == want, "step {step}: reading page {p} gave the wrong bytes (page {p} was {})", match model.get(&p) {
                    Some(s) => format!("last written with pattern {s}"),
                    None => "never written, so it must read as all zeros".to_string(),
                });
            }
        }
    }
    Ok(())
}

// ---- 1a-01 · Pages survive: write, read, files -----------------------------------------------------------------

#[test]
fn s1a_01_new_creates_the_db_file_and_a_log_file_next_to_it() {
    let dir = TempDir::new("create");
    let dm = new_dm(&dir);
    assert!(dir.path("test.bustub").is_file(), "the db file should exist after new");
    assert!(dir.path("test.log").is_file(), "the log file is the db path with its extension replaced by log");
    assert_eq!(dm.db_file_name(), dir.path("test.bustub"), "db_file_name is the path new was given");
    assert_eq!(dm.log_file_name(), dir.path("test.log"), "log_file_name is next to the db file");
}

#[test]
fn s1a_01_a_path_that_cannot_be_opened_is_an_error_not_a_panic() {
    let dir = TempDir::new("badpath");
    let result = DiskManager::new(dir.path("no-such-directory/test.bustub"));
    assert!(result.is_err(), "opening a file in a directory that does not exist must return Err");
}

#[test]
fn s1a_01_opening_the_same_file_twice_in_a_row_works() {
    let dir = TempDir::new("reopen");
    drop(new_dm(&dir));
    let again = new_dm(&dir);
    let mut buf = ZERO;
    again.read_page(PageId(0), &mut buf).unwrap();
    assert!(buf == ZERO, "a page never written reads as zeros, also after reopening an existing file");
}

#[test]
fn s1a_01_opening_an_existing_file_never_shrinks_it() {
    let dir = TempDir::new("no-shrink");
    let big = 1000 * BUSTUB_PAGE_SIZE as u64;
    std::fs::File::create(dir.path("test.bustub")).unwrap().set_len(big).unwrap();
    let dm = new_dm(&dir);
    let len = || std::fs::metadata(dir.path("test.bustub")).unwrap().len();
    assert!(len() >= big, "opening an existing database file keeps its contents, so it must not cut the file short");
    for p in 0..40 {
        dm.write_page(PageId(p), &[p as u8; BUSTUB_PAGE_SIZE]).unwrap();
    }
    assert!(len() >= big, "allocating pages in a reopened file must not shrink it either");
}

#[test]
fn s1a_01_a_page_that_was_never_written_reads_as_zeros() {
    let dir = TempDir::new("zeros");
    let dm = new_dm(&dir);
    let mut buf = [0xAAu8; PS];
    dm.read_page(PageId(7), &mut buf).unwrap();
    assert!(buf == ZERO, "the whole buffer must be overwritten with zeros, not just left as it was");
}

#[test]
fn s1a_01_a_written_page_reads_back_and_rewriting_replaces_it() {
    let dir = TempDir::new("rw");
    let dm = new_dm(&dir);
    let mut buf = ZERO;
    dm.write_page(PageId(3), &pattern(1)).unwrap();
    dm.read_page(PageId(3), &mut buf).unwrap();
    assert!(buf == pattern(1), "page 3 should read back what was written");
    dm.write_page(PageId(3), &pattern(2)).unwrap();
    dm.read_page(PageId(3), &mut buf).unwrap();
    assert!(buf == pattern(2), "a rewrite replaces the page, it does not add to it");
}

#[test]
fn s1a_01_very_large_page_ids_work() {
    let dir = TempDir::new("large");
    let dm = new_dm(&dir);
    let mut buf = ZERO;
    for id in [0, 1, 1_000, 1_000_000, i32::MAX / 2] {
        dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
    }
    for id in [0, 1, 1_000, 1_000_000, i32::MAX / 2] {
        dm.read_page(PageId(id), &mut buf).unwrap();
        assert!(buf == pattern(id as usize), "page {id} should read back its own contents, however large its id");
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1a_01_the_disk_behaves_like_a_map_from_page_id_to_page(ops in ops(24, false)) {
        let dir = TempDir::new("prop1");
        let dm = new_dm(&dir);
        agrees_with_the_model(&dm, &ops)?;
    }

    #[test]
    fn s1a_01_pages_do_not_disturb_each_other(seeds in prop::collection::vec(any::<u16>(), 1..40), order in any::<u64>()) {
        let dir = TempDir::new("prop1b");
        let dm = new_dm(&dir);
        // write all pages, in a scrambled order, then read them all
        let mut ids: Vec<usize> = (0..seeds.len()).collect();
        ids.sort_by_key(|&i| (i as u64).wrapping_mul(order | 1).rotate_left(17));
        for &i in &ids {
            dm.write_page(PageId(i as i32), &pattern(seeds[i] as usize)).unwrap();
        }
        let mut buf = ZERO;
        for (i, &s) in seeds.iter().enumerate() {
            dm.read_page(PageId(i as i32), &mut buf).unwrap();
            prop_assert!(buf == pattern(s as usize), "page {i} was disturbed by a write to another page");
        }
    }
}

// ---- 1a-02 · Delete and reuse ------------------------------------------------------------------------------------

/// The most bytes a sensible disk manager needs for a workload whose live pages never exceeded `peak`: room for twice
/// the peak (growing by doubling is the usual way) or the initial room, plus BusTub's one spare page.
fn size_bound(peak: usize) -> u64 {
    (PS * (DEFAULT_DB_IO_SIZE.max(2 * peak) + 1)) as u64
}

#[test]
fn s1a_02_deleting_a_page_that_was_never_written_does_nothing() {
    let dir = TempDir::new("del-none");
    let dm = new_dm(&dir);
    dm.delete_page(PageId(9));
    dm.write_page(PageId(1), &pattern(5)).unwrap();
    dm.delete_page(PageId(9));
    let mut buf = ZERO;
    dm.read_page(PageId(1), &mut buf).unwrap();
    assert!(buf == pattern(5), "deleting an unknown page must not disturb a page that exists");
}

#[test]
fn s1a_02_a_deleted_page_id_can_be_written_again() {
    let dir = TempDir::new("del-again");
    let dm = new_dm(&dir);
    let mut buf = ZERO;
    dm.write_page(PageId(4), &pattern(1)).unwrap();
    dm.delete_page(PageId(4));
    dm.write_page(PageId(4), &pattern(2)).unwrap();
    dm.read_page(PageId(4), &mut buf).unwrap();
    assert!(buf == pattern(2), "page 4 is a new page after the delete: it reads what was written after it");
}

#[test]
fn s1a_02_deleting_one_page_leaves_the_others_alone() {
    let dir = TempDir::new("del-others");
    let dm = new_dm(&dir);
    for id in 0..10 {
        dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
    }
    dm.delete_page(PageId(4));
    dm.write_page(PageId(20), &pattern(99)).unwrap(); // reuses what page 4 left behind
    let mut buf = ZERO;
    for id in (0..10).filter(|&i| i != 4) {
        dm.read_page(PageId(id), &mut buf).unwrap();
        assert!(buf == pattern(id as usize), "page {id} must be unchanged by deleting page 4 and writing page 20");
    }
}

#[test]
fn s1a_02_churn_at_a_constant_number_of_live_pages_never_grows_the_file() {
    // BusTub's DeletePageTest, as a property: 100 pages live, then 200 pages come and go one at a time.
    let dir = TempDir::new("churn");
    let dm = new_dm(&dir);
    for id in 0..100 {
        dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
    }
    // The first page to come and go makes 101 live at once, which may grow a file sized exactly for 100: measure after it.
    dm.write_page(PageId(100), &pattern(100)).unwrap();
    dm.delete_page(PageId(100));
    let size = dm.get_db_file_size();
    for id in 101..300 {
        dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
        dm.delete_page(PageId(id));
        assert_eq!(dm.get_db_file_size(), size, "page {id} came and went; the file must not grow while the number of live pages does not");
    }
}

#[test]
fn s1a_02_space_freed_by_deleting_everything_is_reused_by_new_pages() {
    let dir = TempDir::new("reuse-all");
    let dm = new_dm(&dir);
    for id in 0..100 {
        dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
    }
    for id in 0..100 {
        dm.delete_page(PageId(id));
    }
    let size = dm.get_db_file_size();
    for id in 1000..1100 {
        dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
    }
    assert_eq!(dm.get_db_file_size(), size, "100 new pages fit in the space of the 100 deleted ones: the file must not grow");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1a_02_the_disk_behaves_like_a_map_with_deletes(ops in ops(24, true)) {
        let dir = TempDir::new("prop2");
        let dm = new_dm(&dir);
        agrees_with_the_model(&dm, &ops)?;
    }

    #[test]
    fn s1a_02_the_file_stays_bounded_by_the_peak_number_of_live_pages(ops in ops(80, true)) {
        let dir = TempDir::new("prop2b");
        let dm = new_dm(&dir);
        let mut live: HashSet<i32> = HashSet::new();
        let mut peak = 0usize;
        for (step, op) in ops.iter().enumerate() {
            match *op {
                Op::Write(p, s) => { dm.write_page(PageId(p), &pattern(s as usize)).unwrap(); live.insert(p); }
                Op::Delete(p) => { dm.delete_page(PageId(p)); live.remove(&p); }
                Op::Read(p) => { dm.read_page(PageId(p), &mut [0u8; PS]).unwrap(); }
            }
            peak = peak.max(live.len());
            prop_assert!(dm.get_db_file_size() <= size_bound(peak), "step {step}: the file is {} bytes but at most {peak} pages were ever live at once, so it should not exceed {} bytes", dm.get_db_file_size(), size_bound(peak));
        }
    }
}

// ---- 1a-03 · Counters and the log --------------------------------------------------------------------------------

fn log_bytes(chunks: &[Vec<u8>]) -> Vec<u8> {
    chunks.iter().flatten().copied().collect()
}

#[test]
fn s1a_03_every_page_write_is_counted_including_rewrites() {
    let dir = TempDir::new("c-writes");
    let dm = new_dm(&dir);
    assert_eq!(dm.get_num_writes(), 0, "no writes yet");
    dm.write_page(PageId(1), &pattern(1)).unwrap();
    dm.write_page(PageId(1), &pattern(2)).unwrap();
    dm.write_page(PageId(2), &pattern(3)).unwrap();
    assert_eq!(dm.get_num_writes(), 3, "a rewrite is a write");
}

#[test]
fn s1a_03_only_deletes_of_existing_pages_are_counted() {
    let dir = TempDir::new("c-deletes");
    let dm = new_dm(&dir);
    dm.write_page(PageId(1), &pattern(1)).unwrap();
    dm.delete_page(PageId(1));
    dm.delete_page(PageId(1));
    dm.delete_page(PageId(77));
    assert_eq!(dm.get_num_deletes(), 1, "deleting a page that is not there counts for nothing");
}

#[test]
fn s1a_03_an_empty_log_write_is_not_a_flush_and_changes_nothing() {
    let dir = TempDir::new("c-empty");
    let dm = new_dm(&dir);
    dm.write_log(&[]).unwrap();
    assert_eq!(dm.get_num_flushes(), 0, "an empty write is not a flush");
    let mut buf = [0xEEu8; 4];
    assert!(!dm.read_log(&mut buf, 0).unwrap(), "the log is still empty, so a read at 0 is past its end");
}

#[test]
fn s1a_03_each_non_empty_log_write_counts_one_flush() {
    let dir = TempDir::new("c-flush");
    let dm = new_dm(&dir);
    dm.write_log(b"one").unwrap();
    dm.write_log(&[]).unwrap();
    dm.write_log(b"two").unwrap();
    assert_eq!(dm.get_num_flushes(), 2, "two non-empty writes, one empty one");
}

#[test]
fn s1a_03_reading_the_log_at_or_past_its_end_returns_false_and_leaves_the_buffer_alone() {
    let dir = TempDir::new("c-past");
    let dm = new_dm(&dir);
    dm.write_log(b"12345").unwrap();
    let mut buf = [0xEEu8; 8];
    assert!(!dm.read_log(&mut buf, 5).unwrap(), "offset 5 is exactly the end of a 5-byte log");
    assert!(!dm.read_log(&mut buf, 500).unwrap(), "far past the end is also false");
    assert_eq!(buf, [0xEE; 8], "a read past the end must not touch the buffer");
}

#[test]
fn s1a_03_a_read_running_off_the_end_of_the_log_is_zero_filled() {
    let dir = TempDir::new("c-short");
    let dm = new_dm(&dir);
    dm.write_log(b"abcdef").unwrap();
    let mut buf = [0xEEu8; 8];
    assert!(dm.read_log(&mut buf, 4).unwrap(), "offset 4 is inside the log");
    assert_eq!(&buf, b"ef\0\0\0\0\0\0", "the two bytes the log has, then zeros");
}

#[test]
fn s1a_03_the_log_file_on_disk_holds_exactly_what_was_appended_and_survives_reopening() {
    let dir = TempDir::new("c-reopen");
    {
        let dm = new_dm(&dir);
        dm.write_log(b"hello ").unwrap();
    }
    let dm = new_dm(&dir);
    dm.write_log(b"again").unwrap();
    assert_eq!(std::fs::read(dm.log_file_name()).unwrap(), b"hello again", "the log is appended to, never truncated by reopening");
    let mut buf = [0u8; 11];
    assert!(dm.read_log(&mut buf, 0).unwrap());
    assert_eq!(&buf, b"hello again", "read_log sees the earlier session's bytes too");
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s1a_03_the_log_reads_like_the_concatenation_of_what_was_appended(
        chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..40), 0..10),
        windows in prop::collection::vec((0u64..500, 1usize..64), 1..8),
    ) {
        let dir = TempDir::new("prop3");
        let dm = new_dm(&dir);
        for c in &chunks {
            dm.write_log(c).unwrap();
        }
        let all = log_bytes(&chunks);
        prop_assert_eq!(dm.get_num_flushes(), chunks.iter().filter(|c| !c.is_empty()).count(), "one flush per non-empty append");
        for (offset, len) in windows {
            let mut buf = vec![0xEEu8; len];
            let got = dm.read_log(&mut buf, offset).unwrap();
            if offset as usize >= all.len() {
                prop_assert!(!got, "offset {offset} is at or past the end of a {}-byte log", all.len());
                prop_assert!(buf.iter().all(|&b| b == 0xEE), "a read past the end must leave the buffer alone");
            } else {
                prop_assert!(got, "offset {offset} is inside the {}-byte log", all.len());
                let mut want = vec![0u8; len];
                let have = &all[offset as usize..];
                let n = have.len().min(len);
                want[..n].copy_from_slice(&have[..n]);
                prop_assert_eq!(buf, want, "window at {} of length {} should be the log bytes, zero-filled past the end", offset, len);
            }
        }
    }

    #[test]
    fn s1a_03_the_counters_match_what_was_done(ops in ops(24, true)) {
        let dir = TempDir::new("prop3b");
        let dm = new_dm(&dir);
        let (mut writes, mut deletes) = (0usize, 0usize);
        let mut live: HashSet<i32> = HashSet::new();
        for op in &ops {
            match *op {
                Op::Write(p, s) => { dm.write_page(PageId(p), &pattern(s as usize)).unwrap(); writes += 1; live.insert(p); }
                Op::Delete(p) => { dm.delete_page(PageId(p)); if live.remove(&p) { deletes += 1; } }
                Op::Read(p) => { dm.read_page(PageId(p), &mut [0u8; PS]).unwrap(); }
            }
        }
        prop_assert_eq!(dm.get_num_writes(), writes, "writes counted");
        prop_assert_eq!(dm.get_num_deletes(), deletes, "deletes of existing pages counted");
    }
}

// ---- 1a-04 · One interface, three disks -------------------------------------------------------------------------

#[test]
fn s1a_04_a_fixed_memory_disk_refuses_page_ids_beyond_its_capacity() {
    let disk = DiskManagerMemory::new(4);
    disk.write_page(PageId(3), &pattern(1)).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| disk.write_page(PageId(4), &pattern(2))));
    assert!(result.is_err(), "a disk of 4 pages has no page 4: writing it is a bug in the caller, so it panics");
}

#[test]
fn s1a_04_copy_page_copies_one_page_id_to_another_through_any_disk() {
    let dir = TempDir::new("copy");
    let file = new_dm(&dir);
    let fixed = DiskManagerMemory::new(8);
    let unlimited = DiskManagerUnlimitedMemory::new();
    let disks: Vec<(&str, &dyn DiskIo)> = vec![("file", &file), ("fixed memory", &fixed), ("unlimited memory", &unlimited)];
    for (name, disk) in disks {
        disk.write_page(PageId(1), &pattern(11)).unwrap();
        copy_page(disk, PageId(1), PageId(5)).unwrap();
        let mut buf = ZERO;
        disk.read_page(PageId(5), &mut buf).unwrap();
        assert!(buf == pattern(11), "{name} disk: page 5 should now hold page 1's bytes");
        disk.read_page(PageId(1), &mut buf).unwrap();
        assert!(buf == pattern(11), "{name} disk: copying must leave the source page as it was");
    }
}

#[test]
fn s1a_04_the_memory_disks_count_writes_and_report_their_usage() {
    let fixed = DiskManagerMemory::new(8);
    let unlimited = DiskManagerUnlimitedMemory::new();
    for id in [0, 1, 1, 6] {
        fixed.write_page(PageId(id), &pattern(1)).unwrap();
        unlimited.write_page(PageId(id), &pattern(1)).unwrap();
    }
    assert_eq!(fixed.get_num_writes(), 4, "a fixed memory disk counts every write, rewrites included");
    assert_eq!(unlimited.get_num_writes(), 4, "an unlimited memory disk counts every write, rewrites included");
    assert_eq!(unlimited.get_memory_usage(), 3 * PS, "three distinct pages exist: 0, 1 and 6");
}

#[test]
fn s1a_04_an_unlimited_memory_disk_reads_zeros_and_never_creates_a_page_by_reading() {
    let disk = DiskManagerUnlimitedMemory::new();
    let mut buf = [0xEEu8; PS];
    disk.read_page(PageId(500), &mut buf).unwrap();
    assert!(buf == ZERO, "a page never written reads as zeros");
    assert_eq!(disk.get_memory_usage(), 0, "reading must not create the page");
}

proptest! {
    #![proptest_config(config())]

    /// The covariant of this stage: three very different disks, one behaviour.
    #[test]
    fn s1a_04_every_disk_agrees_with_the_same_model(ops in ops(32, true)) {
        let dir = TempDir::new("prop4");
        let file = new_dm(&dir);
        let fixed = DiskManagerMemory::new(32);
        let unlimited = DiskManagerUnlimitedMemory::new();
        agrees_with_the_model(&Dyn(&file), &ops)?;
        agrees_with_the_model(&Dyn(&fixed), &ops)?;
        agrees_with_the_model(&Dyn(&unlimited), &ops)?;
    }
}

// ---- 1a-05 · Boss: many threads, a clean shutdown, BusTub's own tests -------------------------------------------

#[test]
fn s1a_05_threads_writing_their_own_pages_all_read_them_back() {
    let dir = TempDir::new("mt-own");
    let dm = Arc::new(new_dm(&dir));
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let dm = Arc::clone(&dm);
            thread::spawn(move || {
                for i in 0..40 {
                    let id = (t * 40 + i) as i32;
                    dm.write_page(PageId(id), &pattern(id as usize)).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let mut buf = ZERO;
    for id in 0..320 {
        dm.read_page(PageId(id), &mut buf).unwrap();
        assert!(buf == pattern(id as usize), "page {id} lost or mixed up when 8 threads wrote at once");
    }
    assert_eq!(dm.get_num_writes(), 320, "every write from every thread is counted exactly once");
}

#[test]
fn s1a_05_a_reader_never_sees_a_page_that_is_half_one_write_and_half_another() {
    let dir = TempDir::new("mt-torn");
    let dm = Arc::new(new_dm(&dir));
    dm.write_page(PageId(0), &pattern(1)).unwrap();
    let stop = Arc::new(AtomicUsize::new(0));
    let writer = {
        let (dm, stop) = (Arc::clone(&dm), Arc::clone(&stop));
        thread::spawn(move || {
            for i in 0..2000 {
                dm.write_page(PageId(0), &pattern(1 + (i % 2))).unwrap();
            }
            stop.store(1, Ordering::SeqCst);
        })
    };
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let (dm, stop) = (Arc::clone(&dm), Arc::clone(&stop));
            thread::spawn(move || {
                let mut buf = ZERO;
                while stop.load(Ordering::SeqCst) == 0 {
                    dm.read_page(PageId(0), &mut buf).unwrap();
                    assert!(buf == pattern(1) || buf == pattern(2), "a read saw a torn page: neither of the two versions ever written");
                }
            })
        })
        .collect();
    writer.join().unwrap();
    for r in readers {
        r.join().unwrap();
    }
}

#[test]
fn s1a_05_concurrent_writes_deletes_and_log_appends_keep_the_counters_exact() {
    let dir = TempDir::new("mt-count");
    let dm = Arc::new(new_dm(&dir));
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let dm = Arc::clone(&dm);
            thread::spawn(move || {
                for i in 0..50 {
                    let id = (t * 100 + i) as i32;
                    dm.write_page(PageId(id), &pattern(i)).unwrap();
                    dm.delete_page(PageId(id));
                    dm.write_log(&[t as u8; 5]).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!((dm.get_num_writes(), dm.get_num_deletes(), dm.get_num_flushes()), (200, 200, 200), "writes, deletes and flushes, all exact");
    let mut buf = vec![0u8; 1000];
    assert!(dm.read_log(&mut buf, 0).unwrap());
    assert!(buf.iter().all(|&b| b < 4), "1000 bytes were appended, each from one of the four threads");
}

#[test]
fn s1a_05_shut_down_makes_the_log_available_to_the_next_manager() {
    let dir = TempDir::new("shutdown");
    {
        let dm = new_dm(&dir);
        dm.write_page(PageId(2), &pattern(9)).unwrap();
        dm.write_log(b"commit 1").unwrap();
        dm.shut_down().unwrap();
    }
    let dm = new_dm(&dir);
    let mut buf = [0u8; 8];
    assert!(dm.read_log(&mut buf, 0).unwrap());
    assert_eq!(&buf, b"commit 1", "the log written before shut_down is there when the same files are opened again");
}

// ---- 1a-c1 and 1a-c2: challenges ------------------------------------------------------------------------------------------------------------

use bustub::storage::disk::cached_disk::CachedDisk;
use bustub::storage::disk::mirrored_disk::MirroredDisk;
use std::sync::atomic::AtomicBool;

/// A memory disk (pages in a map; a deleted page reads as zeros) that can be switched to failing every read and write, and counts the reads that reach it.
struct Flaky {
    inner: std::sync::Mutex<HashMap<PageId, PageData>>,
    down: AtomicBool,
    reads: AtomicUsize,
}

impl Flaky {
    fn new() -> Arc<Flaky> {
        Arc::new(Flaky { inner: Default::default(), down: AtomicBool::new(false), reads: AtomicUsize::new(0) })
    }
    fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }
    fn is_down(&self) -> bool {
        self.down.load(Ordering::SeqCst)
    }
}

impl DiskIo for Flaky {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> std::io::Result<()> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.is_down() {
            return Err(std::io::Error::other("disk is down"));
        }
        *buf = self.inner.lock().unwrap().get(&page_id).copied().unwrap_or(ZERO);
        Ok(())
    }
    fn write_page(&self, page_id: PageId, data: &PageData) -> std::io::Result<()> {
        if self.is_down() {
            return Err(std::io::Error::other("disk is down"));
        }
        self.inner.lock().unwrap().insert(page_id, *data);
        Ok(())
    }
    fn delete_page(&self, page_id: PageId) {
        self.inner.lock().unwrap().remove(&page_id);
    }
}

fn read(disk: &dyn DiskIo, id: PageId) -> PageData {
    let mut buf = ZERO;
    disk.read_page(id, &mut buf).expect("the read works");
    buf
}

#[test]
fn s1a_c1_a_mirror_acts_like_one_disk_when_both_work() {
    let (a, b) = (Flaky::new(), Flaky::new());
    let m = MirroredDisk::new(a.clone(), b.clone());
    m.write_page(PageId(3), &pattern(3)).unwrap();
    m.write_page(PageId(9), &pattern(9)).unwrap();
    assert_eq!(read(&m, PageId(3)), pattern(3));
    assert_eq!(read(&m, PageId(9)), pattern(9));
    assert_eq!(read(&m, PageId(5)), ZERO, "a page never written reads as zeros");
    m.delete_page(PageId(3));
    assert_eq!(read(&m, PageId(3)), ZERO, "deleted pages read as zeros");
    assert_eq!(read(&*a, PageId(9)), pattern(9), "both copies hold the write");
    assert_eq!(read(&*b, PageId(9)), pattern(9), "both copies hold the write");
}

#[test]
fn s1a_c1_reads_survive_either_disk_failing() {
    let (a, b) = (Flaky::new(), Flaky::new());
    let m = MirroredDisk::new(a.clone(), b.clone());
    m.write_page(PageId(1), &pattern(1)).unwrap();
    a.set_down(true);
    assert_eq!(read(&m, PageId(1)), pattern(1), "the primary is down: the secondary answers");
    a.set_down(false);
    b.set_down(true);
    assert_eq!(read(&m, PageId(1)), pattern(1), "the secondary is down: the primary answers");
}

#[test]
fn s1a_c1_a_write_while_one_disk_is_down_still_succeeds() {
    let (a, b) = (Flaky::new(), Flaky::new());
    let m = MirroredDisk::new(a.clone(), b.clone());
    a.set_down(true);
    m.write_page(PageId(2), &pattern(2)).expect("one disk is enough");
    a.set_down(false);
    assert_eq!(read(&m, PageId(2)), pattern(2));
    b.set_down(true);
    assert_eq!(read(&m, PageId(2)), pattern(2), "the first disk came back and now holds the page too");
}

#[test]
fn s1a_c1_a_disk_that_missed_a_write_never_serves_the_old_page() {
    let (a, b) = (Flaky::new(), Flaky::new());
    let m = MirroredDisk::new(a.clone(), b.clone());
    m.write_page(PageId(4), &pattern(1)).unwrap();
    a.set_down(true);
    m.write_page(PageId(4), &pattern(2)).unwrap();
    a.set_down(false);
    assert_eq!(read(&m, PageId(4)), pattern(2), "the primary is back but has the old page: the answer is the latest write");
    b.set_down(true);
    assert_eq!(read(&m, PageId(4)), pattern(2), "and the primary has been brought up to date");
}

#[test]
fn s1a_c1_a_write_fails_only_when_both_disks_fail() {
    let (a, b) = (Flaky::new(), Flaky::new());
    let m = MirroredDisk::new(a.clone(), b.clone());
    a.set_down(true);
    b.set_down(true);
    assert!(m.write_page(PageId(1), &pattern(1)).is_err());
    let mut buf = ZERO;
    assert!(m.read_page(PageId(1), &mut buf).is_err());
}

#[derive(Clone, Debug)]
enum MOp {
    Write(u32, u8),
    Read(u32),
    Delete(u32),
    /// Switch a disk (0 or 1) off or on; the other disk is never switched off at the same time.
    Toggle(u8),
}

fn mirror_ops() -> impl Strategy<Value = Vec<MOp>> {
    proptest::collection::vec(
        prop_oneof![
            4 => (0..5u32, 1..250u8).prop_map(|(p, b)| MOp::Write(p, b)),
            4 => (0..5u32).prop_map(MOp::Read),
            1 => (0..5u32).prop_map(MOp::Delete),
            2 => (0..2u8).prop_map(MOp::Toggle),
        ],
        1..60,
    )
}

proptest! {
    #![proptest_config(config())]

    /// Property: with at most one disk down at any moment, every read returns the latest successful write of the page (zeros if none or deleted). When a disk comes back, every page is read once, which is when it is brought up to date.
    #[test]
    fn s1a_c1_property_a_mirror_survives_any_single_disk_failing(ops in mirror_ops()) {
        let (a, b) = (Flaky::new(), Flaky::new());
        let m = MirroredDisk::new(a.clone(), b.clone());
        let mut model: HashMap<u32, PageData> = HashMap::new();
        for op in ops {
            match op {
                MOp::Write(p, byte) => {
                    let data = [byte; PS];
                    prop_assert!(m.write_page(PageId(p as i32), &data).is_ok());
                    model.insert(p, data);
                }
                MOp::Read(p) => {
                    let mut buf = ZERO;
                    prop_assert!(m.read_page(PageId(p as i32), &mut buf).is_ok(), "a read with one disk down must work");
                    prop_assert_eq!(buf[..8].to_vec(), model.get(&p).unwrap_or(&ZERO)[..8].to_vec());
                    prop_assert_eq!(buf == ZERO, !model.contains_key(&p));
                }
                MOp::Delete(p) => {
                    m.delete_page(PageId(p as i32));
                    model.remove(&p);
                }
                MOp::Toggle(which) => {
                    let (x, other) = if which == 0 { (&a, &b) } else { (&b, &a) };
                    if x.is_down() {
                        x.set_down(false);
                        // the disk is back: the reads that follow are what bring it up to date, so read every page once
                        for q in 0..5u32 {
                            let mut buf = ZERO;
                            prop_assert!(m.read_page(PageId(q as i32), &mut buf).is_ok());
                            prop_assert_eq!(buf == ZERO, !model.contains_key(&q));
                        }
                    } else if !other.is_down() {
                        x.set_down(true);
                    }
                }
            }
        }
    }
}

#[test]
fn s1a_c2_a_cache_hit_does_not_reach_the_disk() {
    let inner = Flaky::new();
    let c = CachedDisk::new(inner.clone(), 4);
    c.write_page(PageId(1), &pattern(1)).unwrap();
    let before = inner.reads.load(Ordering::SeqCst);
    assert_eq!(read(&c, PageId(1)), pattern(1));
    assert_eq!(read(&c, PageId(1)), pattern(1));
    assert_eq!(inner.reads.load(Ordering::SeqCst), before, "a page just written is served from memory");
}

#[test]
fn s1a_c2_a_full_cache_forgets_the_oldest_page_but_never_loses_data() {
    let inner = Flaky::new();
    let c = CachedDisk::new(inner.clone(), 2);
    for p in 0..5i32 {
        c.write_page(PageId(p), &pattern(p as usize)).unwrap();
    }
    for p in 0..5i32 {
        assert_eq!(read(&c, PageId(p)), pattern(p as usize), "page {p}");
    }
}

#[test]
fn s1a_c2_a_deleted_page_reads_as_zeros() {
    let inner = Flaky::new();
    let c = CachedDisk::new(inner.clone(), 4);
    c.write_page(PageId(7), &pattern(7)).unwrap();
    assert_eq!(read(&c, PageId(7)), pattern(7));
    c.delete_page(PageId(7));
    assert_eq!(read(&c, PageId(7)), ZERO, "after a delete the page is gone, cached or not");
}

#[test]
fn s1a_c2_the_cache_and_the_disk_underneath_agree() {
    let inner = Flaky::new();
    let c = CachedDisk::new(inner.clone(), 3);
    c.write_page(PageId(1), &pattern(1)).unwrap();
    c.write_page(PageId(1), &pattern(2)).unwrap();
    assert_eq!(read(&c, PageId(1)), pattern(2), "a rewrite replaces the cached page");
    assert_eq!(read(&*inner, PageId(1)), pattern(2), "write-through: the disk is current");
    c.delete_page(PageId(1));
    assert_eq!(read(&*inner, PageId(1)), ZERO);
}

#[derive(Clone, Debug)]
enum COp {
    Write(u32, u8),
    Read(u32),
    Delete(u32),
}

proptest! {
    #![proptest_config(config())]

    /// Property: a cache is invisible: every answer equals that of a plain map of pages, whatever the capacity.
    #[test]
    fn s1a_c2_property_a_cached_disk_behaves_like_the_disk(capacity in 1usize..5, ops in proptest::collection::vec(
        prop_oneof![
            4 => (0..5u32, 1..250u8).prop_map(|(p, b)| COp::Write(p, b)),
            5 => (0..5u32).prop_map(COp::Read),
            2 => (0..5u32).prop_map(COp::Delete),
        ], 1..60)) {
        let c = CachedDisk::new(Flaky::new(), capacity);
        let mut model: HashMap<u32, PageData> = HashMap::new();
        for op in ops {
            match op {
                COp::Write(p, b) => {
                    let data = [b; PS];
                    c.write_page(PageId(p as i32), &data).unwrap();
                    model.insert(p, data);
                }
                COp::Read(p) => {
                    let got = read(&c, PageId(p as i32));
                    prop_assert_eq!(got == *model.get(&p).unwrap_or(&ZERO), true, "read of page {} differs from the model", p);
                }
                COp::Delete(p) => {
                    c.delete_page(PageId(p as i32));
                    model.remove(&p);
                }
            }
        }
    }
}

// @@ challenge 1a-c3 begin
mod ch_1a_c3 {
    use proptest::prelude::*;

    use bustub::storage::disk::page_bitmap::{FreeError, PageBitmap};
    use std::collections::BTreeSet;

    #[test]
    fn s1a_c3_pages_are_handed_out_in_increasing_order_until_none_is_left() {
        let mut b = PageBitmap::new(3);
        assert_eq!((b.allocate(), b.allocate(), b.allocate(), b.allocate()), (Some(0), Some(1), Some(2), None));
        assert_eq!(b.used(), 3);
    }

    #[test]
    fn s1a_c3_the_lowest_free_page_is_reused_first() {
        let mut b = PageBitmap::new(5);
        for _ in 0..5 {
            b.allocate();
        }
        b.free(3).unwrap();
        b.free(1).unwrap();
        assert_eq!(b.allocate(), Some(1));
        assert_eq!(b.allocate(), Some(3));
        assert_eq!(b.allocate(), None);
    }

    #[test]
    fn s1a_c3_a_double_free_and_an_out_of_range_free_are_refused() {
        let mut b = PageBitmap::new(4);
        let p = b.allocate().unwrap();
        assert_eq!(b.free(p), Ok(()));
        assert_eq!(b.free(p), Err(FreeError::DoubleFree));
        assert_eq!(b.free(2), Err(FreeError::DoubleFree), "never allocated, so already free");
        assert_eq!(b.free(4), Err(FreeError::OutOfRange));
        assert_eq!(b.used(), 0, "errors change nothing");
        assert_eq!((b.allocate(), b.allocate()), (Some(0), Some(1)), "and no page is handed out twice after a refused free");
    }

    #[test]
    fn s1a_c3_is_allocated_tracks_the_state() {
        let mut b = PageBitmap::new(2);
        assert!(!b.is_allocated(0) && !b.is_allocated(9));
        b.allocate();
        assert!(b.is_allocated(0) && !b.is_allocated(1));
    }

    #[test]
    fn s1a_c3_an_empty_bitmap_has_nothing_to_give() {
        let mut b = PageBitmap::new(0);
        assert_eq!(b.allocate(), None);
        assert_eq!(b.free(0), Err(FreeError::OutOfRange));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a set of free pages, for any sequence of allocations and frees (including wrong ones).
        #[test]
        fn s1a_c3_property_a_bitmap_matches_a_set_of_free_pages(capacity in 0usize..12, ops in proptest::collection::vec(prop_oneof![Just(None), (0usize..14).prop_map(Some)], 0..60)) {
            let mut b = PageBitmap::new(capacity);
            let mut free: BTreeSet<usize> = (0..capacity).collect();
            for op in ops {
                match op {
                    None => {
                        let want = free.iter().next().copied();
                        prop_assert_eq!(b.allocate(), want);
                        if let Some(p) = want { free.remove(&p); }
                    }
                    Some(p) => {
                        let want = if p >= capacity { Err(FreeError::OutOfRange) } else if free.contains(&p) { Err(FreeError::DoubleFree) } else { Ok(()) };
                        if want.is_ok() { free.insert(p); }
                        prop_assert_eq!(b.free(p), want);
                    }
                }
                prop_assert_eq!(b.used(), capacity - free.len());
            }
        }
    }
}
// @@ challenge 1a-c3 end

// @@ challenge 1a-c4 begin
mod ch_1a_c4 {
    use proptest::prelude::*;

    use bustub::storage::disk::versioned_pages::VersionedPages;
    use std::collections::HashMap;

    #[test]
    fn s1a_c4_a_snapshot_keeps_the_value_it_saw() {
        let mut s = VersionedPages::new();
        s.write(1, 10);
        let snap = s.snapshot();
        s.write(1, 20);
        assert_eq!((s.read(1), s.read_at(snap, 1)), (Some(20), Some(10)));
    }

    #[test]
    fn s1a_c4_a_page_written_after_the_snapshot_is_absent_from_it() {
        let mut s = VersionedPages::new();
        let snap = s.snapshot();
        s.write(2, 5);
        assert_eq!((s.read(2), s.read_at(snap, 2)), (Some(5), None));
    }

    #[test]
    fn s1a_c4_snapshots_taken_at_different_times_see_different_pasts() {
        let mut s = VersionedPages::new();
        s.write(0, 1);
        let a = s.snapshot();
        s.write(0, 2);
        let b = s.snapshot();
        s.write(0, 3);
        assert_eq!((s.read_at(a, 0), s.read_at(b, 0), s.read(0)), (Some(1), Some(2), Some(3)));
    }

    #[test]
    fn s1a_c4_dropping_one_snapshot_leaves_the_others_alone() {
        let mut s = VersionedPages::new();
        s.write(0, 1);
        let a = s.snapshot();
        let b = s.snapshot();
        assert!(s.drop_snapshot(a));
        assert!(!s.drop_snapshot(a), "already dropped");
        assert!(!s.drop_snapshot(99));
        assert_eq!(s.read_at(a, 0), None);
        assert_eq!(s.read_at(b, 0), Some(1));
        let c = s.snapshot();
        assert!(c != a && c != b, "ids are never reused");
    }

    #[test]
    fn s1a_c4_two_snapshots_with_nothing_written_between_them_agree() {
        let mut s = VersionedPages::new();
        for p in 0..5 {
            s.write(p, p as u64 * 7);
        }
        let (a, b) = (s.snapshot(), s.snapshot());
        for p in 0..6 {
            assert_eq!(s.read_at(a, p), s.read_at(b, p));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a model that copies the whole map at every snapshot.
        #[test]
        fn s1a_c4_property_snapshots_equal_copies(ops in proptest::collection::vec((0u8..4, 0u32..6, any::<u64>()), 0..60)) {
            let mut s = VersionedPages::new();
            let mut now: HashMap<u32, u64> = HashMap::new();
            let mut copies: Vec<(u64, Option<HashMap<u32, u64>>)> = Vec::new();
            for (op, page, value) in ops {
                match op {
                    0 | 1 => { s.write(page, value); now.insert(page, value); }
                    2 => { let id = s.snapshot(); copies.push((id, Some(now.clone()))); }
                    _ => {
                        let idx = (value as usize) % copies.len().max(1);
                        if let Some(c) = copies.get_mut(idx) {
                            prop_assert_eq!(s.drop_snapshot(c.0), c.1.is_some());
                            c.1 = None;
                        }
                    }
                }
                for (id, copy) in &copies {
                    for p in 0..6 {
                        prop_assert_eq!(s.read_at(*id, p), copy.as_ref().and_then(|m| m.get(&p).copied()));
                    }
                }
                for p in 0..6 { prop_assert_eq!(s.read(p), now.get(&p).copied()); }
            }
        }
    }
}
// @@ challenge 1a-c4 end

// @@ challenge 1a-c5 begin
mod ch_1a_c5 {
    use proptest::prelude::*;

    use bustub::storage::disk::slot_allocator::SlotAllocator;
    use std::collections::HashSet;

    #[test]
    fn s1a_c5_slots_are_handed_out_and_reused() {
        let mut a = SlotAllocator::new();
        assert_eq!((a.allocate(), a.allocate(), a.allocate()), (0, 1, 2));
        assert!(a.free(1));
        assert_eq!(a.allocate(), 1, "a freed slot is reused before a fresh one");
        assert_eq!(a.in_use(), 3);
    }

    #[test]
    fn s1a_c5_freeing_twice_is_the_same_as_once() {
        let mut a = SlotAllocator::new();
        a.allocate();
        a.allocate();
        assert!(a.free(0));
        assert!(!a.free(0), "already free");
        assert_eq!(a.in_use(), 1);
        let (x, y) = (a.allocate(), a.allocate());
        assert_ne!(x, y, "the freed slot must not be handed out twice");
    }

    #[test]
    fn s1a_c5_a_slot_that_was_never_handed_out_cannot_be_freed() {
        let mut a = SlotAllocator::new();
        assert!(!a.free(5));
        assert_eq!(a.in_use(), 0);
        assert_eq!(a.allocate(), 0, "and it does not enter the free list");
    }

    #[test]
    fn s1a_c5_in_use_counts_allocations_minus_frees() {
        let mut a = SlotAllocator::new();
        let s: Vec<_> = (0..5).map(|_| a.allocate()).collect();
        for &x in &s[..3] {
            a.free(x);
            a.free(x);
        }
        assert_eq!(a.in_use(), 2);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: whatever mix of allocations and frees (right and wrong), no slot is ever in use twice and the count is right.
        #[test]
        fn s1a_c5_property_no_slot_is_in_use_twice(ops in proptest::collection::vec(prop_oneof![Just(None), (0usize..8).prop_map(Some)], 0..80)) {
            let mut a = SlotAllocator::new();
            let mut live: HashSet<usize> = HashSet::new();
            for op in ops {
                match op {
                    None => {
                        let s = a.allocate();
                        prop_assert!(live.insert(s), "slot {} handed out while still in use", s);
                    }
                    Some(s) => {
                        prop_assert_eq!(a.free(s), live.remove(&s));
                    }
                }
                prop_assert_eq!(a.in_use(), live.len());
            }
        }
    }
}
// @@ challenge 1a-c5 end
