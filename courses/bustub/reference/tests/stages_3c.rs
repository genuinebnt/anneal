//! Tests for module 3c, the table heap, its iterator, indexes over tuples and the catalog. A test name starts with its stage: `s3c_02_…`
//! belongs to stage 3c-02, and `anneal course test` runs just those. Each stage also has a property that runs random operations against a
//! plain model: a `Vec` of what the table should hold, a `BTreeMap` for an index, a `HashMap` for the catalog.

use std::sync::Arc;
use std::thread;

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::catalog::catalog::Catalog;
use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::config::{PageId, BUSTUB_PAGE_SIZE};
use bustub::common::rid::Rid;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;
use bustub::storage::index::generic_key::KeyComparator;
use bustub::storage::index::index::{generic_key_from_tuple, BPlusTreeIndex, Index, IndexMetadata, IndexType, SchemaComparator};
use bustub::storage::page::table_page::TablePage;
use bustub::storage::table::table_heap::TableHeap;
use bustub::storage::table::tuple::{Tuple, TupleMeta};
use bustub::types::type_id::TypeId::*;
use bustub::types::value::Value;
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn bpm(frames: usize) -> BufferPoolManager {
    BufferPoolManager::new(frames, Arc::new(DiskManagerUnlimitedMemory::new()))
}

fn meta(is_deleted: bool) -> TupleMeta {
    TupleMeta { ts: 0, is_deleted }
}

fn bytes_tuple(byte: u8, len: usize) -> Tuple {
    Tuple::from_bytes(Rid::default(), &vec![byte; len])
}

/// A tuple read straight from its page (so the insert tests do not depend on `get_tuple`).
fn raw_get(bpm: &BufferPoolManager, rid: Rid) -> (TupleMeta, Tuple) {
    TablePage::new(&bpm.read_page(rid.page_id())[..]).get_tuple(rid).unwrap()
}

fn heap_test_schema() -> Schema {
    Schema::new(vec![
        Column::new_varchar("a", 20),
        Column::new("b", SmallInt),
        Column::new("c", BigInt),
        Column::new("d", Boolean),
        Column::new_varchar("e", 16),
    ])
}

fn construct_tuple(schema: &Schema, i: usize) -> Tuple {
    Tuple::new(
        &[
            Value::varchar(&format!("row{i}")),
            Value::smallint((i % 100) as i16),
            Value::bigint(i as i64),
            Value::boolean(i % 2 == 0),
            Value::varchar("tail"),
        ],
        schema,
    )
}

// ---- 3c-01 · A table heap: inserting ------------------------------------------------------------------------------------------------

#[test]
fn s3c_01_a_new_heap_has_an_empty_first_page() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let first = heap.get_first_page_id();
    assert!(first.is_valid(), "a new heap has an empty first page: expected `first.is_valid()`");
    let guard = bpm.read_page(first);
    let page = TablePage::new(&guard[..]);
    assert_eq!((page.get_num_tuples(), page.get_next_page_id()), (0, None), "the page was formatted, not left as zeros");
}

#[test]
fn s3c_01_inserted_tuples_get_consecutive_slots_of_the_first_page() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let first = heap.get_first_page_id();
    for i in 0..5 {
        let rid = heap.insert_tuple(&meta(false), &bytes_tuple(i as u8, 20)).unwrap();
        assert_eq!(rid, Rid::new(first, i), "tuple {i}");
        let (m, t) = raw_get(&bpm, rid);
        assert_eq!((m, t.data()), (meta(false), vec![i as u8; 20].as_slice()), "inserted tuples get consecutive slots of the first page");
    }
}

#[test]
fn s3c_01_a_full_page_makes_the_heap_start_and_link_a_new_one() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let first = heap.get_first_page_id();
    // 66 tuples of 100 bytes fill a page exactly (module 3b)
    let rids: Vec<Rid> = (0..70).map(|i| heap.insert_tuple(&meta(false), &bytes_tuple(i as u8, 100)).unwrap()).collect();
    assert!(
        rids[..66].iter().all(|r| r.page_id() == first),
        "a full page makes the heap start and link a new one: expected `rids[..66].iter().all(|r| r.page_id() == first)`"
    );
    let second = rids[66].page_id();
    assert_ne!(second, first, "a full page makes the heap start and link a new one");
    assert_eq!(
        rids[66..].iter().map(|r| (r.page_id(), r.slot_num())).collect::<Vec<_>>(),
        (0..4).map(|s| (second, s)).collect::<Vec<_>>(),
        "slots restart at 0 in the new page"
    );
    assert_eq!(TablePage::new(&bpm.read_page(first)[..]).get_next_page_id(), Some(second), "the full page points at the new one");
    assert_eq!(TablePage::new(&bpm.read_page(second)[..]).get_next_page_id(), None, "a full page makes the heap start and link a new one");
    for (i, rid) in rids.iter().enumerate() {
        assert_eq!(raw_get(&bpm, *rid).1.data(), vec![i as u8; 100].as_slice(), "tuple {i}");
    }
}

#[test]
fn s3c_01_a_tuple_that_fits_no_page_is_an_error() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let biggest = BUSTUB_PAGE_SIZE - 8 - 24;
    assert!(heap.insert_tuple(&meta(false), &bytes_tuple(0, biggest + 1)).is_err(), "bigger than an empty page can take");
    assert!(
        heap.insert_tuple(&meta(false), &bytes_tuple(0, BUSTUB_PAGE_SIZE)).is_err(),
        "a tuple that fits no page is an error: expected `heap.insert_tuple(&meta(false), &bytes_tuple(0, BUSTUB_PAGE_SIZE)).is_err()`"
    );
    let rid = heap.insert_tuple(&meta(false), &bytes_tuple(7, biggest)).unwrap();
    assert_eq!(raw_get(&bpm, rid).1.get_length() as usize, biggest, "the largest tuple that fits");
    // after an error the heap still works, and the next tuple goes to a new page
    let next = heap.insert_tuple(&meta(false), &bytes_tuple(8, 10)).unwrap();
    assert_ne!(next.page_id(), rid.page_id(), "a tuple that fits no page is an error");
}

#[test]
fn s3c_01_a_refused_tuple_leaves_a_heap_with_tuples_as_it_was() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let first = heap.insert_tuple(&meta(false), &bytes_tuple(1, 100)).unwrap();
    assert!(
        heap.insert_tuple(&meta(false), &bytes_tuple(2, BUSTUB_PAGE_SIZE)).is_err(),
        "a tuple that fits no page is an error, also after other tuples"
    );
    assert_eq!(
        heap.make_iterator().count(),
        1,
        "a scan after the refused insert sees exactly the tuple that was there (no empty page left behind)"
    );
    let next = heap.insert_tuple(&meta(false), &bytes_tuple(3, 100)).unwrap();
    assert_eq!(next.page_id(), first.page_id(), "the refused tuple did not move the heap on to a new page");
    assert_eq!(heap.make_iterator().count(), 2, "the heap goes on working after the refused insert");
}

#[test]
fn s3c_01_a_tuple_too_big_for_the_rest_of_a_page_moves_on_to_a_new_one() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let a = heap.insert_tuple(&meta(false), &bytes_tuple(1, 5000)).unwrap();
    let b = heap.insert_tuple(&meta(false), &bytes_tuple(2, 5000)).unwrap();
    assert_ne!(a.page_id(), b.page_id(), "a tuple too big for the rest of a page moves on to a new one");
    let c = heap.insert_tuple(&meta(false), &bytes_tuple(3, 100)).unwrap();
    assert_eq!(c.page_id(), b.page_id(), "small tuples go on filling the last page (earlier pages are not revisited)");
}

#[test]
fn s3c_01_the_metadata_is_stored_with_the_tuple() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rid = heap.insert_tuple(&TupleMeta { ts: 42, is_deleted: true }, &bytes_tuple(1, 8)).unwrap();
    assert_eq!(raw_get(&bpm, rid).0, TupleMeta { ts: 42, is_deleted: true }, "the metadata is stored with the tuple");
}

#[test]
fn s3c_01_five_thousand_tuples_in_a_small_pool() {
    let bpm = bpm(50);
    let heap = TableHeap::new(&bpm);
    let schema = heap_test_schema();
    let rids: Vec<Rid> = (0..5000).map(|i| heap.insert_tuple(&meta(false), &construct_tuple(&schema, i)).unwrap()).collect();
    let distinct: std::collections::HashSet<_> = rids.iter().collect();
    assert_eq!(distinct.len(), 5000, "five thousand tuples in a small pool");
    for i in (0..5000).step_by(97) {
        assert_eq!(raw_get(&bpm, rids[i]).1.get_value(&schema, 2), Value::bigint(i as i64), "five thousand tuples in a small pool");
    }
    let pages: std::collections::HashSet<PageId> = rids.iter().map(|r| r.page_id()).collect();
    assert!(pages.len() > 10 && pages.len() < 100, "{} pages", pages.len());
}

