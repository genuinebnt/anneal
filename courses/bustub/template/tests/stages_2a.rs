//! Tests for module 2a, typed pages. A test name starts with its stage: `s2a_03_…` belongs to stage 2a-03, and
//! `anneal course test` runs just those.
//!
//! The tests use only the public items: the `FixedSize` encodings, `Rid`, `GenericKey` and its comparators, `array_size` and
//! `PageArray`. Values are encoded into buffers at random offsets and must decode to themselves without touching their neighbours;
//! `PageArray` runs random operations against a `Vec`; `lower_bound` is compared with `partition_point`. The byte order you choose
//! for an encoding is yours: only the round trip is tested.

use std::cmp::Ordering;
use std::fmt::Debug;

use bustub::common::config::{PageId, BUSTUB_PAGE_SIZE};
use bustub::common::rid::Rid;
use bustub::storage::index::fixed_size::{array_size, FixedSize};
use bustub::storage::index::generic_key::{GenericComparator, GenericKey, KeyComparator};
use bustub::storage::index::int_comparator::IntComparator;
use bustub::storage::page::layout::HTABLE_BUCKET_PAGE_METADATA_SIZE;
use bustub::storage::page::page_array::PageArray;
#[path = "common/pool.rs"]
mod pool;
use pool::{pool_with, Policy};
use proptest::prelude::*;

mod common;