#[test]
fn s3c_01_threads_inserting_at_once_never_share_a_slot() {
    let bpm = bpm(30);
    let heap = TableHeap::new(&bpm);
    let rids: Vec<Vec<Rid>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..4u8)
            .map(|t| {
                let heap = &heap;
                scope.spawn(move || {
                    (0..400).map(|i| heap.insert_tuple(&meta(false), &bytes_tuple(t, 20 + (i % 7))).unwrap()).collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let all: std::collections::HashSet<Rid> = rids.iter().flatten().copied().collect();
    assert_eq!(all.len(), 1600, "every rid is unique");
    for (t, mine) in rids.iter().enumerate() {
        for rid in mine {
            assert_eq!(raw_get(&bpm, *rid).1.data()[0], t as u8, "the tuple at {rid:?} is the one this thread inserted");
        }
    }
}

// ---- 3c-01 · Getting and updating -----------------------------------------------------------------------------------------------

#[test]
fn s3c_01_a_tuple_and_its_meta_come_back_by_rid() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rid = heap.insert_tuple(&TupleMeta { ts: 7, is_deleted: false }, &bytes_tuple(5, 30)).unwrap();
    let (m, t) = heap.get_tuple(rid).unwrap();
    assert_eq!(
        (m, t.data(), t.get_rid()),
        (TupleMeta { ts: 7, is_deleted: false }, vec![5u8; 30].as_slice(), rid),
        "a tuple and its meta come back by rid"
    );
    assert_eq!(heap.get_tuple_meta(rid).unwrap(), m, "a tuple and its meta come back by rid");
}

#[test]
fn s3c_01_a_bad_rid_is_an_error() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rid = heap.insert_tuple(&meta(false), &bytes_tuple(1, 8)).unwrap();
    let bad = Rid::new(rid.page_id(), rid.slot_num() + 1);
    assert!(heap.get_tuple(bad).is_err(), "a bad rid is an error: expected `heap.get_tuple(bad).is_err()`");
    assert!(heap.get_tuple_meta(bad).is_err(), "a bad rid is an error: expected `heap.get_tuple_meta(bad).is_err()`");
    assert!(
        heap.update_tuple_meta(&meta(true), bad).is_err(),
        "a bad rid is an error: expected `heap.update_tuple_meta(&meta(true), bad).is_err()`"
    );
    assert!(
        heap.update_tuple_in_place(&meta(true), &bytes_tuple(1, 8), bad, None).is_err(),
        "a bad rid is an error: expected `heap.update_tuple_in_place(&meta(true), &bytes_tuple(1, 8), bad, None).is_err()`"
    );
}

#[test]
fn s3c_01_marking_a_tuple_deleted_changes_only_its_meta() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rids: Vec<Rid> = (0..4).map(|i| heap.insert_tuple(&meta(false), &bytes_tuple(i, 12)).unwrap()).collect();
    heap.update_tuple_meta(&TupleMeta { ts: 3, is_deleted: true }, rids[2]).unwrap();
    assert_eq!(
        heap.get_tuple_meta(rids[2]).unwrap(),
        TupleMeta { ts: 3, is_deleted: true },
        "marking a tuple deleted changes only its meta"
    );
    assert_eq!(heap.get_tuple(rids[2]).unwrap().1.data(), vec![2u8; 12].as_slice(), "the bytes are still there");
    assert!(
        !heap.get_tuple_meta(rids[1]).unwrap().is_deleted,
        "marking a tuple deleted changes only its meta: expected `!heap.get_tuple_meta(rids[1]).unwrap().is_deleted`"
    );
}

#[test]
fn s3c_01_an_in_place_update_runs_its_check_under_the_latch() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rid = heap.insert_tuple(&TupleMeta { ts: 1, is_deleted: false }, &bytes_tuple(1, 16)).unwrap();
    // no check: always updates
    assert!(heap.update_tuple_in_place(&TupleMeta { ts: 2, is_deleted: false }, &bytes_tuple(2, 16), rid, None).unwrap(), "an in place update runs its check under the latch: expected `heap.update_tuple_in_place(&TupleMeta {{ ts: 2, is_deleted: false }}, &bytes_tuple(2, 16), rid, None)....`");
    assert_eq!(heap.get_tuple(rid).unwrap().1.data(), vec![2u8; 16].as_slice(), "an in place update runs its check under the latch");
    // a check that refuses: nothing changes
    let refuse = |_: &TupleMeta, _: &Tuple, _: Rid| false;
    assert!(!heap.update_tuple_in_place(&meta(true), &bytes_tuple(9, 16), rid, Some(&refuse)).unwrap(), "an in place update runs its check under the latch: expected `!heap.update_tuple_in_place(&meta(true), &bytes_tuple(9, 16), rid, Some(&refuse)).unwrap()`");
    assert_eq!(
        heap.get_tuple(rid).unwrap(),
        (TupleMeta { ts: 2, is_deleted: false }, {
            let mut t = bytes_tuple(2, 16);
            t.set_rid(rid);
            t
        }),
        "an in place update runs its check under the latch"
    );
    // a check that looks at the OLD tuple and meta and rid
    let only_ts_2 = |m: &TupleMeta, t: &Tuple, r: Rid| m.ts == 2 && t.data()[0] == 2 && r == rid;
    assert!(heap.update_tuple_in_place(&TupleMeta { ts: 3, is_deleted: false }, &bytes_tuple(3, 16), rid, Some(&only_ts_2)).unwrap(), "an in place update runs its check under the latch: expected `heap.update_tuple_in_place(&TupleMeta {{ ts: 3, is_deleted: false }}, &bytes_tuple(3, 16), rid, Some(&...`");
    assert!(
        !heap.update_tuple_in_place(&meta(true), &bytes_tuple(4, 16), rid, Some(&only_ts_2)).unwrap(),
        "the tuple is at ts 3 now: the check fails"
    );
}

#[test]
fn s3c_01_an_in_place_update_must_keep_the_length() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rid = heap.insert_tuple(&meta(false), &bytes_tuple(1, 16)).unwrap();
    assert!(heap.update_tuple_in_place(&meta(false), &bytes_tuple(2, 17), rid, None).is_err(), "an in place update must keep the length: expected `heap.update_tuple_in_place(&meta(false), &bytes_tuple(2, 17), rid, None).is_err()`");
    assert_eq!(heap.get_tuple(rid).unwrap().1.data(), vec![1u8; 16].as_slice(), "an in place update must keep the length");
}

#[test]
fn s3c_01_every_tuple_of_a_big_table_is_found_by_its_rid() {
    let bpm = bpm(20);
    let heap = TableHeap::new(&bpm);
    let schema = heap_test_schema();
    let rids: Vec<Rid> = (0..2000).map(|i| heap.insert_tuple(&meta(false), &construct_tuple(&schema, i)).unwrap()).collect();
    for (i, rid) in rids.iter().enumerate().rev() {
        let (_, t) = heap.get_tuple(*rid).unwrap();
        assert_eq!(t.get_value(&schema, 0), Value::varchar(&format!("row{i}")), "every tuple of a big table is found by its rid");
    }
}

// ---- 3c-02 · The iterator -------------------------------------------------------------------------------------------------------

#[test]
fn s3c_02_an_empty_table_has_nothing_to_iterate() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let mut it = heap.make_iterator();
    assert!(it.is_end(), "an empty table has nothing to iterate: expected `it.is_end()`");
    assert!(it.next().is_none(), "an empty table has nothing to iterate: expected `it.next().is_none()`");
    assert!(heap.make_eager_iterator().is_end(), "an empty table has nothing to iterate: expected `heap.make_eager_iterator().is_end()`");
}

#[test]
fn s3c_02_the_iterator_visits_every_tuple_in_insertion_order_across_pages() {
    let bpm = bpm(20);
    let heap = TableHeap::new(&bpm);
    let schema = heap_test_schema();
    let rids: Vec<Rid> = (0..3000).map(|i| heap.insert_tuple(&meta(false), &construct_tuple(&schema, i)).unwrap()).collect();
    let mut seen = 0;
    for (i, (m, t)) in heap.make_iterator().enumerate() {
        assert_eq!(t.get_rid(), rids[i], "tuple {i} is where insert said");
        assert_eq!(t.get_value(&schema, 2), Value::bigint(i as i64), "the iterator visits every tuple in insertion order across pages");
        assert!(!m.is_deleted, "the iterator visits every tuple in insertion order across pages: expected `!m.is_deleted`");
        seen += 1;
    }
    assert_eq!(seen, 3000, "the iterator visits every tuple in insertion order across pages");
}

#[test]
fn s3c_02_a_cursor_can_be_driven_by_hand() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rids: Vec<Rid> = (0..3).map(|i| heap.insert_tuple(&meta(false), &bytes_tuple(i, 10)).unwrap()).collect();
    let mut it = heap.make_iterator();
    for rid in &rids {
        assert!(!it.is_end(), "a cursor can be driven by hand: expected `!it.is_end()`");
        assert_eq!(it.get_rid(), *rid, "a cursor can be driven by hand");
        assert_eq!(it.get_tuple().unwrap().1.get_rid(), *rid, "a cursor can be driven by hand");
        it.advance();
    }
    assert!(it.is_end(), "past the last tuple");
    assert!(it.get_tuple().is_err(), "a cursor can be driven by hand: expected `it.get_tuple().is_err()`");
}

#[test]
fn s3c_02_deleted_tuples_are_returned_with_their_meta() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    let rids: Vec<Rid> = (0..6).map(|i| heap.insert_tuple(&meta(false), &bytes_tuple(i, 10)).unwrap()).collect();
    for i in [1, 4] {
        heap.update_tuple_meta(&meta(true), rids[i]).unwrap();
    }
    let flags: Vec<bool> = heap.make_iterator().map(|(m, _)| m.is_deleted).collect();
    assert_eq!(flags, vec![false, true, false, false, true, false], "skipping them is the scan executor's job");
}