fn config() -> ProptestConfig {
    ProptestConfig { cases: 128, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

// ---- 2a-01 · Ids that survive bytes -------------------------------------------------------------------------------------

/// Encodes `value` at `offset` of a buffer full of 0xAA, checks that exactly `T::SIZE` bytes were written there, and decodes it back.
fn survives<T: FixedSize + PartialEq + Debug>(value: &T, offset: usize) -> Result<(), TestCaseError> {
    let mut buf = vec![0xAAu8; 64];
    value.encode(&mut buf[offset..offset + T::SIZE]);
    prop_assert!(buf[..offset].iter().all(|&b| b == 0xAA), "encoding {:?} wrote before its slot", value);
    prop_assert!(buf[offset + T::SIZE..].iter().all(|&b| b == 0xAA), "encoding {:?} wrote after its slot", value);
    let back = T::decode(&buf[offset..offset + T::SIZE]);
    prop_assert_eq!(&back, value, "decode(encode(x)) must be x");
    Ok(())
}

fn rid_strategy() -> impl Strategy<Value = Rid> {
    (any::<i32>(), any::<u32>()).prop_map(|(p, s)| Rid::new(PageId(p), s))
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s2a_01_i32_survives_bytes_at_any_offset(x in any::<i32>(), offset in 0usize..56) {
        survives(&x, offset)?;
    }

    #[test]
    fn s2a_01_u32_and_i64_survive_bytes_at_any_offset(a in any::<u32>(), b in any::<i64>(), offset in 0usize..48) {
        survives(&a, offset)?;
        survives(&b, offset)?;
    }

    #[test]
    fn s2a_01_page_ids_survive_bytes_including_the_invalid_one(x in any::<i32>(), offset in 0usize..56) {
        survives(&PageId(x), offset)?;
        survives(&PageId::INVALID, offset)?;
    }

    #[test]
    fn s2a_01_rids_survive_bytes(rid in rid_strategy(), offset in 0usize..48) {
        survives(&rid, offset)?;
    }

    /// `get` packs the page id in the high 32 bits and the slot in the low 32 (BusTub's definition), `from_i64` undoes it.
    #[test]
    fn s2a_01_a_rid_packs_into_an_i64_and_back(rid in rid_strategy()) {
        let packed = rid.get();
        prop_assert_eq!(packed, ((rid.page_id().0 as i64) << 32) | rid.slot_num() as i64, "page id in the high bits, slot in the low bits");
        prop_assert_eq!(Rid::from_i64(packed), rid);
        prop_assert_eq!(Rid::from_i64(packed).page_id(), rid.page_id());
        prop_assert_eq!(Rid::from_i64(packed).slot_num(), rid.slot_num());
    }
}

#[test]
fn s2a_01_sizes_are_what_a_page_layout_can_count_on() {
    assert_eq!(
        (<i32 as FixedSize>::SIZE, <u32 as FixedSize>::SIZE, <i64 as FixedSize>::SIZE),
        (4, 4, 8),
        "sizes are what a page layout can count on"
    );
    assert_eq!((<PageId as FixedSize>::SIZE, <Rid as FixedSize>::SIZE), (4, 8), "sizes are what a page layout can count on");
}

#[test]
fn s2a_01_the_default_rid_names_no_page_and_page_zero_is_a_real_page() {
    assert!(!Rid::default().page_id().is_valid(), "the default rid is invalid");
    let zero = Rid::new(PageId(0), 7);
    assert!(zero.page_id().is_valid(), "the default rid names no page and page zero is a real page: expected `zero.page_id().is_valid()`");
    assert_eq!(Rid::from_i64(zero.get()), zero, "page 0, slot 7 is not confused with an invalid rid");
}

// ---- 2a-02 · Keys and how to order them ------------------------------------------------------------------------------------

fn key<const N: usize>(n: i64) -> GenericKey<N> {
    let mut k = GenericKey::<N>::default();
    k.set_from_integer(n);
    k
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn s2a_02_a_generic_key_holds_any_integer_and_the_rest_of_it_is_zero(n in any::<i64>(), junk in any::<u8>()) {
        let mut k = GenericKey::<16>::default();
        k.data = [junk; 16];
        k.set_from_integer(n);
        prop_assert_eq!(k.get_as_integer(), n);
        prop_assert!(k.data[8..].iter().all(|&b| b == 0), "set_from_integer clears the key before storing the integer");
    }

    #[test]
    fn s2a_02_generic_keys_survive_bytes(n in any::<i64>(), offset in 0usize..32) {
        let k = key::<8>(n);
        let mut buf = vec![0x55u8; 64];
        k.encode(&mut buf[offset..offset + <GenericKey<8> as FixedSize>::SIZE]);
        prop_assert_eq!(<GenericKey<8>>::decode(&buf[offset..offset + 8]), k);
        let big = key::<24>(n);
        big.encode(&mut buf[offset..offset + 24]);
        prop_assert_eq!(<GenericKey<24>>::decode(&buf[offset..offset + 24]).get_as_integer(), n);
    }

    /// The comparators order keys exactly as the integers inside them are ordered, so sorting by a comparator sorts the integers
    /// (negative numbers before positive ones, which their raw bytes would not).
    #[test]
    fn s2a_02_comparators_agree_with_integer_order(a in any::<i64>(), b in any::<i64>(), c in any::<i32>(), d in any::<i32>()) {
        let g = GenericComparator::<8>;
        prop_assert_eq!(g.compare(&key(a), &key(b)), a.cmp(&b));
        prop_assert_eq!(IntComparator.compare(&c, &d), c.cmp(&d));
    }

    #[test]
    fn s2a_02_a_comparator_can_drive_a_sort(mut xs in prop::collection::vec(any::<i64>(), 0..60)) {
        let g = GenericComparator::<8>;
        let mut keys: Vec<GenericKey<8>> = xs.iter().map(|&n| key(n)).collect();
        keys.sort_by(|a, b| g.compare(a, b));
        xs.sort();
        prop_assert_eq!(keys.iter().map(|k| k.get_as_integer()).collect::<Vec<_>>(), xs);
    }

    #[test]
    fn s2a_02_a_pair_is_its_two_halves_side_by_side(n in any::<i64>(), rid in rid_strategy(), offset in 0usize..32) {
        type Entry = (GenericKey<8>, Rid);
        prop_assert_eq!(Entry::SIZE, 16);
        survives::<Entry>(&(key(n), rid), offset)?;
        survives::<((i32, u32), (i64, PageId))>(&((n as i32, 7), (n, PageId(n as i32))), offset)?;
    }
}

#[test]
fn s2a_02_array_size_is_what_fits_after_the_metadata() {
    assert_eq!(array_size(8, 16), (BUSTUB_PAGE_SIZE - 8) / 16, "array size is what fits after the metadata");
    assert_eq!(array_size(0, 8192), 1, "array size is what fits after the metadata");
    assert_eq!(
        array_size(HTABLE_BUCKET_PAGE_METADATA_SIZE, <(GenericKey<8>, Rid) as FixedSize>::SIZE),
        511,
        "array size is what fits after the metadata"
    );
    const AT_COMPILE_TIME: usize = array_size(8, 12);
    assert_eq!(AT_COMPILE_TIME, 682, "array size is what fits after the metadata");
}

#[test]
fn s2a_02_keys_of_other_sizes_and_a_zero_key() {
    assert_eq!(GenericKey::<8>::default().get_as_integer(), 0, "keys of other sizes and a zero key");
    assert_eq!(<GenericKey<64> as FixedSize>::SIZE, 64, "keys of other sizes and a zero key");
    assert_eq!(key::<64>(-5).get_as_integer(), -5, "keys of other sizes and a zero key");
}

// ---- 2a-03 · A sorted array inside a page -----------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum ArrayOp {
    Insert(usize, i32),
    Remove(usize),
    Set(usize, i32),
}

fn array_ops() -> impl Strategy<Value = Vec<ArrayOp>> {
    prop::collection::vec(
        prop_oneof![
            4 => (0..40usize, any::<i32>()).prop_map(|(i, v)| ArrayOp::Insert(i, v)),
            3 => (0..40usize).prop_map(ArrayOp::Remove),
            2 => (0..40usize, any::<i32>()).prop_map(|(i, v)| ArrayOp::Set(i, v)),
        ],
        1..150,
    )
}

const CAP: usize = 16;

proptest! {
    #![proptest_config(config())]

    /// Insert, remove and set at arbitrary positions: the array always reads as the `Vec` model, whatever the buffer holds around it.
    #[test]
    fn s2a_03_an_array_in_a_page_behaves_like_a_vec(ops in array_ops(), pad in 0usize..8) {
        let mut page = vec![0xEEu8; pad + CAP * 4 + 8];
        let mut model: Vec<i32> = Vec::new();
        {
            let mut array = PageArray::<_, i32>::new(&mut page[pad..pad + CAP * 4]);
            prop_assert_eq!(array.capacity(), CAP);
            for op in ops {
                match op {
                    ArrayOp::Insert(i, v) if model.len() < CAP => { let at = i % (model.len() + 1); array.insert_at(at, model.len(), &v); model.insert(at, v); }
                    ArrayOp::Remove(i) if !model.is_empty() => { let at = i % model.len(); array.remove_at(at, model.len()); model.remove(at); }
                    ArrayOp::Set(i, v) if !model.is_empty() => { let at = i % model.len(); array.set(at, &v); model[at] = v; }
                    _ => {}
                }
                let got: Vec<i32> = (0..model.len()).map(|i| array.get(i)).collect();
                prop_assert_eq!(&got, &model, "the array must read as the model after {:?}", op);
            }
        }
        prop_assert!(page[..pad].iter().all(|&b| b == 0xEE), "the array wrote before its bytes");
        prop_assert!(page[pad + CAP * 4..].iter().all(|&b| b == 0xEE), "the array wrote after its bytes");
    }

    #[test]
    fn s2a_03_pairs_move_whole_and_overlapping_shifts_do_not_smear(values in prop::collection::vec((any::<i64>(), any::<u32>()), 1..14), at in 0usize..14) {
        type E = (i64, Rid);
        let entries: Vec<E> = values.iter().map(|&(a, s)| (a, Rid::new(PageId(a as i32), s))).collect();
        let mut array = PageArray::<_, E>::new(vec![0u8; 16 * E::SIZE]);
        for (i, e) in entries.iter().enumerate() { array.set(i, e); }
        let at = at % (entries.len() + 1);
        let extra = (-1i64, Rid::new(PageId(-1), 99));
        array.insert_at(at, entries.len(), &extra);
        let mut model = entries.clone();
        model.insert(at, extra);
        prop_assert_eq!((0..model.len()).map(|i| array.get(i)).collect::<Vec<_>>(), model.clone());
        array.remove_at(at, model.len());
        prop_assert_eq!((0..entries.len()).map(|i| array.get(i)).collect::<Vec<_>>(), entries);
    }
}

#[test]
fn s2a_03_capacity_counts_whole_entries_only() {
    assert_eq!(PageArray::<_, i64>::new(vec![0u8; 100]).capacity(), 12);
    assert_eq!(PageArray::<_, i32>::new(&[0u8; 3][..]).capacity(), 0);
}

#[test]
fn s2a_03_it_works_over_a_read_only_view_and_an_owned_buffer() {
    let mut owned = PageArray::<_, u32>::new(vec![0u8; 16]);
    owned.set(2, &77);
    let bytes: Vec<u8> = (0..16).map(|i| i as u8).collect();
    let view = PageArray::<_, u32>::new(&bytes[..]);
    assert_eq!(owned.get(2), 77, "it works over a read only view and an owned buffer");
    assert_eq!(view.capacity(), 4, "it works over a read only view and an owned buffer");
}

#[test]
fn s2a_03_going_past_the_capacity_is_a_bug_and_panics() {
    let get = std::panic::catch_unwind(|| PageArray::<_, i32>::new(vec![0u8; 8]).get(2));
    assert!(get.is_err(), "get(2) on a capacity of 2");
    let set = std::panic::catch_unwind(|| PageArray::<_, i32>::new(vec![0u8; 8]).set(2, &1));
    assert!(set.is_err(), "set(2) on a capacity of 2");
    let full = std::panic::catch_unwind(|| {
        let mut a = PageArray::<_, i32>::new(vec![0u8; 8]);
        a.insert_at(0, 2, &1); // already holds 2 = full
    });
    assert!(full.is_err(), "inserting into a full array");
    let past_len = std::panic::catch_unwind(|| {
        let mut a = PageArray::<_, i32>::new(vec![0u8; 16]);
        a.insert_at(3, 1, &1); // beyond len
    });
    assert!(past_len.is_err(), "inserting beyond len");
    let remove = std::panic::catch_unwind(|| PageArray::<_, i32>::new(vec![0u8; 16]).remove_at(1, 1));
    assert!(remove.is_err(), "removing entry 1 of a length 1");
}

// ---- 2a-04 · Searching a page -----------------------------------------------------------------------------------------------

fn sorted_array(values: &[i32]) -> PageArray<Vec<u8>, i32> {
    let mut array = PageArray::new(vec![0u8; values.len().max(1) * 4 + 16]);
    for (i, v) in values.iter().enumerate() {
        array.set(i, v);
    }
    array
}

proptest! {
    #![proptest_config(config())]

    /// `lower_bound` is the first position whose entry is not less than the target: exactly `partition_point`, duplicates included.
    #[test]
    fn s2a_04_lower_bound_is_the_partition_point(mut values in prop::collection::vec(-20i32..20, 0..30), target in -22i32..22) {
        values.sort();
        let array = sorted_array(&values);
        let got = array.lower_bound(values.len(), |e| e.cmp(&target));
        prop_assert_eq!(got, values.partition_point(|&v| v < target), "lower_bound({}) in {:?}", target, values);
    }

    /// Only the first `len` entries count: stale bytes after them must not be searched.
    #[test]
    fn s2a_04_entries_past_len_are_not_searched(mut values in prop::collection::vec(-20i32..20, 1..30), cut in 0usize..30, target in -22i32..22) {
        values.sort();
        let len = cut % (values.len() + 1);
        let array = sorted_array(&values);
        prop_assert_eq!(array.lower_bound(len, |e| e.cmp(&target)), values[..len].partition_point(|&v| v < target));
    }

    /// Inserting at `lower_bound` keeps an array sorted: the page-sized version of a sorted set.
    #[test]
    fn s2a_04_inserting_at_the_lower_bound_keeps_the_array_sorted(inserts in prop::collection::vec(-50i32..50, 1..40)) {
        let mut array = PageArray::<_, i32>::new(vec![0u8; 64 * 4]);
        let mut model: Vec<i32> = Vec::new();
        for v in inserts {
            let at = array.lower_bound(model.len(), |e| e.cmp(&v));
            if at < model.len() && array.get(at) == v { continue; }
            array.insert_at(at, model.len(), &v);
            model.insert(model.partition_point(|&m| m < v), v);
            prop_assert_eq!((0..model.len()).map(|i| array.get(i)).collect::<Vec<_>>(), model.clone());
        }
    }

    /// Searching entries that are `(GenericKey, Rid)` with the key comparator, the way a bucket page does.
    #[test]
    fn s2a_04_a_search_by_key_finds_the_right_entry(mut keys in prop::collection::vec(-100i64..100, 1..25), probe in -102i64..102) {
        type E = (GenericKey<8>, Rid);
        keys.sort();
        keys.dedup();
        let mut array = PageArray::<_, E>::new(vec![0u8; 32 * E::SIZE]);
        for (i, &k) in keys.iter().enumerate() { array.set(i, &(key(k), Rid::new(PageId(k as i32), i as u32))); }
        let cmp = GenericComparator::<8>;
        let target = key::<8>(probe);
        let at = array.lower_bound(keys.len(), |(k, _)| cmp.compare(k, &target));
        prop_assert_eq!(at, keys.partition_point(|&k| k < probe));
        if let Some(pos) = keys.iter().position(|&k| k == probe) {
            prop_assert_eq!(pos, at);
            prop_assert_eq!(array.get(at).1, Rid::new(PageId(probe as i32), at as u32));
        }
    }
}

#[test]
fn s2a_04_a_target_larger_than_everything_gives_len() {
    let array = sorted_array(&[1, 3, 5]);
    assert_eq!(array.lower_bound(3, |e| e.cmp(&9)), 3, "a target larger than everything gives len");
    assert_eq!(array.lower_bound(3, |e| e.cmp(&0)), 0, "a target larger than everything gives len");
    assert_eq!(array.lower_bound(0, |e| e.cmp(&0)), 0, "an empty array");
}

#[test]
fn s2a_04_the_comparison_closure_is_called_logarithmically_often() {
    let values: Vec<i32> = (0..1024).collect();
    let array = sorted_array(&values);
    let mut calls = 0;
    let at = array.lower_bound(values.len(), |e| {
        calls += 1;
        e.cmp(&700)
    });
    assert_eq!(at, 700, "the comparison closure is called logarithmically often");
    assert!(calls <= 12, "binary search on 1024 entries needs about 10 comparisons, yours made {calls}");
}

// ---- 2a-05 · Boss: the pieces on a real page ---------------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..config() })]

    /// A sorted bucket page (a length, a capacity, then `(key, rid)` entries kept sorted) built only from the pieces of this module,
    /// living in the buffer pool: after any sequence of inserts and removes, and after the page has been evicted and read back, it still
    /// holds exactly the keys of a `BTreeMap` model.
    #[test]
    fn s2a_05_a_sorted_bucket_survives_the_buffer_pool(ops in prop::collection::vec((any::<bool>(), 0i64..60), 1..120)) {
        type Entry = (GenericKey<8>, Rid);
        let (bpm, _disk) = pool_with(Policy::Fifo, 2);
        let page = bpm.new_page();
        let cmp = GenericComparator::<8>;
        let max = array_size(HTABLE_BUCKET_PAGE_METADATA_SIZE, Entry::SIZE);
        let mut model = std::collections::BTreeMap::new();
        {
            let mut guard = bpm.write_page(page);
            let data = guard.get_data_mut();
            data[..4].copy_from_slice(&0u32.to_le_bytes());
            data[4..8].copy_from_slice(&(max as u32).to_le_bytes());
        }
        for (i, (insert, n)) in ops.into_iter().enumerate() {
            let mut guard = bpm.write_page(page);
            let data = guard.get_data_mut();
            let len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            let mut array = PageArray::<_, Entry>::new(&mut data[HTABLE_BUCKET_PAGE_METADATA_SIZE..]);
            let target = key::<8>(n);
            let at = array.lower_bound(len, |(k, _)| cmp.compare(k, &target));
            let present = at < len && array.get(at).0.get_as_integer() == n;
            let new_len = if insert && !present {
                array.insert_at(at, len, &(target, Rid::new(PageId(n as i32), i as u32)));
                model.insert(n, i as u32);
                len + 1
            } else if !insert && present {
                array.remove_at(at, len);
                model.remove(&n);
                len - 1
            } else { len };
            data[..4].copy_from_slice(&(new_len as u32).to_le_bytes());
            drop(guard);
            if i % 17 == 0 { // push the page out of the two-frame pool
                for _ in 0..3 { let other = bpm.new_page(); drop(bpm.write_page(other)); }
            }
        }
        let guard = bpm.read_page(page);
        let data = guard.get_data();
        let len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
        let array = PageArray::<_, Entry>::new(&data[HTABLE_BUCKET_PAGE_METADATA_SIZE..]);
        let got: Vec<(i64, u32)> = (0..len).map(|i| { let (k, r) = array.get(i); (k.get_as_integer(), r.slot_num()) }).collect();
        let want: Vec<(i64, u32)> = model.into_iter().collect();
        prop_assert_eq!(got, want);
    }
}