#[test]
fn s3c_02_an_iterator_stops_where_the_table_ended_when_it_was_made() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    for i in 0..5 {
        heap.insert_tuple(&meta(false), &bytes_tuple(i, 10)).unwrap();
    }
    let mut seen = 0;
    for (_, t) in heap.make_iterator() {
        seen += 1;
        // the Halloween problem: a statement that inserts into the table it scans must not see what it inserts
        heap.insert_tuple(&meta(false), &bytes_tuple(100, 10)).unwrap();
        assert!(t.data()[0] < 100, "an iterator stops where the table ended when it was made: expected `t.data()[0] < 100`");
    }
    assert_eq!(seen, 5, "an iterator stops where the table ended when it was made");
    assert_eq!(heap.make_iterator().count(), 10, "an iterator stops where the table ended when it was made");
}

#[test]
fn s3c_02_the_stopping_point_can_be_a_page_boundary_and_the_eager_iterator_has_none() {
    let bpm = bpm(10);
    let heap = TableHeap::new(&bpm);
    for i in 0..66 {
        heap.insert_tuple(&meta(false), &bytes_tuple(i, 100)).unwrap(); // the first page is exactly full
    }
    let snapshot = heap.make_iterator();
    heap.insert_tuple(&meta(false), &bytes_tuple(200, 100)).unwrap(); // starts the second page
    assert_eq!(snapshot.count(), 66, "the new page's tuple is not part of the snapshot");
    let mut eager = heap.make_eager_iterator();
    let mut n = 0;
    while !eager.is_end() {
        n += 1;
        if n == 10 {
            heap.insert_tuple(&meta(false), &bytes_tuple(201, 100)).unwrap();
        }
        eager.advance();
    }
    assert_eq!(n, 68, "the eager iterator also sees a tuple inserted while it runs");
}

// ---- 3c-03 · Indexes --------------------------------------------------------------------------------------------------------------

fn three_ints() -> Schema {
    Schema::new(vec![Column::new("a", Integer), Column::new("b", Integer), Column::new("c", Integer)])
}

fn key(schema: &Schema, values: &[i32]) -> Tuple {
    Tuple::new(&values.iter().map(|&v| Value::integer(v)).collect::<Vec<_>>(), schema)
}

#[test]
fn s3c_03_index_metadata_knows_its_key_schema() {
    let table = three_ints();
    let m = IndexMetadata::new("idx", "t", &table, vec![2, 0], false);
    assert_eq!(
        (m.get_name(), m.get_table_name(), m.get_key_attrs(), m.get_index_column_count(), m.is_primary_key()),
        ("idx", "t", &[2u32, 0][..], 2, false),
        "index metadata knows its key schema"
    );
    let ks = m.get_key_schema();
    assert_eq!(ks.columns().iter().map(|c| c.name()).collect::<Vec<_>>(), vec!["c", "a"], "index metadata knows its key schema");
    assert_eq!(ks.columns().iter().map(|c| c.offset()).collect::<Vec<_>>(), vec![0, 4], "offsets of the key schema, not the table's");
}

#[test]
fn s3c_03_a_key_tuple_becomes_the_bytes_of_a_fixed_size_key() {
    let ks = Schema::new(vec![Column::new("a", Integer)]);
    let k4 = generic_key_from_tuple::<4>(&key(&ks, &[0x0102_0304]));
    assert_eq!(k4.data, [4, 3, 2, 1], "a key tuple becomes the bytes of a fixed size key");
    let k16 = generic_key_from_tuple::<16>(&key(&ks, &[7]));
    assert_eq!(k16.data, [7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], "padded with zeros");
}

#[test]
#[should_panic]
fn s3c_03_a_key_bigger_than_the_index_key_is_a_bug() {
    let ks = Schema::new(vec![Column::new("a", Integer), Column::new("b", Integer)]);
    generic_key_from_tuple::<4>(&key(&ks, &[1, 2]));
}

#[test]
fn s3c_03_keys_compare_column_by_column_as_numbers() {
    let ks = Arc::new(Schema::new(vec![Column::new("a", Integer), Column::new("b", Integer)]));
    let cmp = SchemaComparator::<8>::new(ks.clone());
    let k = |a, b| generic_key_from_tuple::<8>(&key(&ks, &[a, b]));
    use std::cmp::Ordering::*;
    assert_eq!(cmp.compare(&k(1, 5), &k(1, 6)), Less, "keys compare column by column as numbers");
    assert_eq!(cmp.compare(&k(1, 6), &k(2, 0)), Less, "the first column decides");
    assert_eq!(cmp.compare(&k(2, 0), &k(1, 100)), Greater, "keys compare column by column as numbers");
    assert_eq!(cmp.compare(&k(3, 3), &k(3, 3)), Equal, "keys compare column by column as numbers");
    assert_eq!(cmp.compare(&k(-3, 0), &k(2, 0)), Less, "numbers, not bytes: -3 is stored as 0xFFFFFFFD");
    assert_eq!(cmp.compare(&k(0, -1), &k(0, 1)), Less, "keys compare column by column as numbers");
}

fn slots(rids: &[Rid]) -> Vec<u32> {
    rids.iter().map(|r| r.slot_num()).collect()
}

fn entry_slots(entries: &[(Tuple, Rid)]) -> Vec<u32> {
    entries.iter().map(|(_, r)| r.slot_num()).collect()
}

#[test]
fn s3c_03_an_index_inserts_scans_and_deletes() {
    let bpm = bpm(50);
    let table = three_ints();
    let index = BPlusTreeIndex::<16>::new(IndexMetadata::new("i", "t", &table, vec![0], false), &bpm);
    let ks = index.metadata().get_key_schema().clone();
    for i in 0..500 {
        assert!(index.insert_entry(&key(&ks, &[i * 3]), Rid::new(PageId(1), i as u32)), "a new entry (key {}, slot {i}) is added", i * 3);
    }
    assert!(!index.insert_entry(&key(&ks, &[30]), Rid::new(PageId(1), 10)), "an entry is in the index once");
    assert_eq!(index.scan_key(&key(&ks, &[30])), vec![Rid::new(PageId(1), 10)], "a key finds the rid of its entry");
    assert!(index.scan_key(&key(&ks, &[31])).is_empty(), "a key that is not there finds nothing");
    index.delete_entry(&key(&ks, &[30]), Rid::new(PageId(1), 10));
    assert!(index.scan_key(&key(&ks, &[30])).is_empty(), "a deleted entry is gone");
    index.delete_entry(&key(&ks, &[30]), Rid::new(PageId(1), 10)); // deleting a missing entry is not an error
    index.delete_entry(&key(&ks, &[33]), Rid::new(PageId(1), 99)); // nor is deleting a key with a rid it does not have
    assert_eq!(index.scan_key(&key(&ks, &[33])), vec![Rid::new(PageId(1), 11)], "an entry is removed only with its own rid");
    assert_eq!(index.scan_all().len(), 499, "every other entry is still there");
}

#[test]
fn s3c_03_a_key_may_have_many_rows_found_in_rid_order() {
    let bpm = bpm(50);
    let table = three_ints();
    let index = BPlusTreeIndex::<16>::new(IndexMetadata::new("i", "t", &table, vec![0], false), &bpm);
    let ks = index.metadata().get_key_schema().clone();
    // 300 rows over the keys 0, 1, 2, inserted in an order that is neither by key nor by rid
    for n in 0..300u32 {
        let slot = (n * 7) % 300;
        assert!(index.insert_entry(&key(&ks, &[(slot % 3) as i32]), Rid::new(PageId(slot as i32 / 100), slot)), "rows may share a key");
    }
    let ones = index.scan_key(&key(&ks, &[1]));
    assert_eq!(ones.len(), 100, "the key 1 finds every one of its 100 rows, across leaves");
    assert!(ones.windows(2).all(|w| w[0] < w[1]), "the rids of one key come in rid order");
    assert!(ones.iter().all(|r| r.slot_num() % 3 == 1), "a lookup finds only the rows of its key");
    index.delete_entry(&key(&ks, &[1]), Rid::new(PageId(0), 1));
    assert_eq!(index.scan_key(&key(&ks, &[1])).len(), 99, "deleting one row's entry leaves the key's other rows");
    let all = index.scan_all();
    let order: Vec<(i64, Rid)> = all.iter().map(|(k, r)| (k.get_value(&ks, 0).as_i64().unwrap(), *r)).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "a scan gives the entries by key, then by rid");
    assert_eq!(index.scan_from(&key(&ks, &[2])).len(), 100, "from the key 2 on: the key's 100 rows and nothing before them");
}

#[test]
fn s3c_03_a_key_with_a_null_is_indexed_first_but_a_lookup_with_a_null_finds_nothing() {
    let bpm = bpm(20);
    let table = three_ints();
    let index = BPlusTreeIndex::<16>::new(IndexMetadata::new("i", "t", &table, vec![0, 1], false), &bpm);
    let ks = index.metadata().get_key_schema().clone();
    let null_key = |a: Option<i32>, b: Option<i32>| {
        Tuple::new(&[a.map_or(Value::null(Integer), Value::integer), b.map_or(Value::null(Integer), Value::integer)], &ks)
    };
    for (slot, (a, b)) in
        [(Some(1), Some(1)), (None, Some(1)), (Some(1), None), (None, None), (None, None), (Some(-5), Some(0))].iter().enumerate()
    {
        assert!(index.insert_entry(&null_key(*a, *b), Rid::new(PageId(2), slot as u32)), "row {slot} is indexed, NULL or not");
    }
    assert_eq!(
        entry_slots(&index.scan_all()),
        vec![3, 4, 1, 5, 2, 0],
        "NULL orders before every value: (N,N) (N,N) (N,1) (-5,0) (1,N) (1,1)"
    );
    assert!(index.scan_key(&null_key(None, Some(1))).is_empty(), "NULL = 1 is never true: a lookup with a NULL finds nothing");
    assert!(index.scan_key(&null_key(None, None)).is_empty(), "NULL = NULL is never true either");
    let (k, _) = &index.scan_all()[0];
    assert!(k.get_value(&ks, 0).is_null() && k.get_value(&ks, 1).is_null(), "a scan gives each entry's key tuple, NULLs included");
    index.delete_entry(&null_key(None, None), Rid::new(PageId(2), 3));
    assert_eq!(entry_slots(&index.scan_all()), vec![4, 1, 5, 2, 0], "an entry whose key has a NULL is deleted like any other");
}

#[test]
fn s3c_03_a_primary_key_index_holds_each_key_once_and_no_null() {
    let bpm = bpm(20);
    let table = three_ints();
    let index = BPlusTreeIndex::<16>::new(IndexMetadata::new("pk", "t", &table, vec![0], true), &bpm);
    let ks = index.metadata().get_key_schema().clone();
    let rid = |slot| Rid::new(PageId(3), slot);
    assert!(index.insert_entry(&key(&ks, &[7]), rid(0)), "a new key is added");
    assert!(!index.insert_entry(&key(&ks, &[7]), rid(1)), "a key that is there is refused, whatever the rid");
    assert!(!index.insert_entry(&Tuple::new(&[Value::null(Integer)], &ks), rid(2)), "a key with a NULL is refused");
    assert_eq!(index.scan_key(&key(&ks, &[7])), vec![rid(0)], "the key keeps its first rid");
    index.delete_entry(&key(&ks, &[7]), rid(1));
    assert_eq!(index.scan_key(&key(&ks, &[7])), vec![rid(0)], "deleting the key with another rid removes nothing");
    index.delete_entry(&key(&ks, &[7]), rid(0));
    assert!(index.scan_key(&key(&ks, &[7])).is_empty(), "deleting the key with its own rid removes it");
    assert!(index.insert_entry(&key(&ks, &[7]), rid(1)), "and then the key is free for another row");
}

#[test]
fn s3c_03_a_scan_returns_entries_in_key_order_even_for_negative_and_composite_keys() {
    let bpm = bpm(50);
    let table = three_ints();
    let index = BPlusTreeIndex::<16>::new(IndexMetadata::new("i", "t", &table, vec![0, 1], false), &bpm);
    let ks = index.metadata().get_key_schema().clone();
    let keys = [(2, 1), (-5, 9), (2, -1), (0, 0), (-5, -9), (7, 7)];
    for (n, (a, b)) in keys.iter().enumerate() {
        index.insert_entry(&key(&ks, &[*a, *b]), Rid::new(PageId(0), n as u32));
    }
    let all = index.scan_all();
    let mut expected: Vec<(usize, (i32, i32))> = keys.iter().copied().enumerate().collect();
    expected.sort_by_key(|(_, k)| *k);
    assert_eq!(
        entry_slots(&all),
        expected.iter().map(|(n, _)| *n as u32).collect::<Vec<_>>(),
        "a scan returns entries in key order even for negative and composite keys"
    );
    let scanned_keys: Vec<(i64, i64)> =
        all.iter().map(|(k, _)| (k.get_value(&ks, 0).as_i64().unwrap(), k.get_value(&ks, 1).as_i64().unwrap())).collect();
    assert_eq!(scanned_keys, expected.iter().map(|(_, (a, b))| (*a as i64, *b as i64)).collect::<Vec<_>>(), "each entry carries its key");
    assert_eq!(
        entry_slots(&index.scan_from(&key(&ks, &[0, 0]))),
        vec![3, 2, 0, 5],
        "keys from (0, 0) on: (0,0) (2,-1) (2,1) (7,7) are slots 3, 2, 0, 5"
    );
    assert_eq!(slots(&index.scan_key(&key(&ks, &[2, -1]))), vec![2], "a composite key is looked up whole");
}

// ---- 3c-04 · The catalog: tables --------------------------------------------------------------------------------------------------

#[test]
fn s3c_04_a_created_table_can_be_found_by_name_and_by_oid() {
    let bpm = bpm(20);
    let mut catalog = Catalog::new(&bpm);
    let schema = three_ints();
    let t = catalog.create_table("t1", &schema).unwrap();
    assert_eq!((t.name.as_str(), t.oid, &t.schema), ("t1", 0, &schema), "a created table can be found by name and by oid");
    assert!(
        Arc::ptr_eq(&catalog.get_table("t1").unwrap(), &t),
        "a created table can be found by name and by oid: expected `Arc::ptr_eq(&catalog.get_table(\"t1\").unwrap(), &t)`"
    );
    assert!(
        Arc::ptr_eq(&catalog.get_table_by_oid(0).unwrap(), &t),
        "a created table can be found by name and by oid: expected `Arc::ptr_eq(&catalog.get_table_by_oid(0).unwrap(), &t)`"
    );
    assert!(catalog.get_table("nope").is_none() && catalog.get_table_by_oid(5).is_none(), "a created table can be found by name and by oid: expected `catalog.get_table(\"nope\").is_none() && catalog.get_table_by_oid(5).is_none()`");
}

#[test]
fn s3c_04_oids_are_handed_out_in_creation_order_and_names_are_unique() {
    let bpm = bpm(20);
    let mut catalog = Catalog::new(&bpm);
    let schema = three_ints();
    let oids: Vec<u32> = ["a", "b", "c"].iter().map(|n| catalog.create_table(n, &schema).unwrap().oid).collect();
    assert_eq!(oids, vec![0, 1, 2], "oids are handed out in creation order and names are unique");
    assert!(catalog.create_table("b", &schema).is_none(), "the name is taken");
    assert_eq!(catalog.create_table("d", &schema).unwrap().oid, 3, "a refused creation does not use an oid");
    let mut names = catalog.get_table_names();
    names.sort();
    assert_eq!(names, vec!["a", "b", "c", "d"], "oids are handed out in creation order and names are unique");
}

#[test]
fn s3c_04_every_table_has_its_own_heap() {
    let bpm = bpm(20);
    let mut catalog = Catalog::new(&bpm);
    let schema = three_ints();
    let (a, b) = (catalog.create_table("a", &schema).unwrap(), catalog.create_table("b", &schema).unwrap());
    assert_ne!(a.table.get_first_page_id(), b.table.get_first_page_id(), "every table has its own heap");
    let ra = a.table.insert_tuple(&meta(false), &key(&schema, &[1, 2, 3])).unwrap();
    assert_eq!(a.table.get_tuple(ra).unwrap().1.get_value(&schema, 1), Value::integer(2), "every table has its own heap");
    assert_eq!(b.table.make_iterator().count(), 0, "every table has its own heap");
    assert_eq!(a.table.make_iterator().count(), 1, "every table has its own heap");
}

#[test]
fn s3c_04_a_new_catalog_has_no_tables_and_names_are_case_sensitive() {
    let bpm = bpm(20);
    let mut catalog = Catalog::new(&bpm);
    assert!(
        catalog.get_table_names().is_empty(),
        "a new catalog has no tables and names are case sensitive: expected `catalog.get_table_names().is_empty()`"
    );
    assert!(catalog.get_table("t").is_none() && catalog.get_table_by_oid(0).is_none(), "a new catalog has no tables and names are case sensitive: expected `catalog.get_table(\"t\").is_none() && catalog.get_table_by_oid(0).is_none()`");
    let schema = three_ints();
    catalog.create_table("People", &schema).unwrap();
    assert!(catalog.get_table("people").is_none(), "names are compared exactly");
    assert!(catalog.create_table("people", &schema).is_some(), "so People and people are two tables");
    assert_eq!(catalog.get_table("people").unwrap().oid, 1, "a new catalog has no tables and names are case sensitive");
}

#[test]
fn s3c_04_many_tables_are_each_found_by_name_and_by_oid() {
    let bpm = bpm(20);
    let mut catalog = Catalog::new(&bpm);
    let schema = three_ints();
    for i in 0..100u32 {
        let t = catalog.create_table(&format!("t{i}"), &schema).unwrap();
        assert_eq!(t.oid, i, "many tables are each found by name and by oid");
    }
    for i in (0..100u32).rev() {
        let by_name = catalog.get_table(&format!("t{i}")).unwrap();
        let by_oid = catalog.get_table_by_oid(i).unwrap();
        assert!(Arc::ptr_eq(&by_name, &by_oid) && by_oid.name == format!("t{i}"), "many tables are each found by name and by oid: expected `Arc::ptr_eq(&by_name, &by_oid) && by_oid.name == format!(\"t{{i}}\")`");
    }
    assert_eq!(catalog.get_table_names().len(), 100, "many tables are each found by name and by oid");
}

// ---- 3c-04 · The catalog: indexes -------------------------------------------------------------------------------------------------

fn table_with_rows<'a>(catalog: &mut Catalog<'a>, name: &str, n: i32) -> Schema {
    let schema = three_ints();
    let info = catalog.create_table(name, &schema).unwrap();
    for i in 0..n {
        info.table.insert_tuple(&meta(false), &key(&schema, &[i, i % 10, i * 2])).unwrap();
    }
    schema
}