#[test]
fn s2a_05_an_ordering_is_a_total_order_on_the_keys_it_will_index() {
    let g = GenericComparator::<8>;
    let ks: Vec<GenericKey<8>> = [-3i64, -1, 0, 0, 5, i64::MIN, i64::MAX].iter().map(|&n| key(n)).collect();
    for a in &ks {
        assert_eq!(g.compare(a, a), Ordering::Equal, "an ordering is a total order on the keys it will index");
        for b in &ks {
            assert_eq!(g.compare(a, b), g.compare(b, a).reverse(), "antisymmetry");
            for c in &ks {
                if g.compare(a, b) != Ordering::Greater && g.compare(b, c) != Ordering::Greater {
                    assert_ne!(g.compare(a, c), Ordering::Greater, "transitivity");
                }
            }
        }
    }
}

// ---- 2a-c1 and 2a-c2: challenges ------------------------------------------------------------------------------------------------------------

use bustub::storage::page::bound_search::{count_of, lower_bound, upper_bound};
use bustub::storage::page::ranked_array::RankedSet;

fn ch_config() -> ProptestConfig {
    ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() }
}

#[test]
fn s2a_c1_rank_counts_the_smaller_numbers_whether_or_not_the_number_is_there() {
    let mut s = RankedSet::new();
    for x in [10, 30, 20, 40] {
        assert!(s.insert(x), "s2a c1 rank counts the smaller numbers whether or not the number is there: expected `s.insert(x)`");
    }
    assert_eq!(
        (s.rank(5), s.rank(10), s.rank(15), s.rank(40), s.rank(41)),
        (0, 0, 1, 3, 4),
        "s2a c1 rank counts the smaller numbers whether or not the number is there"
    );
}