#[test]
fn s3c_04_a_new_index_is_filled_with_the_rows_the_table_already_has() {
    let bpm = bpm(50);
    let mut catalog = Catalog::new(&bpm);
    let schema = table_with_rows(&mut catalog, "t", 300);
    let index = catalog.create_index("t_a", "t", vec![0], false).unwrap().unwrap();
    assert_eq!(
        (index.name.as_str(), index.table_name.as_str(), index.key_size, index.is_primary_key, index.index_oid),
        ("t_a", "t", 4, false, 0),
        "a new index is filled with the rows the table already has"
    );
    let table = catalog.get_table("t").unwrap();
    let ks = &index.key_schema;
    for i in [0, 17, 299] {
        let rids = index.index.scan_key(&key(ks, &[i]));
        assert_eq!(rids.len(), 1, "a new index is filled with the rows the table already has");
        assert_eq!(
            table.table.get_tuple(rids[0]).unwrap().1.get_value(&schema, 2),
            Value::integer(i * 2),
            "a new index is filled with the rows the table already has"
        );
    }
    assert_eq!(index.index.scan_all().len(), 300, "a new index is filled with the rows the table already has");
}

#[test]
fn s3c_04_deleted_rows_are_not_indexed_and_rows_that_share_a_key_each_have_an_entry() {
    let bpm = bpm(50);
    let mut catalog = Catalog::new(&bpm);
    let schema = three_ints();
    let table = catalog.create_table("t", &schema).unwrap();
    let rids: Vec<Rid> = [5, 6, 7, 6].iter().map(|&v| table.table.insert_tuple(&meta(false), &key(&schema, &[v, 0, 0])).unwrap()).collect();
    table.table.update_tuple_meta(&meta(true), rids[1]).unwrap();
    let index = catalog.create_index("i", "t", vec![0], false).unwrap().unwrap();
    assert_eq!(
        index.index.scan_all().iter().map(|(_, r)| *r).collect::<Vec<_>>(),
        vec![rids[0], rids[3], rids[2]],
        "every live row has one entry; the deleted row with key 6 has none"
    );
    let second_seven = table.table.insert_tuple(&meta(false), &key(&schema, &[7, 1, 1])).unwrap();
    let j = catalog.create_index("j", "t", vec![0], false).unwrap().expect("two live rows with the key 7 can share an index");
    assert_eq!(j.index.scan_key(&key(&j.key_schema, &[7])), vec![rids[2], second_seven], "the key 7 finds both of its rows");
    let err = catalog.create_index("pk", "t", vec![0], true).err().expect("a primary key over two rows with the key 7 fails");
    assert!(err.message.contains("duplicate key value violates unique constraint \"pk\""), "the error names the unique violation: {err}");
    assert!(catalog.get_index("pk", "t").is_none(), "an index that failed to fill is not in the catalog");
    let u = catalog.create_table("u", &schema).unwrap();
    for _ in 0..2 {
        let values = [Value::null(Integer), Value::integer(0), Value::integer(0)];
        u.table.insert_tuple(&meta(false), &Tuple::new(&values, &schema)).unwrap();
    }
    let index = catalog.create_index("i", "u", vec![0], false).unwrap().expect("rows whose key is NULL are indexed too");
    assert_eq!(index.index.scan_all().len(), 2, "each row whose key is NULL has its entry");
    let err = catalog.create_index("pk", "u", vec![0], true).err().expect("a primary key over a NULL fails");
    assert!(err.message.contains("violates not-null constraint"), "the error names the not-null violation: {err}");
}

#[test]
fn s3c_04_creating_an_index_can_fail_without_an_error() {
    let bpm = bpm(50);
    let mut catalog = Catalog::new(&bpm);
    table_with_rows(&mut catalog, "t", 10);
    assert!(catalog.create_index("i", "nope", vec![0], false).unwrap().is_none(), "no such table");
    assert!(
        catalog.create_index("i", "t", vec![0], false).unwrap().is_some(),
        "creating an index can fail without an error: expected `catalog.create_index(\"i\", \"t\", vec![0], false).unwrap().is_some()`"
    );
    assert!(catalog.create_index("i", "t", vec![1], false).unwrap().is_none(), "the table already has an index called i");
    table_with_rows(&mut catalog, "u", 1);
    assert!(catalog.create_index("i", "u", vec![0], false).unwrap().is_some(), "index names are per table");
}

#[test]
fn s3c_04_only_integer_keys_of_at_most_64_bytes() {
    let bpm = bpm(50);
    let mut catalog = Catalog::new(&bpm);
    let schema = Schema::new(vec![Column::new("n", Integer), Column::new_varchar("s", 10), Column::new("d", Decimal)]);
    catalog.create_table("t", &schema).unwrap();
    assert!(catalog.create_index("i", "t", vec![1], false).is_err(), "a VARCHAR column");
    assert!(catalog.create_index("i", "t", vec![2], false).is_err(), "a DECIMAL column");
    assert!(catalog.create_index("i", "t", vec![], false).is_err(), "no columns");
    let wide = Schema::new((0..17).map(|i| Column::new(&format!("c{i}"), Integer)).collect());
    catalog.create_table("w", &wide).unwrap();
    assert!(catalog.create_index("i", "w", (0..17).collect(), false).is_err(), "17 integers are 68 bytes");
    assert_eq!(
        catalog.create_index("i", "w", (0..16).collect(), false).unwrap().unwrap().key_size,
        64,
        "only integer keys of at most 64 bytes"
    );
}

#[test]
fn s3c_04_indexes_are_found_by_name_by_oid_and_by_table() {
    let bpm = bpm(50);
    let mut catalog = Catalog::new(&bpm);
    table_with_rows(&mut catalog, "t", 5);
    table_with_rows(&mut catalog, "u", 5);
    let a = catalog.create_index("a", "t", vec![0], false).unwrap().unwrap();
    let b = catalog.create_index("b", "u", vec![0, 1], true).unwrap().unwrap();
    let c = catalog.create_index("c", "t", vec![1], false).unwrap().unwrap();
    assert_eq!((a.index_oid, b.index_oid, c.index_oid), (0, 1, 2), "indexes are found by name by oid and by table");
    assert!(
        Arc::ptr_eq(&catalog.get_index("a", "t").unwrap(), &a),
        "indexes are found by name by oid and by table: expected `Arc::ptr_eq(&catalog.get_index(\"a\", \"t\").unwrap(), &a)`"
    );
    assert!(
        Arc::ptr_eq(&catalog.get_index_by_oid(1).unwrap(), &b),
        "indexes are found by name by oid and by table: expected `Arc::ptr_eq(&catalog.get_index_by_oid(1).unwrap(), &b)`"
    );
    assert!(catalog.get_index("a", "u").is_none() && catalog.get_index("zzz", "t").is_none() && catalog.get_index("a", "nope").is_none(), "indexes are found by name by oid and by table: expected `catalog.get_index(\"a\", \"u\").is_none() && catalog.get_index(\"zzz\", \"t\").is_none() && catalog.get_inde...`");
    assert!(
        catalog.get_index_by_oid(9).is_none(),
        "indexes are found by name by oid and by table: expected `catalog.get_index_by_oid(9).is_none()`"
    );
    let names: Vec<String> = catalog.get_table_indexes("t").iter().map(|i| i.name.clone()).collect();
    assert_eq!(names, vec!["a", "c"], "in creation order");
    assert!(
        catalog.get_table_indexes("nope").is_empty(),
        "indexes are found by name by oid and by table: expected `catalog.get_table_indexes(\"nope\").is_empty()`"
    );
    assert_eq!(
        (b.key_size, b.is_primary_key, b.index.metadata().get_index_column_count()),
        (8, true, 2),
        "indexes are found by name by oid and by table"
    );
}

#[test]
fn s3c_04_a_composite_index_scans_in_key_order() {
    let bpm = bpm(50);
    let mut catalog = Catalog::new(&bpm);
    let schema = table_with_rows(&mut catalog, "t", 100);
    let index = catalog.create_index("ix", "t", vec![1, 0], false).unwrap().unwrap(); // (i % 10, i)
    let table = catalog.get_table("t").unwrap();
    let order: Vec<i64> =
        index.index.scan_all().iter().map(|(_, r)| table.table.get_tuple(*r).unwrap().1.get_value(&schema, 0).as_i64().unwrap()).collect();
    let mut expected: Vec<i64> = (0..100).collect();
    expected.sort_by_key(|i| (i % 10, *i));
    assert_eq!(order, expected, "a composite index scans in key order");
    let _ = IndexType::BPlusTreeIndex;
}

// ---- Properties against models ---------------------------------------------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 48, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

#[derive(Clone, Copy, Debug)]
enum HeapOp {
    Insert(u8, usize),
    Delete(usize),
    Get(usize),
    Overwrite(usize, u8),
}

fn heap_ops() -> impl Strategy<Value = Vec<HeapOp>> {
    prop::collection::vec(
        prop_oneof![
            5 => (any::<u8>(), 1usize..900).prop_map(|(b, l)| HeapOp::Insert(b, l)),
            2 => (0usize..400).prop_map(HeapOp::Delete),
            3 => (0usize..400).prop_map(HeapOp::Get),
            1 => (0usize..400, any::<u8>()).prop_map(|(i, b)| HeapOp::Overwrite(i, b)),
        ],
        1..150,
    )
}