#[test]
fn s2a_c1_select_is_the_inverse_of_rank() {
    let mut s = RankedSet::new();
    for x in [7, 3, 9, 5] {
        s.insert(x);
    }
    assert_eq!(
        (s.select(0), s.select(1), s.select(3), s.select(4)),
        (Some(3), Some(5), Some(9), None),
        "s2a c1 select is the inverse of rank"
    );
    for i in 0..s.len() {
        assert_eq!(s.rank(s.select(i).unwrap()), i, "s2a c1 select is the inverse of rank");
    }
}

#[test]
fn s2a_c1_inserting_twice_and_removing_what_is_not_there_say_so() {
    let mut s = RankedSet::new();
    assert!(s.insert(1), "s2a c1 inserting twice and removing what is not there say so: expected `s.insert(1)`");
    assert!(!s.insert(1), "s2a c1 inserting twice and removing what is not there say so: expected `!s.insert(1)`");
    assert_eq!(s.len(), 1, "s2a c1 inserting twice and removing what is not there say so");
    assert!(!s.remove(2), "s2a c1 inserting twice and removing what is not there say so: expected `!s.remove(2)`");
    assert!(s.remove(1), "s2a c1 inserting twice and removing what is not there say so: expected `s.remove(1)`");
    assert!(s.is_empty(), "s2a c1 inserting twice and removing what is not there say so: expected `s.is_empty()`");
    assert_eq!((s.rank(100), s.select(0)), (0, None), "s2a c1 inserting twice and removing what is not there say so");
}

#[test]
fn s2a_c1_count_range_is_half_open_and_forgiving() {
    let mut s = RankedSet::new();
    for x in 0..10u64 {
        s.insert(x * 10);
    }
    assert_eq!(s.count_range(20, 50), 3, "20, 30 and 40");
    assert_eq!(s.count_range(20, 21), 1, "s2a c1 count range is half open and forgiving");
    assert_eq!(s.count_range(50, 20), 0, "an empty or reversed range");
    assert_eq!(s.count_range(0, u64::MAX), 10, "s2a c1 count range is half open and forgiving");
    assert_eq!(s.count_range(91, 1000), 0, "s2a c1 count range is half open and forgiving");
}

#[test]
fn s2a_c1_a_large_set_answers_quickly() {
    let mut s = RankedSet::new();
    for x in (0..20_000u64).rev() {
        s.insert(x * 3);
    }
    let t = std::time::Instant::now();
    let mut total = 0;
    for q in 0..200_000u64 {
        total += s.rank(q);
    }
    assert!(total > 0, "s2a c1 a large set answers quickly: expected `total > 0`");
    assert!(
        t.elapsed() < std::time::Duration::from_secs(5),
        "200 000 ranks over 20 000 numbers took {:?}: rank must not scan",
        t.elapsed()
    );
}