proptest! {
    #![proptest_config(pconfig())]

    /// A heap is a growing list: every insert gets its own record id, every rid reads back what was put there (and its metadata), a
    /// deleted tuple is still readable, and an overwrite changes only that tuple. Tuples up to 900 bytes make pages fill and chain.
    #[test]
    fn s3c_01_a_heap_behaves_like_a_vec_of_tuples(ops in heap_ops()) {
        let bpm = bpm(20);
        let heap = TableHeap::new(&bpm);
        let mut model: Vec<(Rid, TupleMeta, Vec<u8>)> = Vec::new();
        for op in ops {
            match op {
                HeapOp::Insert(b, len) => {
                    let rid = heap.insert_tuple(&meta(false), &bytes_tuple(b, len)).unwrap();
                    prop_assert!(model.iter().all(|m| m.0 != rid), "record id {:?} handed out twice", rid);
                    model.push((rid, meta(false), vec![b; len]));
                }
                HeapOp::Delete(i) => { let n = model.len().max(1); if let Some(m) = model.get_mut(i % n) {
                    heap.update_tuple_meta(&meta(true), m.0).unwrap();
                    m.1 = meta(true);
                } },
                HeapOp::Get(i) => if let Some(m) = model.get(i % model.len().max(1)) {
                    let (got_meta, t) = heap.get_tuple(m.0).unwrap();
                    prop_assert_eq!((got_meta, t.data(), t.get_rid()), (m.1, &m.2[..], m.0));
                    prop_assert_eq!(heap.get_tuple_meta(m.0).unwrap(), m.1);
                },
                HeapOp::Overwrite(i, b) => { let n = model.len().max(1); if let Some(m) = model.get_mut(i % n) {
                    let new = bytes_tuple(b, m.2.len());
                    prop_assert!(heap.update_tuple_in_place(&meta(false), &new, m.0, None).unwrap());
                    m.1 = meta(false);
                    m.2 = vec![b; m.2.len()];
                } },
            }
        }
        for (rid, m, bytes) in &model {
            let (got_meta, t) = heap.get_tuple(*rid).unwrap();
            prop_assert_eq!((got_meta, t.data()), (*m, &bytes[..]));
        }
    }

    /// An iterator returns every tuple, deleted ones included with their metadata, in insertion order; one made before more inserts stops
    /// where the table ended and an eager one goes on to the end (the table has a tuple when the iterators are made: on an empty table both are at the end).
    #[test]
    fn s3c_02_an_iterator_visits_the_table_in_insertion_order(first in prop::collection::vec((any::<u8>(), 1usize..700, any::<bool>()), 1..80), more in prop::collection::vec((any::<u8>(), 1usize..700), 0..30)) {
        let bpm = bpm(20);
        let heap = TableHeap::new(&bpm);
        let mut model: Vec<(bool, Vec<u8>)> = vec![];
        for (b, len, deleted) in &first {
            let rid = heap.insert_tuple(&meta(false), &bytes_tuple(*b, *len)).unwrap();
            if *deleted { heap.update_tuple_meta(&meta(true), rid).unwrap(); }
            model.push((*deleted, vec![*b; *len]));
        }
        let snapshot = heap.make_iterator();
        let eager = heap.make_eager_iterator();
        for (b, len) in &more {
            heap.insert_tuple(&meta(false), &bytes_tuple(*b, *len)).unwrap();
        }
        let seen: Vec<(bool, Vec<u8>)> = snapshot.map(|(m, t)| (m.is_deleted, t.data().to_vec())).collect();
        prop_assert_eq!(&seen, &model, "an iterator made before the later inserts must not see them");
        let all: Vec<(bool, Vec<u8>)> = eager.map(|(m, t)| (m.is_deleted, t.data().to_vec())).collect();
        let mut expected = model.clone();
        expected.extend(more.iter().map(|(b, len)| (false, vec![*b; *len])));
        prop_assert_eq!(all, expected, "an eager iterator sees them");
        prop_assert_eq!(heap.make_iterator().count(), model.len() + more.len());
    }

    /// An index over two integer columns behaves like a sorted set of entries `(key, rid)`: keys repeat and may have a NULL in them
    /// (ordered first); an insert refuses only an entry that is there, a delete removes only its own entry, a lookup finds the rids of
    /// its key in rid order (none for a key with a NULL), and every scan is in order. A primary-key index behaves like a `BTreeMap`
    /// from the key to one rid instead, refusing a key it holds and a key with a NULL.
    #[test]
    fn s3c_03_an_index_behaves_like_a_sorted_set_of_entries(ops in prop::collection::vec((prop::option::weighted(0.85, -4i32..4), prop::option::weighted(0.85, -4i32..4), 0u8..3, 0u32..6), 1..150), probe in (prop::option::weighted(0.9, -5i32..5), -5i32..5), unique in any::<bool>()) {
        let bpm = bpm(50);
        let table = three_ints();
        let index = BPlusTreeIndex::<16>::new(IndexMetadata::new("i", "t", &table, vec![0, 1], unique), &bpm);
        let ks = index.metadata().get_key_schema().clone();
        let tuple = |a: Option<i32>, b: Option<i32>| Tuple::new(&[a.map_or(Value::null(Integer), Value::integer), b.map_or(Value::null(Integer), Value::integer)], &ks);
        type Key = (Option<i32>, Option<i32>);
        let mut model: BTreeSet<(Key, Rid)> = BTreeSet::new();
        for (a, b, op, slot) in ops {
            let (k, rid) = ((a, b), Rid::new(PageId(slot as i32 % 3), slot));
            let has_null = a.is_none() || b.is_none();
            match op {
                0 | 1 => {
                    let fresh = if unique { !has_null && !model.iter().any(|(m, _)| *m == k) } else { !model.contains(&(k, rid)) };
                    prop_assert_eq!(index.insert_entry(&tuple(a, b), rid), fresh, "insert ({:?}, {:?})", k, rid);
                    if fresh { model.insert((k, rid)); }
                }
                _ => { index.delete_entry(&tuple(a, b), rid); model.remove(&(k, rid)); }
            }
            let want: Vec<Rid> = if has_null { vec![] } else { model.iter().filter(|(m, _)| *m == k).map(|(_, r)| *r).collect() };
            prop_assert_eq!(index.scan_key(&tuple(a, b)), want, "lookup of {:?}", k);
        }
        let as_model = |entries: Vec<(Tuple, Rid)>| -> Vec<(Key, Rid)> {
            entries.iter().map(|(t, r)| ((t.get_value(&ks, 0).as_i64().map(|v| v as i32), t.get_value(&ks, 1).as_i64().map(|v| v as i32)), *r)).collect()
        };
        prop_assert_eq!(as_model(index.scan_all()), model.iter().copied().collect::<Vec<_>>(), "scan_all");
        let from: Key = (probe.0, Some(probe.1));
        prop_assert_eq!(as_model(index.scan_from(&tuple(from.0, from.1))), model.iter().filter(|(k, _)| *k >= from).copied().collect::<Vec<_>>(), "scan_from {:?}", from);
    }

    /// The catalog hands out oids in creation order, refuses a name that is in use (names are case sensitive), and finds every table by name
    /// and by oid; an index created on a table with rows holds one entry per live row, and a primary key fails if two live rows share
    /// a key.
    #[test]
    fn s3c_04_a_catalog_behaves_like_maps(names in prop::collection::vec("[a-cA-C]{1,2}", 1..25), rows in prop::collection::vec((-30i32..30, any::<bool>()), 0..50)) {
        let bpm = bpm(40);
        let mut catalog = Catalog::new(&bpm);
        let schema = three_ints();
        let mut model: HashMap<String, u32> = HashMap::new();
        for name in &names {
            let created = catalog.create_table(name, &schema);
            if model.contains_key(name) {
                prop_assert!(created.is_none(), "{:?} is already a table", name);
            } else {
                let info = created.unwrap();
                prop_assert_eq!(info.oid as usize, model.len(), "oids count up from zero in creation order");
                model.insert(name.clone(), info.oid);
            }
        }
        for (name, oid) in &model {
            prop_assert_eq!(catalog.get_table(name).unwrap().oid, *oid);
            prop_assert_eq!(&catalog.get_table_by_oid(*oid).unwrap().name, name);
        }
        let mut listed = catalog.get_table_names();
        listed.sort();
        let mut expected: Vec<String> = model.keys().cloned().collect();
        expected.sort();
        prop_assert_eq!(listed, expected);
        // an index over a table with rows
        let t = catalog.get_table(&names[0]).unwrap();
        let mut live: Vec<(i32, Rid)> = vec![];
        for (v, deleted) in &rows {
            let rid = t.table.insert_tuple(&meta(false), &key(&schema, &[*v, 0, 0])).unwrap();
            if *deleted { t.table.update_tuple_meta(&meta(true), rid).unwrap(); } else { live.push((*v, rid)); }
        }
        live.sort();
        // a deleted row is not indexed; every live row is, keys shared or not
        let created = catalog.create_index("idx", &names[0], vec![0], false).unwrap().unwrap();
        prop_assert_eq!(created.index.scan_all().iter().map(|(_, r)| *r).collect::<Vec<_>>(), live.iter().map(|(_, r)| *r).collect::<Vec<_>>(), "every live row has its entry, by key then rid");
        // a primary key fails when two live rows share a key
        let shared = live.windows(2).any(|w| w[0].0 == w[1].0);
        let pk = catalog.create_index("pk", &names[0], vec![0], true);
        prop_assert_eq!(pk.is_err(), shared, "a primary key over live rows fails exactly when two share a key: {:?}", pk.as_ref().err());
        prop_assert_eq!(catalog.get_index("pk", &names[0]).is_some(), !shared, "a failed index is not in the catalog");
    }
}