proptest! {
    #![proptest_config(ch_config())]

    /// Property: against a `BTreeSet` and counting by brute force, after any sequence of inserts and removes.
    #[test]
    fn s2a_c1_property_rank_and_select_match_a_model(ops in proptest::collection::vec((any::<bool>(), 0u64..40), 0..80), probes in proptest::collection::vec((0u64..45, 0u64..45), 1..10)) {
        let mut s = RankedSet::new();
        let mut m = std::collections::BTreeSet::new();
        for (ins, x) in ops {
            if ins { prop_assert_eq!(s.insert(x), m.insert(x)); } else { prop_assert_eq!(s.remove(x), m.remove(&x)); }
        }
        prop_assert_eq!(s.len(), m.len());
        for (a, b) in probes {
            prop_assert_eq!(s.rank(a), m.range(..a).count());
            prop_assert_eq!(s.count_range(a, b), if b <= a { 0 } else { m.range(a..b).count() });
        }
        for i in 0..m.len() + 2 {
            prop_assert_eq!(s.select(i), m.iter().nth(i).copied());
        }
    }
}

#[test]
fn s2a_c2_the_bounds_of_a_run_of_equal_keys() {
    let v = [1, 3, 3, 3, 7];
    assert_eq!((lower_bound(&v, 3), upper_bound(&v, 3)), (1, 4), "s2a c2 the bounds of a run of equal keys");
    assert_eq!(count_of(&v, 3), 3, "s2a c2 the bounds of a run of equal keys");
    assert_eq!(count_of(&v, 1), 1, "s2a c2 the bounds of a run of equal keys");
}

#[test]
fn s2a_c2_a_key_that_is_not_there_has_equal_bounds() {
    let v = [1, 3, 3, 7];
    assert_eq!((lower_bound(&v, 5), upper_bound(&v, 5)), (3, 3), "s2a c2 a key that is not there has equal bounds");
    assert_eq!((lower_bound(&v, 0), upper_bound(&v, 0)), (0, 0), "s2a c2 a key that is not there has equal bounds");
    assert_eq!((lower_bound(&v, 100), upper_bound(&v, 100)), (4, 4), "s2a c2 a key that is not there has equal bounds");
    assert_eq!(count_of(&v, 5), 0, "s2a c2 a key that is not there has equal bounds");
}

#[test]
fn s2a_c2_the_empty_slice_and_the_all_equal_slice() {
    assert_eq!((lower_bound(&[], 1), upper_bound(&[], 1)), (0, 0), "s2a c2 the empty slice and the all equal slice");
    let v = [5; 9];
    assert_eq!((lower_bound(&v, 5), upper_bound(&v, 5)), (0, 9), "s2a c2 the empty slice and the all equal slice");
    assert_eq!((lower_bound(&v, 4), upper_bound(&v, 4)), (0, 0), "s2a c2 the empty slice and the all equal slice");
    assert_eq!((lower_bound(&v, 6), upper_bound(&v, 6)), (9, 9), "s2a c2 the empty slice and the all equal slice");
}

#[test]
fn s2a_c2_negative_keys_and_the_extremes() {
    let v = [i64::MIN, -5, -5, 0, i64::MAX];
    assert_eq!(count_of(&v, -5), 2, "s2a c2 negative keys and the extremes");
    assert_eq!(count_of(&v, i64::MIN), 1, "s2a c2 negative keys and the extremes");
    assert_eq!(count_of(&v, i64::MAX), 1, "s2a c2 negative keys and the extremes");
    assert_eq!(upper_bound(&v, i64::MAX), 5, "s2a c2 negative keys and the extremes");
}

proptest! {
    #![proptest_config(ch_config())]

    /// Property: the bounds are what `partition_point` says, for any sorted slice with duplicates.
    #[test]
    fn s2a_c2_property_bounds_match_partition_point(mut v in proptest::collection::vec(-6i64..6, 0..40), key in -8i64..8) {
        v.sort();
        prop_assert_eq!(lower_bound(&v, key), v.partition_point(|&x| x < key));
        prop_assert_eq!(upper_bound(&v, key), v.partition_point(|&x| x <= key));
        prop_assert_eq!(count_of(&v, key), v.iter().filter(|&&x| x == key).count());
    }
}

// @@ challenge 2a-c3 begin
mod ch_2a_c3 {
    use proptest::prelude::*;

    use bustub::storage::page::slotted_page::SlottedPage;

    #[test]
    fn s2a_c3_records_come_back_and_space_is_accounted() {
        let mut p = SlottedPage::new(20);
        assert_eq!(p.insert(b"abcd"), Some(0), "this check");
        assert_eq!(p.free_space(), 12, "this check");
        assert_eq!(p.insert(b"wxyz"), Some(1), "this check");
        assert_eq!(p.free_space(), 4, "this check");
        assert_eq!(p.insert(b"q"), None, "1 byte + 4 does not fit in 4... it needs 5");
        assert_eq!((p.get(0), p.get(1), p.get(2)), (Some(&b"abcd"[..]), Some(&b"wxyz"[..]), None), "this check");
    }

    #[test]
    fn s2a_c3_a_deleted_record_gives_its_space_and_its_slot_back() {
        let mut p = SlottedPage::new(20);
        p.insert(b"abcd");
        p.insert(b"wxyz");
        assert!(p.delete(0), "this check: expected `p.delete(0)`");
        assert!(!p.delete(0), "already free");
        assert_eq!(p.free_space(), 12, "this check");
        assert_eq!(p.insert(b"1234"), Some(0), "the lowest free slot is reused");
        assert_eq!(p.get(0), Some(&b"1234"[..]), "this check");
    }

    #[test]
    fn s2a_c3_scattered_free_space_is_usable_for_one_bigger_record() {
        let mut p = SlottedPage::new(100);
        let slots: Vec<_> = (0..4).map(|i| p.insert(&[i as u8; 20]).unwrap()).collect(); // 4 * 24 = 96 bytes
        assert_eq!(p.insert(&[9; 5]), None, "this check");
        p.delete(slots[0]);
        p.delete(slots[2]);
        assert_eq!(p.free_space(), 52, "this check");
        assert!(p.insert(&[7; 48]).is_some(), "48 + 4 = 52: two holes of 24 make room for one record of 48");
        assert_eq!(p.get(slots[1]), Some(&[1u8; 20][..]), "this check");
        assert_eq!(p.get(slots[3]), Some(&[3u8; 20][..]), "the records that stayed are intact");
    }

    #[test]
    fn s2a_c3_empty_records_cost_only_their_slot_entry() {
        let mut p = SlottedPage::new(8);
        assert_eq!(p.insert(b""), Some(0), "this check");
        assert_eq!(p.insert(b""), Some(1), "this check");
        assert_eq!(p.insert(b""), None, "this check");
        assert_eq!(p.get(0), Some(&b""[..]), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a model of slots with a byte budget: lowest free slot, fit by total free space, records intact.
        #[test]
        fn s2a_c3_property_a_page_matches_a_model(size in 8usize..120, ops in proptest::collection::vec((any::<bool>(), 0usize..24, any::<u8>()), 0..60)) {
            let mut p = SlottedPage::new(size);
            let mut m: Vec<Option<Vec<u8>>> = Vec::new();
            for (ins, n, byte) in ops {
                if ins {
                    let rec = vec![byte; n];
                    let used: usize = m.iter().flatten().map(|r| r.len() + 4).sum();
                    let want = if n + 4 > size - used { None } else {
                        let slot = m.iter().position(|s| s.is_none()).unwrap_or_else(|| { m.push(None); m.len() - 1 });
                        m[slot] = Some(rec.clone());
                        Some(slot)
                    };
                    prop_assert_eq!(p.insert(&rec), want);
                } else {
                    let slot = n % (m.len() + 1);
                    let want = m.get_mut(slot).is_some_and(|s| s.take().is_some());
                    prop_assert_eq!(p.delete(slot), want);
                }
                let used: usize = m.iter().flatten().map(|r| r.len() + 4).sum();
                prop_assert_eq!(p.free_space(), size - used);
                for (i, s) in m.iter().enumerate() {
                    prop_assert_eq!(p.get(i), s.as_deref());
                }
            }
        }
    }
}
// @@ challenge 2a-c3 end

// @@ challenge 2a-c4 begin
mod ch_2a_c4 {
    use proptest::prelude::*;

    use bustub::storage::page::key_block::{decode_keys, encode_keys};