// ---- 3c-05 · Boss: a table, its index and the catalog together ----------------------------------------------------------------

#[test]
fn s3c_05_a_table_and_its_index_agree_after_inserts_and_deletes_through_the_catalog() {
    let bpm = bpm(60);
    let mut catalog = Catalog::new(&bpm);
    let schema = three_ints();
    let table = catalog.create_table("orders", &schema).unwrap();
    let index = catalog.create_index("orders_pk", "orders", vec![0], true).unwrap().unwrap();
    let mut live: BTreeMap<i32, Rid> = BTreeMap::new();
    for i in 0..400 {
        let k = (i * 37) % 401;
        let tuple = key(&schema, &[k, i, 0]);
        let rid = table.table.insert_tuple(&meta(false), &tuple).unwrap();
        assert!(index.index.insert_entry(&key(&index.key_schema, &[k]), rid), "a table and its index agree after inserts and deletes through the catalog: expected `index.index.insert_entry(&key(&index.key_schema, &[k]), rid)`");
        live.insert(k, rid);
    }
    for k in (0..401).step_by(3) {
        if let Some(rid) = live.remove(&k) {
            table.table.update_tuple_meta(&meta(true), rid).unwrap();
            index.index.delete_entry(&key(&index.key_schema, &[k]), rid);
        }
    }
    assert_eq!(
        index.index.scan_all().iter().map(|(_, r)| *r).collect::<Vec<_>>(),
        live.values().copied().collect::<Vec<_>>(),
        "the index lists the live rows in key order"
    );
    let scanned: Vec<i32> =
        table.table.make_iterator().filter(|(m, _)| !m.is_deleted).map(|(_, t)| t.get_value(&schema, 0).as_i64().unwrap() as i32).collect();
    let mut sorted = scanned.clone();
    sorted.sort();
    assert_eq!(sorted, live.keys().copied().collect::<Vec<_>>(), "a scan of the table finds the same rows");
    for (k, rid) in &live {
        assert_eq!(
            table.table.get_tuple(*rid).unwrap().1.get_value(&schema, 0),
            Value::integer(*k),
            "a table and its index agree after inserts and deletes through the catalog"
        );
    }
}

// @@ challenge 3c-c1 begin
mod ch_3c_c1 {
    use proptest::prelude::*;

    use bustub::storage::table::free_space_map::FreeSpaceMap;

    #[test]
    fn s3c_c1_the_lowest_page_with_enough_room() {
        let mut m = FreeSpaceMap::new(4);
        for (p, f) in [(1, 50), (2, 20), (3, 80)] {
            m.set(p, f);
        }
        assert_eq!((m.find(10), m.find(60), m.find(100)), (Some(1), Some(3), None), "this check");
        m.set(0, 70);
        assert_eq!(m.find(60), Some(0), "this check");
        assert_eq!(m.get(0), 70, "this check");
    }

    #[test]
    fn s3c_c1_nothing_has_room_in_a_new_map() {
        let m = FreeSpaceMap::new(5);
        assert_eq!(m.find(1), None, "this check");
        assert_eq!(m.find(0), Some(0), "every page has at least 0 bytes");
    }

    #[test]
    fn s3c_c1_no_pages_and_one_page() {
        let m = FreeSpaceMap::new(0);
        assert_eq!((m.find(0), m.pages()), (None, 0), "this check");
        let mut one = FreeSpaceMap::new(1);
        one.set(0, 5);
        assert_eq!((one.find(5), one.find(6)), (Some(0), None), "this check");
    }

    #[test]
    fn s3c_c1_pages_that_do_not_fill_the_tree_are_never_returned() {
        let mut m = FreeSpaceMap::new(5);
        m.set(4, 9);
        assert_eq!(m.find(9), Some(4), "this check");
        assert_eq!(m.find(10), None, "the padding leaves of the tree are not pages");
    }

    #[test]
    fn s3c_c1_a_large_map_answers_quickly() {
        let n = 200_000;
        let mut m = FreeSpaceMap::new(n);
        m.set(n - 1, 100);
        let t = std::time::Instant::now();
        for _ in 0..200_000 {
            assert_eq!(m.find(100), Some(n - 1), "this check");
        }
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "200 000 finds took {:?}: find must not scan", t.elapsed());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a linear scan, after any updates.
        #[test]
        fn s3c_c1_property_find_equals_a_scan(pages in 0usize..20, ops in proptest::collection::vec((0usize..20, 0u32..100), 0..60), probes in proptest::collection::vec(0u32..120, 1..10)) {
            let mut m = FreeSpaceMap::new(pages);
            let mut model = vec![0u32; pages];
            for (p, f) in ops {
                if pages == 0 { break; }
                let p = p % pages;
                m.set(p, f);
                model[p] = f;
                for &need in &probes {
                    prop_assert_eq!(m.find(need), model.iter().position(|&x| x >= need));
                }
                prop_assert_eq!(m.get(p), f);
            }
        }
    }
}
// @@ challenge 3c-c1 end

// @@ challenge 3c-c2 begin
mod ch_3c_c2 {
    use proptest::prelude::*;

    use bustub::catalog::name_catalog::{NameCatalog, NameError};
    use std::collections::BTreeMap;

    #[test]
    fn s3c_c2_names_are_case_insensitive_and_unique() {
        let mut c = NameCatalog::new();
        assert_eq!(c.create("Users"), Ok(1), "this check");
        assert_eq!(c.create("USERS"), Err(NameError::Exists), "this check");
        assert_eq!(c.lookup("users"), Some(1), "this check");
        assert_eq!(c.create(""), Err(NameError::Invalid), "this check");
        assert_eq!(c.names(), vec!["users".to_string()], "this check");
    }

    #[test]
    fn s3c_c2_an_id_is_never_reused() {
        let mut c = NameCatalog::new();
        assert_eq!(c.create("a"), Ok(1), "this check");
        assert!(c.drop("A"), "this check: expected `c.drop(\"A\")`");
        assert!(!c.drop("a"), "this check: expected `!c.drop(\"a\")`");
        assert_eq!(c.create("a"), Ok(2), "the dropped table's id must not come back");
        assert_eq!(c.create("b"), Ok(3), "this check");
    }

    #[test]
    fn s3c_c2_rename_keeps_the_id() {
        let mut c = NameCatalog::new();
        c.create("old").unwrap();
        c.create("other").unwrap();
        assert_eq!(c.rename("OLD", "New"), Ok(()), "this check");
        assert_eq!((c.lookup("old"), c.lookup("new")), (None, Some(1)), "this check");
        assert_eq!(c.rename("missing", "x"), Err(NameError::Missing), "this check");
        assert_eq!(c.rename("new", "OTHER"), Err(NameError::Exists), "this check");
        assert_eq!(c.rename("new", "NEW"), Ok(()), "renaming to the same name in another case is fine");
    }

    #[test]
    fn s3c_c2_an_empty_catalog_has_no_names_and_dropping_a_missing_table_is_false() {
        let mut c = NameCatalog::new();
        assert!(c.names().is_empty(), "this check: expected `c.names().is_empty()`");
        assert_eq!(c.lookup("x"), None, "this check");
        assert!(!c.drop("x"), "this check: expected `!c.drop(\"x\")`");
        assert_eq!(c.create("x"), Ok(1), "a failed drop does not use up an id");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a map from lowercase name to id with a counter that only grows.
        #[test]
        fn s3c_c2_property_a_catalog_matches_a_model(ops in proptest::collection::vec((0u8..3, 0u8..4, any::<bool>()), 0..60)) {
            let mut c = NameCatalog::new();
            let mut m: BTreeMap<String, u32> = BTreeMap::new();
            let mut next = 1u32;
            for (op, n, upper) in ops {
                let name = if upper { format!("T{n}") } else { format!("t{n}") };
                let low = name.to_ascii_lowercase();
                match op {
                    0 => {
                        let want = if m.contains_key(&low) { Err(NameError::Exists) } else { m.insert(low, next); next += 1; Ok(next - 1) };
                        prop_assert_eq!(c.create(&name), want);
                    }
                    1 => prop_assert_eq!(c.drop(&name), m.remove(&low).is_some()),
                    _ => {
                        let new = format!("t{}", (n + 1) % 4);
                        let want = if !m.contains_key(&low) { Err(NameError::Missing) } else if low != new && m.contains_key(&new) { Err(NameError::Exists) } else { let id = m.remove(&low).unwrap(); m.insert(new.clone(), id); Ok(()) };
                        prop_assert_eq!(c.rename(&name, &new), want);
                    }
                }
                prop_assert_eq!(c.names(), m.keys().cloned().collect::<Vec<_>>());
                for (k, &id) in &m { prop_assert_eq!(c.lookup(k), Some(id)); }
            }
        }
    }
}
// @@ challenge 3c-c2 end

// @@ challenge 3c-c3 begin
mod ch_3c_c3 {
    use proptest::prelude::*;

    use bustub::storage::table::raise::{give_raise, Slot};
    use std::collections::BTreeMap;

    fn live(rows: &[Slot]) -> BTreeMap<u32, i64> {
        let mut m = BTreeMap::new();
        for (id, s) in rows.iter().flatten() {
            assert!(m.insert(*id, *s).is_none(), "employee {id} is live twice");
        }
        m
    }