    fn k(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    #[test]
    fn s2a_c4_the_exact_bytes_of_a_small_block() {
        let block = encode_keys(&[k("apple"), k("apply"), k("banana")]);
        let mut want = vec![3, 0, 5];
        want.extend_from_slice(b"apple");
        want.extend_from_slice(&[4, 1]);
        want.extend_from_slice(b"y");
        want.extend_from_slice(&[0, 6]);
        want.extend_from_slice(b"banana");
        assert_eq!(block, want, "this check");
    }

    #[test]
    fn s2a_c4_blocks_round_trip_including_empty_ones_and_equal_keys() {
        for keys in [vec![], vec![k("")], vec![k("a"), k("a"), k("ab")], vec![k("x"); 5]] {
            assert_eq!(decode_keys(&encode_keys(&keys)), Some(keys.clone()), "{keys:?}");
        }
    }

    #[test]
    fn s2a_c4_alike_keys_take_less_room_than_unlike_ones() {
        let alike: Vec<_> = (0..20).map(|i| k(&format!("/users/42/orders/{i:03}"))).collect();
        let plain: usize = alike.iter().map(|x| x.len()).sum();
        assert!(encode_keys(&alike).len() < plain / 2, "20 keys sharing 16 bytes should be well under half their plain size");
    }

    #[test]
    fn s2a_c4_malformed_blocks_are_rejected_not_panicked_on() {
        assert_eq!(decode_keys(&[]), None, "no count");
        assert_eq!(decode_keys(&[2, 0, 1, b'a']), None, "the second key is missing");
        assert_eq!(decode_keys(&[1, 3, 1, b'a']), None, "shared is longer than the (empty) previous key");
        assert_eq!(decode_keys(&[1, 0, 5, b'a']), None, "truncated rest");
        assert_eq!(decode_keys(&[1, 0, 1, b'a', 9]), None, "trailing byte");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: round trip and the size formula, for any sorted keys.
        #[test]
        fn s2a_c4_property_round_trip_and_size(mut keys in proptest::collection::vec(proptest::collection::vec(0u8..4, 0..12), 0..20)) {
            keys.sort();
            let block = encode_keys(&keys);
            prop_assert_eq!(decode_keys(&block), Some(keys.clone()));
            let mut want = 1;
            let mut prev: &[u8] = &[];
            for key in &keys {
                let shared = prev.iter().zip(key).take_while(|(a, b)| a == b).count();
                want += 2 + key.len() - shared;
                prev = key;
            }
            prop_assert_eq!(block.len(), want);
            for cut in 0..block.len() {
                prop_assert_eq!(decode_keys(&block[..cut]), None, "a block cut at {} must be rejected", cut);
            }
        }

        /// Property: decoding any bytes never panics, and what it accepts re-encodes to the same bytes.
        #[test]
        fn s2a_c4_property_arbitrary_bytes_decode_or_fail_cleanly(bytes in proptest::collection::vec(any::<u8>(), 0..40)) {
            if let Some(keys) = decode_keys(&bytes) {
                prop_assert_eq!(encode_keys(&keys), bytes);
            }
        }
    }
}
// @@ challenge 2a-c4 end

// @@ challenge 2a-c5 begin
mod ch_2a_c5 {
    use proptest::prelude::*;

    use bustub::storage::page::shift_array::{insert_at, remove_at};

    #[test]
    fn s2a_c5_inserting_in_the_middle_shifts_the_tail_without_smearing() {
        let mut a = [1, 2, 3, 0];
        assert_eq!(insert_at(&mut a, 3, 1, 9), 4, "this check");
        assert_eq!(a, [1, 9, 2, 3], "this check");
    }

    #[test]
    fn s2a_c5_inserting_at_the_front_and_at_the_end() {
        let mut a = [5, 6, 7, 0, 0];
        assert_eq!(insert_at(&mut a, 3, 0, 1), 4, "this check");
        assert_eq!(a, [1, 5, 6, 7, 0], "this check");
        assert_eq!(insert_at(&mut a, 4, 4, 8), 5, "this check");
        assert_eq!(a, [1, 5, 6, 7, 8], "this check");
    }

    #[test]
    fn s2a_c5_removing_closes_the_gap() {
        let mut a = [1, 9, 2, 3];
        assert_eq!(remove_at(&mut a, 4, 1), 3, "this check");
        assert_eq!(&a[..3], &[1, 2, 3], "this check");
        assert_eq!(remove_at(&mut a, 3, 0), 2, "this check");
        assert_eq!(&a[..2], &[2, 3], "this check");
        assert_eq!(remove_at(&mut a, 2, 1), 1, "this check");
        assert_eq!(&a[..1], &[2], "this check");
    }

    #[test]
    fn s2a_c5_inserting_into_a_full_prefix_up_to_capacity_and_removing_everything() {
        let mut a = [0u32; 6];
        let mut len = 0;
        for v in (1..=6).rev() {
            len = insert_at(&mut a, len, 0, v);
        }
        assert_eq!(a, [1, 2, 3, 4, 5, 6], "each insert at the front shifts everything");
        while len > 0 {
            len = remove_at(&mut a, len, 0);
        }
        assert_eq!(len, 0, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against `Vec::insert` and `Vec::remove` on the live prefix.
        #[test]
        fn s2a_c5_property_shifting_matches_a_vec(init in proptest::collection::vec(any::<u32>(), 0..12), ops in proptest::collection::vec((any::<bool>(), 0usize..14, any::<u32>()), 0..30)) {
            let mut arr = [0u32; 16];
            arr[..init.len()].copy_from_slice(&init);
            let mut len = init.len();
            let mut model = init;
            for (ins, i, v) in ops {
                if ins && len < arr.len() {
                    let at = i % (len + 1);
                    len = insert_at(&mut arr, len, at, v);
                    model.insert(at, v);
                } else if !ins && len > 0 {
                    let at = i % len;
                    len = remove_at(&mut arr, len, at);
                    model.remove(at);
                }
                prop_assert_eq!(&arr[..len], &model[..]);
            }
        }
    }
}
// @@ challenge 2a-c5 end