    #[test]
    fn s3c_c3_each_matching_row_is_raised_once() {
        let mut rows = vec![Some((1, 100)), Some((2, 5000))];
        assert_eq!(give_raise(&mut rows, 1000, 2), 1, "this check");
        assert_eq!(live(&rows), BTreeMap::from([(1, 200), (2, 5000)]), "this check");
    }

    #[test]
    fn s3c_c3_the_update_ends_even_when_the_new_salary_is_still_below_the_limit() {
        let mut rows = vec![Some((1, 1)), Some((2, 2))];
        assert_eq!(give_raise(&mut rows, 1_000_000, 3), 2, "each row once, not until it passes the limit");
        assert_eq!(live(&rows), BTreeMap::from([(1, 3), (2, 6)]), "this check");
    }

    #[test]
    fn s3c_c3_nothing_matches_and_dead_slots_are_skipped() {
        let mut rows = vec![None, Some((1, 5000)), None];
        assert_eq!(give_raise(&mut rows, 1000, 2), 0, "this check");
        assert_eq!(rows.len(), 3, "nothing was appended");
    }

    #[test]
    fn s3c_c3_a_salary_at_the_limit_is_not_below_it() {
        let mut rows = vec![Some((1, 1000)), Some((2, 999))];
        assert_eq!(give_raise(&mut rows, 1000, 2), 1, "this check");
        assert_eq!(live(&rows), BTreeMap::from([(1, 1000), (2, 1998)]), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the live salaries equal applying the raise once to the rows below the limit.
        #[test]
        fn s3c_c3_property_the_raise_is_applied_once(sal in proptest::collection::vec(proptest::option::of(1i64..2000), 0..12), limit in 1i64..2500, factor in 1i64..4) {
            let mut rows: Vec<Slot> = sal.iter().enumerate().map(|(i, s)| s.map(|x| (i as u32, x))).collect();
            let before = live(&rows);
            let n = give_raise(&mut rows, limit, factor);
            let want: BTreeMap<u32, i64> = before.iter().map(|(&id, &s)| (id, if s < limit { s * factor } else { s })).collect();
            prop_assert_eq!(live(&rows), want);
            prop_assert_eq!(n, before.values().filter(|&&s| s < limit).count());
        }
    }
}
// @@ challenge 3c-c3 end

// @@ challenge 3c-c4 begin
mod ch_3c_c4 {
    use proptest::prelude::*;

    use bustub::storage::table::moving_heap::MovingHeap;
    use std::collections::BTreeMap;

    #[test]
    fn s3c_c4_rows_keep_their_ids_and_land_on_pages_with_room() {
        let mut h = MovingHeap::new(10);
        let a = h.insert(&[1; 6]).unwrap();
        let b = h.insert(&[2; 6]).unwrap();
        assert_eq!((a, b), (0, 1), "this check");
        assert_ne!(h.page_of(a), h.page_of(b), "6 + 6 does not fit in a page of 10");
        assert_eq!((h.get(a), h.get(b)), (Some(&[1u8; 6][..]), Some(&[2u8; 6][..])), "this check");
    }

    #[test]
    fn s3c_c4_an_update_that_fits_stays_where_it_is() {
        let mut h = MovingHeap::new(10);
        let a = h.insert(&[1; 4]).unwrap();
        let page = h.page_of(a);
        assert!(h.update(a, &[9; 9]), "this check: expected `h.update(a, &[9; 9])`");
        assert_eq!(h.page_of(a), page, "this check");
        assert_eq!(h.get(a), Some(&[9u8; 9][..]), "this check");
    }

    #[test]
    fn s3c_c4_an_update_that_does_not_fit_moves_the_row_and_keeps_its_id() {
        let mut h = MovingHeap::new(10);
        let a = h.insert(&[1; 5]).unwrap();
        let b = h.insert(&[2; 5]).unwrap();
        assert_eq!(h.page_of(a), h.page_of(b), "this check");
        assert!(h.update(a, &[7; 8]), "8 does not fit beside b's 5, so a moves");
        assert_ne!(h.page_of(a), h.page_of(b), "this check");
        assert_eq!((h.get(a), h.get(b)), (Some(&[7u8; 8][..]), Some(&[2u8; 5][..])), "this check");
        for p in 0..h.pages() {
            assert!(h.used(p) <= 10, "this check: expected `h.used(p) <= 10`");
        }
    }

    #[test]
    fn s3c_c4_oversized_rows_and_unknown_ids_are_refused() {
        let mut h = MovingHeap::new(10);
        assert_eq!(h.insert(&[0; 11]), None, "this check");
        let a = h.insert(&[1; 3]).unwrap();
        assert!(!h.update(a, &[0; 11]), "this check: expected `!h.update(a, &[0; 11])`");
        assert!(!h.update(99, &[1]), "this check: expected `!h.update(99, &[1])`");
        assert!(h.delete(a), "this check: expected `h.delete(a)`");
        assert!(!h.delete(a), "this check: expected `!h.delete(a)`");
        assert_eq!(h.get(a), None, "this check");
        assert_eq!(h.insert(&[1]).unwrap(), 1, "ids are not reused");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a map from rid to row, with every page within its size and the used bytes adding up.
        #[test]
        fn s3c_c4_property_a_moving_heap_is_a_map_within_its_pages(ops in proptest::collection::vec((0u8..3, 0u32..8, 0usize..14, any::<u8>()), 0..80)) {
            let mut h = MovingHeap::new(12);
            let mut m: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
            let mut next = 0u32;
            for (op, rid, len, byte) in ops {
                let row = vec![byte; len];
                match op {
                    0 => {
                        let got = h.insert(&row);
                        if len > 12 { prop_assert_eq!(got, None); } else { prop_assert_eq!(got, Some(next)); m.insert(next, row); next += 1; }
                    }
                    1 => {
                        let want = m.contains_key(&rid) && len <= 12;
                        prop_assert_eq!(h.update(rid, &row), want);
                        if want { m.insert(rid, row); }
                    }
                    _ => prop_assert_eq!(h.delete(rid), m.remove(&rid).is_some()),
                }
                for (k, v) in &m { prop_assert_eq!(h.get(*k), Some(v.as_slice())); }
                let mut total = 0;
                for p in 0..h.pages() { prop_assert!(h.used(p) <= 12); total += h.used(p); }
                prop_assert_eq!(total, m.values().map(|v| v.len()).sum::<usize>());
            }
        }
    }
}
// @@ challenge 3c-c4 end

// @@ challenge 3c-c5 begin
mod ch_3c_c5 {
    use proptest::prelude::*;

    use bustub::catalog::evolving_table::EvolvingTable;

    #[test]
    fn s3c_c5_old_rows_read_the_default_of_a_new_column() {
        let mut t = EvolvingTable::new(2);
        let a = t.insert(&[1, 2]).unwrap();
        t.add_column(7);
        let b = t.insert(&[3, 4, 5]).unwrap();
        assert_eq!(t.get(a), Some(vec![1, 2, 7]), "this check");
        assert_eq!(t.get(b), Some(vec![3, 4, 5]), "this check");
        assert_eq!(t.columns(), 3, "this check");
    }

    #[test]
    fn s3c_c5_dropping_a_column_removes_it_from_every_row() {
        let mut t = EvolvingTable::new(2);
        let a = t.insert(&[1, 2]).unwrap();
        t.add_column(7);
        assert!(t.drop_column(0), "this check: expected `t.drop_column(0)`");
        assert_eq!(t.get(a), Some(vec![2, 7]), "this check");
        assert!(!t.drop_column(5), "this check: expected `!t.drop_column(5)`");
    }

    #[test]
    fn s3c_c5_a_row_of_the_wrong_width_is_refused() {
        let mut t = EvolvingTable::new(2);
        assert_eq!(t.insert(&[1]), None, "this check");
        assert_eq!(t.insert(&[1, 2, 3]), None, "this check");
        t.add_column(0);
        assert!(t.insert(&[1, 2, 3]).is_some(), "this check: expected `t.insert(&[1, 2, 3]).is_some()`");
    }

    #[test]
    fn s3c_c5_columns_added_later_have_their_own_defaults() {
        let mut t = EvolvingTable::new(1);
        let a = t.insert(&[1]).unwrap();
        t.add_column(10);
        let b = t.insert(&[2, 20]).unwrap();
        t.add_column(30);
        assert_eq!((t.get(a), t.get(b)), (Some(vec![1, 10, 30]), Some(vec![2, 20, 30])), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equal to a table that rewrites every row eagerly at each schema change.
        #[test]
        fn s3c_c5_property_lazy_defaults_equal_eager_rewriting(ops in proptest::collection::vec((0u8..3, any::<i64>(), 0usize..6), 0..40)) {
            let mut t = EvolvingTable::new(1);
            let mut rows: Vec<Vec<i64>> = Vec::new();
            let mut width = 1usize;
            for (op, v, i) in ops {
                match op {
                    0 => { let row: Vec<i64> = (0..width as i64).map(|k| v.wrapping_add(k)).collect(); prop_assert_eq!(t.insert(&row), Some(rows.len())); rows.push(row); }
                    1 => { t.add_column(v); for r in &mut rows { r.push(v); } width += 1; }
                    _ => { let ok = i < width; prop_assert_eq!(t.drop_column(i), ok); if ok { for r in &mut rows { r.remove(i); } width -= 1; } }
                }
                prop_assert_eq!(t.columns(), width);
                for (rid, r) in rows.iter().enumerate() { prop_assert_eq!(t.get(rid), Some(r.clone())); }
            }
        }
    }
}
// @@ challenge 3c-c5 end
