//! Tests for module 0d: sketches and a CRDT: a count-min sketch, HyperLogLog and an observed-remove set.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bustub::primer::count_min_sketch::CountMinSketch;
use bustub::primer::hyperloglog::{HyperLogLog, HyperLogLogPresto};
use bustub::primer::orset::{ORSet, ORSetDriver};
use proptest::prelude::*;

fn cms<K: std::hash::Hash>(width: u32, depth: u32) -> CountMinSketch<K> {
    CountMinSketch::new(width, depth).unwrap()
}

// ---- 0d-01: count-min sketch: insert and count -----------------------------------------------------------------------------------------

#[test]
fn s0d_01_a_zero_dimension_is_an_error() {
    for i in [10u32, 20, 30, 40] {
        assert!(CountMinSketch::<i32>::new(0, i).is_err(), "a zero dimension is an error: expected `CountMinSketch::<i32>::new(0, i).is_err()`");
        assert!(CountMinSketch::<i32>::new(i * 5, 0).is_err(), "a zero dimension is an error: expected `CountMinSketch::<i32>::new(i * 5, 0).is_err()`");
    }
    assert!(CountMinSketch::<i32>::new(1, 1).is_ok(), "a zero dimension is an error: expected `CountMinSketch::<i32>::new(1, 1).is_ok()`");
}

#[test]
fn s0d_01_counts_of_strings() {
    let s: CountMinSketch<String> = cms(200, 12);
    let key = |x: &str| x.to_string();
    assert_eq!(s.count(&key("test")), 0, "counts of strings");
    s.insert(&key("Welcome to CMU DB (15-445/645)"));
    assert_eq!(s.count(&key("Welcome to CMU DB (15-445/645)")), 1, "counts of strings");
    let names = ["DJ-Cache", "Sirui", "Andy", "Melody", "William", "Saransh", "Song", "Ruiqi", "David"];
    for i in 0..10u32 {
        for n in names {
            s.insert(&key(n));
        }
        for n in names {
            assert_eq!(s.count(&key(n)), i + 1, "counts of strings");
        }
    }
    assert_eq!(s.count(&key("NonExistent")), 0, "counts of strings");
}

#[test]
fn s0d_01_counts_of_integers_including_negative_ones() {
    let s: CountMinSketch<i64> = cms(500, 20);
    assert_eq!(s.count(&0), 0, "counts of integers including negative ones");
    s.insert(&0);
    for i in 0..30u32 {
        for j in 0..4u32 {
            for k in [10i64, 122, 200, 911, 15445] {
                s.insert(&k);
            }
            for k in [10i64, 122, 200, 911, 15445] {
                assert_eq!(s.count(&k), i * 4 + j + 1, "counts of integers including negative ones");
            }
        }
        for j in 0..5u32 {
            for k in [-1i64, -2, -3, -15445] {
                s.insert(&k);
            }
            for k in [-1i64, -2, -3, -15445] {
                assert_eq!(s.count(&k), i * 5 + j + 1, "counts of integers including negative ones");
            }
        }
    }
    assert_eq!((s.count(&0), s.count(&10), s.count(&-1), s.count(&999999)), (1, 120, 150, 0), "counts of integers including negative ones");
}

#[test]
fn s0d_01_width_one_makes_every_item_collide_and_the_minimum_is_the_total() {
    let s: CountMinSketch<i64> = cms(1, 20);
    s.insert(&1);
    s.insert(&2);
    assert_eq!((s.count(&1), s.count(&2)), (2, 2), "width one makes every item collide and the minimum is the total");
    s.insert(&3);
    s.insert(&4);
    for k in 1..=4 {
        assert_eq!(s.count(&k), 4, "width one makes every item collide and the minimum is the total");
    }
    let one: CountMinSketch<i64> = cms(1, 1);
    for k in 0..5 {
        one.insert(&k);
    }
    assert_eq!((one.count(&0), one.count(&999), one.count(&-1)), (5, 5, 5), "even an item never inserted");
}

#[test]
fn s0d_01_the_estimate_is_never_below_the_truth_with_one_row() {
    let s: CountMinSketch<i64> = cms(50, 1);
    s.insert(&15445);
    s.insert(&(15445 + 4));
    s.insert(&15445);
    assert!(s.count(&15445) >= 2, "the estimate is never below the truth with one row: expected `s.count(&15445) >= 2`");
    assert!(s.count(&(15445 + 4)) >= 1, "the estimate is never below the truth with one row: expected `s.count(&(15445 + 4)) >= 1`");
}

#[test]
fn s0d_01_each_row_hashes_differently_but_always_the_same_way() {
    let s: CountMinSketch<i64> = cms(1000, 8);
    let columns: Vec<usize> = (0..8).map(|row| s.column(row, &12345)).collect();
    assert_eq!(columns, (0..8).map(|row| s.column(row, &12345)).collect::<Vec<_>>(), "each row hashes differently but always the same way");
    assert!(columns.iter().all(|c| *c < 1000), "each row hashes differently but always the same way: expected `columns.iter().all(|c| *c < 1000)`");
    let distinct: std::collections::BTreeSet<_> = columns.iter().collect();
    assert!(distinct.len() >= 5, "rows should not all pick the same column: {columns:?}");
}

// ---- 0d-01: clear, merge and top-k ------------------------------------------------------------------------------------------------------------

#[test]
fn s0d_01_clear_resets_every_count() {
    let s: CountMinSketch<i32> = cms(200, 10);
    for (item, times) in [(1, 15), (2, 10), (3, 8)] {
        for _ in 0..times {
            s.insert(&item);
        }
    }
    assert_eq!((s.count(&1), s.count(&2), s.count(&3)), (15, 10, 8), "clear resets every count");
    s.clear();
    assert_eq!((s.count(&1), s.count(&2), s.count(&3), s.count(&999)), (0, 0, 0, 0), "clear resets every count");
    s.insert(&1);
    assert_eq!(s.count(&1), 1, "clear resets every count");
}

#[test]
fn s0d_01_merge_adds_the_counts_of_two_sketches() {
    let (a, b): (CountMinSketch<String>, CountMinSketch<String>) = (cms(250, 8), cms(250, 8));
    let key = |x: &str| x.to_string();
    for (item, times) in [("055", 5), ("4987", 2), ("3125", 3), ("2256", 1)] {
        for _ in 0..times {
            a.insert(&key(item));
        }
    }
    for (item, times) in [("4739", 5), ("3125", 2), ("4987", 3), ("2256", 4)] {
        for _ in 0..times {
            b.insert(&key(item));
        }
    }
    a.merge(&b).unwrap();
    for item in ["055", "4987", "4739", "2256", "3125"] {
        assert_eq!(a.count(&key(item)), 5, "{item}");
    }
    assert_eq!((b.count(&key("4739")), b.count(&key("3125"))), (5, 2), "the other sketch is unchanged");
}

#[test]
fn s0d_01_merge_with_collisions_and_with_incompatible_sketches() {
    let (a, b): (CountMinSketch<i32>, CountMinSketch<i32>) = (cms(1, 20), cms(1, 20));
    for k in [1, 2, 5] {
        a.insert(&k);
    }
    for k in [3, 4] {
        b.insert(&k);
    }
    a.merge(&b).unwrap();
    assert_eq!((a.count(&1), a.count(&996)), (5, 5), "merge with collisions and with incompatible sketches");
    let other: CountMinSketch<i32> = cms(2, 20);
    assert!(a.merge(&other).is_err(), "merge with collisions and with incompatible sketches: expected `a.merge(&other).is_err()`");
    let other: CountMinSketch<i32> = cms(1, 19);
    assert!(a.merge(&other).is_err(), "merge with collisions and with incompatible sketches: expected `a.merge(&other).is_err()`");
}

#[test]
fn s0d_01_top_k_orders_the_candidates_by_estimated_count() {
    for iter in 1..10u32 {
        let s: CountMinSketch<String> = cms(10, 3);
        for (item, times) in [("frequent", iter + 4), ("medium", iter + 2), ("rare", iter)] {
            for _ in 0..times {
                s.insert(&item.to_string());
            }
        }
        let candidates: Vec<String> = ["rare", "frequent", "medium"].iter().map(|s| s.to_string()).collect();
        let top = s.top_k(3, &candidates);
        assert_eq!(top, vec![("frequent".to_string(), iter + 4), ("medium".to_string(), iter + 2), ("rare".to_string(), iter)], "top k orders the candidates by estimated count");
    }
}

#[test]
fn s0d_01_top_k_keeps_at_most_k_and_at_most_the_candidates() {
    let s: CountMinSketch<i32> = cms(200, 15);
    for (item, times) in [(1, 1), (2, 5), (3, 3), (4, 9)] {
        for _ in 0..times {
            s.insert(&item);
        }
    }
    let top: Vec<i32> = s.top_k(3, &[1, 2, 3, 4]).into_iter().map(|p| p.0).collect();
    assert_eq!(top, vec![4, 2, 3], "top k keeps at most k and at most the candidates");
    assert_eq!(s.top_k(10, &[1, 2]).len(), 2, "top k keeps at most k and at most the candidates");
    assert!(s.top_k(0, &[1, 2]).is_empty(), "top k keeps at most k and at most the candidates: expected `s.top_k(0, &[1, 2]).is_empty()`");
    assert_eq!(s.top_k(2, &[7, 8]).iter().map(|p| p.1).collect::<Vec<_>>(), vec![0, 0], "unseen candidates count 0 and keep their order");
}

#[test]
fn s0d_01_top_k_tracks_a_sketch_that_keeps_counting() {
    // BusTub's TopKDynamicTest: the same sketch gets more and more inserts
    let cases: [[u32; 4]; 6] = [[1, 2, 3, 4], [7, 5, 3, 1], [2, 2, 5, 7], [6, 6, 2, 2], [1, 3, 6, 6], [400, 200, 300, 100]];
    let expected: [[i32; 3]; 6] = [[4, 3, 2], [1, 2, 3], [4, 3, 1], [1, 2, 4], [4, 3, 2], [1, 3, 2]];
    let s: CountMinSketch<i32> = cms(200, 15);
    for (case, want) in cases.iter().zip(expected) {
        for (i, times) in case.iter().enumerate() {
            for _ in 0..*times {
                s.insert(&(i as i32 + 1));
            }
        }
        let got: Vec<i32> = s.top_k(3, &[1, 2, 3, 4]).into_iter().map(|p| p.0).collect();
        assert_eq!(got, want, "top k tracks a sketch that keeps counting");
    }
}

// ---- 0d-02: HyperLogLog: registers ---------------------------------------------------------------------------------------------------------

#[test]
fn s0d_02_the_register_is_the_top_bits_of_the_hash() {
    let h: HyperLogLog<i64> = HyperLogLog::new(3);
    assert_eq!(h.register_index(0), 0, "the register is the top bits of the hash");
    assert_eq!(h.register_index(u64::MAX), 7, "the register is the top bits of the hash");
    assert_eq!(h.register_index(0b101 << 61), 5, "the register is the top bits of the hash");
    assert_eq!(h.register_index(0b101 << 61 | 12345), 5, "the register is the top bits of the hash");
    let one: HyperLogLog<i64> = HyperLogLog::new(0);
    assert_eq!(one.register_index(u64::MAX), 0, "the register is the top bits of the hash");
}

#[test]
fn s0d_02_the_run_length_is_the_position_of_the_leftmost_one_after_the_register_bits() {
    let h: HyperLogLog<i64> = HyperLogLog::new(3);
    // 3 register bits, then 61 bits
    assert_eq!(h.position_of_leftmost_one(0b000_1 << 60), 1, "the run length is the position of the leftmost one after the register bits");
    assert_eq!(h.position_of_leftmost_one(0b111_01 << 59), 2, "the run length is the position of the leftmost one after the register bits");
    assert_eq!(h.position_of_leftmost_one(0b010_0001 << 57), 4, "the run length is the position of the leftmost one after the register bits");
    assert_eq!(h.position_of_leftmost_one(0b101 << 61), 62, "all zeros after the register bits: 64 - 3 + 1");
    assert_eq!(h.position_of_leftmost_one(1), 61, "the last bit");
}

#[test]
fn s0d_02_with_no_register_bits_all_64_bits_count() {
    let h: HyperLogLog<i64> = HyperLogLog::new(0);
    assert_eq!(h.position_of_leftmost_one(1 << 63), 1, "with no register bits all 64 bits count");
    assert_eq!(h.position_of_leftmost_one(1), 64, "with no register bits all 64 bits count");
    assert_eq!(h.position_of_leftmost_one(0), 65, "with no register bits all 64 bits count");
}

#[test]
fn s0d_02_adding_a_value_raises_exactly_one_register_to_its_rank() {
    let h: HyperLogLog<i64> = HyperLogLog::new(4);
    assert!(h.registers().iter().all(|r| *r == 0), "adding a value raises exactly one register to its rank: expected `h.registers().iter().all(|r| *r == 0)`");
    h.add_elem(&42);
    let regs = h.registers();
    assert_eq!(regs.iter().filter(|r| **r > 0).count(), 1, "adding a value raises exactly one register to its rank");
    assert!(*regs.iter().max().unwrap() >= 1, "adding a value raises exactly one register to its rank: expected `*regs.iter().max().unwrap() >= 1`");
}

#[test]
fn s0d_02_adding_the_same_value_twice_changes_nothing() {
    let h: HyperLogLog<i64> = HyperLogLog::new(4);
    h.add_elem(&42);
    let once = h.registers();
    for _ in 0..100 {
        h.add_elem(&42);
    }
    assert_eq!(h.registers(), once, "adding the same value twice changes nothing");
}

#[test]
fn s0d_02_a_register_only_ever_grows() {
    let h: HyperLogLog<i64> = HyperLogLog::new(2);
    let mut before = h.registers();
    for i in 0..500 {
        h.add_elem(&i);
        let after = h.registers();
        assert!(before.iter().zip(&after).all(|(b, a)| a >= b), "a register only ever grows: expected `before.iter().zip(&after).all(|(b, a)| a >= b)`");
        before = after;
    }
    assert!(before.iter().all(|r| *r > 0), "500 values reach all 4 registers");
}

#[test]
fn s0d_02_a_negative_size_has_no_registers_and_ignores_everything() {
    let none: HyperLogLog<i64> = HyperLogLog::new(-2);
    none.add_elem(&1);
    assert!(none.registers().is_empty(), "a negative size has no registers and ignores everything: expected `none.registers().is_empty()`");
}

#[test]
fn s0d_02_adding_is_thread_safe() {
    let h: Arc<HyperLogLog<i64>> = Arc::new(HyperLogLog::new(3));
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let h = h.clone();
            std::thread::spawn(move || (0..2000).for_each(|i| h.add_elem(&(t * 2000 + i))))
        })
        .collect();
    handles.into_iter().for_each(|t| t.join().unwrap());
    let sequential: HyperLogLog<i64> = HyperLogLog::new(3);
    (0..16000).for_each(|i| sequential.add_elem(&i));
    assert_eq!(h.registers(), sequential.registers(), "taking the maximum is order independent, so threads lose nothing");
}

// ---- 0d-02: HyperLogLog: the estimate --------------------------------------------------------------------------------------------------

#[test]
fn s0d_02_an_empty_sketch_estimates_the_formula_on_all_zero_registers() {
    let h: HyperLogLog<i64> = HyperLogLog::new(3);
    assert_eq!(h.cardinality(), 0, "nothing computed yet");
    h.compute_cardinality();
    // CONSTANT * m^2 / m = 0.79402 * 8
    assert_eq!(h.cardinality(), 6, "an empty sketch estimates the formula on all zero registers");
}

#[test]
fn s0d_02_zero_register_bits_is_one_register() {
    let h: HyperLogLog<i64> = HyperLogLog::new(0);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 0, "0.79402 rounded down");
    h.add_elem(&1);
    h.compute_cardinality();
    let one = h.cardinality();
    assert!(one >= 1, "zero register bits is one register: expected `one >= 1`");
    h.add_elem(&-1);
    h.compute_cardinality();
    assert!(h.cardinality() >= one, "a second value can only raise the single register");
}

#[test]
fn s0d_02_a_negative_size_has_cardinality_zero() {
    let h: HyperLogLog<i64> = HyperLogLog::new(-2);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 0, "a negative size has cardinality zero");
}

#[test]
fn s0d_02_the_estimate_is_close_for_many_distinct_values() {
    let h: HyperLogLog<i64> = HyperLogLog::new(10); // 1024 registers: about 3% standard error
    for i in 0..100_000i64 {
        h.add_elem(&i);
        h.add_elem(&i); // repeats do not count
    }
    h.compute_cardinality();
    // the course's constant 0.79402 is about 10% above the textbook 0.7213 for this many registers, so the estimate runs high
    let ratio = h.cardinality() as f64 / 100_000.0;
    assert!((0.95..1.3).contains(&ratio), "estimate / truth = {ratio}");
}

#[test]
fn s0d_02_strings_and_a_small_set_give_a_plausible_estimate() {
    let h: HyperLogLog<String> = HyperLogLog::new(8);
    for i in 0..2000 {
        h.add_elem(&format!("user-{i}"));
    }
    h.compute_cardinality();
    let ratio = h.cardinality() as f64 / 2000.0;
    assert!((0.8..1.4).contains(&ratio), "estimate / truth = {ratio}");
}

// ---- 0d-03: HyperLogLog, Presto-style ---------------------------------------------------------------------------------------------------

#[test]
fn s0d_03_a_run_of_18_trailing_zeros_is_split_into_dense_and_overflow_bits() {
    // BusTub's PrestoCase1: two register bits would be 1; 262144 = 2^18 has top bit 0 (register 0) and 18 trailing zeros
    let h: HyperLogLogPresto<i64> = HyperLogLogPresto::new(1);
    assert_eq!(h.cardinality(), 0, "a run of 18 trailing zeros is split into dense and overflow bits");
    h.add_elem(&262144);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 3, "a run of 18 trailing zeros is split into dense and overflow bits");
    assert_eq!(h.dense_bucket()[0], 2, "18 = 0b1_0010: the low four bits");
    assert_eq!(h.overflow_bucket_of(0), 1, "and the bit above them");
}

#[test]
fn s0d_03_zero_has_all_the_remaining_bits_as_trailing_zeros() {
    let h: HyperLogLogPresto<i64> = HyperLogLogPresto::new(1);
    h.add_elem(&262144);
    h.add_elem(&0);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 3, "zero has all the remaining bits as trailing zeros");
    assert_eq!(h.dense_bucket()[0], 15, "63 = 0b11_1111");
    assert_eq!(h.overflow_bucket_of(0), 3, "zero has all the remaining bits as trailing zeros");
}

#[test]
fn s0d_03_a_second_register_and_the_estimate_for_long_runs() {
    let h: HyperLogLogPresto<i64> = HyperLogLogPresto::new(1);
    h.add_elem(&262144);
    h.add_elem(&0);
    h.add_elem(&-9151314442816847872); // top bit 1 (register 1), then bit 56
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 227086569448168320, "a second register and the estimate for long runs");
    assert_eq!(h.dense_bucket()[1], 8, "56 = 0b11_1000");
    assert_eq!(h.overflow_bucket_of(0), 3, "a second register and the estimate for long runs");
    h.add_elem(&-1);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 227086569448168320, "a value with no trailing zeros changes nothing");
    h.add_elem(&i64::MIN);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 14647083729406857216, "a second register and the estimate for long runs");
    assert_eq!(h.dense_bucket()[1], 15, "a second register and the estimate for long runs");
}

#[test]
fn s0d_03_one_register_uses_all_64_bits() {
    let h: HyperLogLogPresto<i64> = HyperLogLogPresto::new(0);
    h.add_elem(&65536);
    assert_eq!(h.dense_bucket()[0], 0, "one register uses all 64 bits");
    assert_eq!(h.overflow_bucket_of(0), 1, "one register uses all 64 bits");
    h.add_elem(&i64::MIN);
    h.compute_cardinality();
    assert_eq!((h.dense_bucket()[0], h.overflow_bucket_of(0)), (15, 3), "one register uses all 64 bits");
    h.add_elem(&0);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 14647083729406857216, "one register uses all 64 bits");
    assert_eq!((h.dense_bucket()[0], h.overflow_bucket_of(0)), (0, 4), "64 = 0b100_0000: dense 0, overflow 4");
}

#[test]
fn s0d_03_threads_adding_in_any_order_leave_the_same_registers_as_one_thread() {
    let h: Arc<HyperLogLogPresto<i64>> = Arc::new(HyperLogLogPresto::new(2));
    let values: Vec<i64> = (0..64).map(|k| 1i64 << k).chain((0..64).map(|k| (1i64 << 62) | (1i64 << k))).collect();
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let (h, values) = (h.clone(), values.clone());
            std::thread::spawn(move || {
                for round in 0..200 {
                    for (i, v) in values.iter().enumerate() {
                        if (i + round + t) % 2 == 0 {
                            h.add_elem(v);
                        }
                    }
                }
            })
        })
        .collect();
    handles.into_iter().for_each(|t| t.join().unwrap());
    for v in &values {
        h.add_elem(v);
    }
    let one: HyperLogLogPresto<i64> = HyperLogLogPresto::new(2);
    for v in &values {
        one.add_elem(v);
    }
    assert_eq!(h.dense_bucket(), one.dense_bucket(), "threads adding in any order leave the same registers as one thread");
    for i in 0..4 {
        assert_eq!(h.overflow_bucket_of(i), one.overflow_bucket_of(i), "threads adding in any order leave the same registers as one thread");
    }
}

#[test]
fn s0d_03_negative_size_and_strings() {
    let h: HyperLogLogPresto<i64> = HyperLogLogPresto::new(-2);
    h.compute_cardinality();
    assert_eq!(h.cardinality(), 0, "negative size and strings");
    let s: HyperLogLogPresto<String> = HyperLogLogPresto::new(8);
    for i in 0..2000 {
        s.add_elem(&format!("user-{i}"));
    }
    s.compute_cardinality();
    // counting trailing zeros instead of the position of the first 1 halves every register's weight: about half of the plain estimate
    let ratio = s.cardinality() as f64 / 2000.0;
    assert!((0.35..0.8).contains(&ratio), "estimate / truth = {ratio}");
}

// ---- 0d-04: the observed-remove set ---------------------------------------------------------------------------------------------------

#[test]
fn s0d_04_add_and_remove_on_one_replica() {
    let mut s: ORSet<i32> = ORSet::new();
    for i in 0..10 {
        assert!(!s.contains(&i), "add and remove on one replica: expected `!s.contains(&i)`");
    }
    for i in 0..10 {
        s.add(&i, i as i64);
        assert!(s.contains(&i), "add and remove on one replica: expected `s.contains(&i)`");
    }
    for i in 0..10 {
        s.remove(&i);
        assert!(!s.contains(&i), "add and remove on one replica: expected `!s.contains(&i)`");
    }
}

#[test]
fn s0d_04_adding_an_element_twice_with_two_ids_needs_one_remove() {
    let mut s: ORSet<String> = ORSet::new();
    s.add(&"a".to_string(), 1);
    s.add(&"a".to_string(), 2);
    s.remove(&"a".to_string());
    assert!(!s.contains(&"a".to_string()), "a remove kills every add it has seen");
}

#[test]
fn s0d_04_adding_back_after_a_remove_works() {
    let mut s: ORSet<i32> = ORSet::new();
    s.add(&1, 0);
    s.remove(&1);
    s.add(&1, 1);
    assert!(s.contains(&1), "the new add has a new id that the remove never saw");
}

#[test]
fn s0d_04_removing_an_absent_element_is_harmless() {
    let mut s: ORSet<i32> = ORSet::new();
    s.remove(&7);
    s.add(&7, 0);
    assert!(s.contains(&7), "a remove of something not yet seen does not block a later add");
}

#[test]
fn s0d_04_other_elements_are_not_affected() {
    let mut s: ORSet<i32> = ORSet::new();
    s.add(&1, 0);
    s.add(&2, 1);
    s.remove(&1);
    assert!(!s.contains(&1) && s.contains(&2), "other elements are not affected: expected `!s.contains(&1) && s.contains(&2)`");
}

// ---- 0d-04: merging, and a network of replicas ---------------------------------------------------------------------------------------------

#[test]
fn s0d_04_merge_brings_in_adds_and_removes() {
    let (mut a, mut b): (ORSet<i32>, ORSet<i32>) = (ORSet::new(), ORSet::new());
    a.add(&1, 0);
    b.add(&2, 1);
    b.remove(&2);
    a.merge(&b);
    assert!(a.contains(&1) && !a.contains(&2), "merge brings in adds and removes: expected `a.contains(&1) && !a.contains(&2)`");
    assert!(!b.contains(&1), "merge brings in adds and removes: expected `!b.contains(&1)`");
    b.merge(&a);
    assert!(b.contains(&1), "merge brings in adds and removes: expected `b.contains(&1)`");
    b.add(&2, 2);
    assert!(b.contains(&2), "merge brings in adds and removes: expected `b.contains(&2)`");
}

#[test]
fn s0d_04_add_wins_over_a_concurrent_remove() {
    let (mut a, mut b): (ORSet<String>, ORSet<String>) = (ORSet::new(), ORSet::new());
    let c = "15-445".to_string();
    a.add(&c, 0);
    a.remove(&c);
    b.add(&c, 1);
    let (copy_a, copy_b) = (a.clone(), b.clone());
    a.merge(&copy_b);
    b.merge(&copy_a);
    assert!(a.contains(&c) && b.contains(&c), "add wins over a concurrent remove: expected `a.contains(&c) && b.contains(&c)`");
}

#[test]
fn s0d_04_merging_twice_changes_nothing() {
    let (mut a, mut b): (ORSet<String>, ORSet<String>) = (ORSet::new(), ORSet::new());
    let s = |x: &str| x.to_string();
    a.add(&s("15-410"), 0);
    a.remove(&s("15-410"));
    b.add(&s("15-410"), 1);
    b.add(&s("15-721"), 2);
    let (copy_a, copy_b) = (a.clone(), b.clone());
    a.merge(&copy_b);
    b.merge(&copy_a);
    for set in [&a, &b] {
        assert!(set.contains(&s("15-410")) && set.contains(&s("15-721")), "merging twice changes nothing: expected `set.contains(&s(\"15-410\")) && set.contains(&s(\"15-721\"))`");
    }
    a.merge(&copy_b);
    b.merge(&copy_a);
    assert_eq!(a.to_string(), b.to_string(), "merging twice changes nothing");
}

#[test]
fn s0d_04_elements_lists_each_element_once_and_to_string_sorts_them() {
    let mut s: ORSet<i32> = ORSet::new();
    s.add(&3, 0);
    s.add(&1, 1);
    s.add(&3, 2);
    s.add(&2, 3);
    s.remove(&2);
    let mut e = s.elements();
    e.sort();
    assert_eq!(e, vec![1, 3], "elements lists each element once and to string sorts them");
    assert_eq!(s.to_string(), "{1, 3}", "elements lists each element once and to string sorts them");
    assert_eq!(ORSet::<i32>::new().to_string(), "{}", "elements lists each element once and to string sorts them");
}

#[test]
fn s0d_04_merge_order_does_not_matter() {
    let mut replicas: Vec<ORSet<i32>> = (0..3).map(|_| ORSet::new()).collect();
    let mut uid = 0;
    for step in 0..30 {
        let r = step % 3;
        let elem = (step * 7 % 5) as i32;
        if step % 4 == 3 {
            replicas[r].remove(&elem);
        } else {
            replicas[r].add(&elem, uid);
            uid += 1;
        }
    }
    let mut left = replicas[0].clone();
    left.merge(&replicas[1]);
    left.merge(&replicas[2]);
    let mut right = replicas[2].clone();
    right.merge(&replicas[1]);
    right.merge(&replicas[0]);
    assert_eq!(left.to_string(), right.to_string(), "merge order does not matter");
}

// ---- 0d-05: BusTub's driver tests -------------------------------------------------------------------------------------------------------

#[test]
fn s0d_05_add_remove_and_sync_across_three_nodes() {
    let mut d: ORSetDriver<i32> = ORSetDriver::new(3);
    for i in 0..10usize {
        d.add(i % 3, &(i as i32));
        assert!(d.contains(i % 3, &(i as i32)), "add remove and sync across three nodes: expected `d.contains(i % 3, &(i as i32))`");
        assert!(!d.contains((i + 1) % 3, &(i as i32)) && !d.contains((i + 2) % 3, &(i as i32)), "add remove and sync across three nodes: expected `!d.contains((i + 1) % 3, &(i as i32)) && !d.contains((i + 2) % 3, &(i as i32))`");
        d.sync();
        assert!((0..3).all(|n| d.contains(n, &(i as i32))), "add remove and sync across three nodes: expected `(0..3).all(|n| d.contains(n, &(i as i32)))`");
    }
    for i in 0..10usize {
        d.remove(i % 3, &(i as i32));
        assert!(!d.contains(i % 3, &(i as i32)), "add remove and sync across three nodes: expected `!d.contains(i % 3, &(i as i32))`");
        assert!(d.contains((i + 1) % 3, &(i as i32)) && d.contains((i + 2) % 3, &(i as i32)), "add remove and sync across three nodes: expected `d.contains((i + 1) % 3, &(i as i32)) && d.contains((i + 2) % 3, &(i as i32))`");
        d.sync();
        assert!((0..3).all(|n| !d.contains(n, &(i as i32))), "add remove and sync across three nodes: expected `(0..3).all(|n| !d.contains(n, &(i as i32)))`");
    }
}

#[test]
fn s0d_05_merge_test_two_nodes_agree_after_sync() {
    let mut d: ORSetDriver<i32> = ORSetDriver::new(2);
    d.add(0, &1);
    d.add(1, &1);
    d.remove(0, &1);
    d.sync();
    assert!(d.contains(0, &1), "merge test two nodes agree after sync: expected `d.contains(0, &1)`");
    assert_eq!(d.contains(0, &1), d.contains(1, &1), "merge test two nodes agree after sync");
    d.add(1, &2);
    d.sync();
    d.remove(0, &2);
    d.add(0, &1);
    d.sync();
    d.remove(1, &1);
    d.sync();
    assert_eq!(d.contains(0, &1), d.contains(1, &1), "merge test two nodes agree after sync");
    assert_eq!(d.contains(0, &2), d.contains(1, &2), "merge test two nodes agree after sync");
}

#[test]
fn s0d_05_removing_everything_and_adding_it_back() {
    let mut d: ORSetDriver<i32> = ORSetDriver::new(3);
    for i in 0..10usize {
        d.add(i % 3, &(i as i32));
    }
    d.sync();
    for i in 0..10usize {
        assert!((0..3).all(|n| d.contains(n, &(i as i32))), "removing everything and adding it back: expected `(0..3).all(|n| d.contains(n, &(i as i32)))`");
        d.remove(i % 3, &(i as i32));
    }
    d.sync();
    for i in 0..10 {
        assert!((0..3).all(|n| !d.contains(n, &i)), "removing everything and adding it back: expected `(0..3).all(|n| !d.contains(n, &i))`");
    }
    for i in 0..10usize {
        d.add(i % 3, &(i as i32));
    }
    d.sync();
    for i in 0..10 {
        assert!((0..3).all(|n| d.contains(n, &i)), "removing everything and adding it back: expected `(0..3).all(|n| d.contains(n, &i))`");
    }
}

#[test]
fn s0d_05_adds_win_a_lot() {
    let mut d: ORSetDriver<i32> = ORSetDriver::new(3);
    for i in 0..10 {
        for n in 0..3 {
            d.add(n, &i);
        }
    }
    for i in 0..10usize {
        d.remove(i % 3, &(i as i32));
        assert!(!d.contains(i % 3, &(i as i32)), "adds win a lot: expected `!d.contains(i % 3, &(i as i32))`");
    }
    d.sync();
    for i in 0..10 {
        assert!((0..3).all(|n| d.contains(n, &i)), "the other replicas' adds survive the remove");
    }
}

#[test]
fn s0d_05_a_lost_network_still_converges_later() {
    let mut d: ORSetDriver<i32> = ORSetDriver::new(3);
    for i in 0..10 {
        d.add((i % 2) as usize, &i);
    }
    for i in 0..20 {
        d.add(2, &i);
    }
    d.save(0);
    d.save(1);
    d.load(0);
    d.load(1);
    for i in 0..10 {
        assert!(d.contains(0, &i) && d.contains(1, &i) && d.contains(2, &i), "a lost network still converges later: expected `d.contains(0, &i) && d.contains(1, &i) && d.contains(2, &i)`");
    }
    for i in 10..20 {
        assert!(!d.contains(0, &i) && !d.contains(1, &i) && d.contains(2, &i), "a lost network still converges later: expected `!d.contains(0, &i) && !d.contains(1, &i) && d.contains(2, &i)`");
        d.remove(0, &i);
    }
    for i in 10..20 {
        d.add(0, &i);
        d.remove(2, &i);
    }
    d.sync();
    for i in 10..20 {
        assert!((0..3).all(|n| d.contains(n, &i)), "node 0's fresh add beats node 2's remove of the old one");
    }
}

// ---- properties ----------------------------------------------------------------------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 64, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

proptest! {
    #![proptest_config(pconfig())]

    /// A count-min sketch of any size **never counts an item lower than it occurred**, however many items collide; with a width that gives
    /// every distinct item a column of its own in some row it is exact; and counts never decrease as the stream grows.
    #[test]
    fn s0d_01_the_estimate_is_never_below_the_truth(width in 1u32..40, depth in 1u32..5, stream in prop::collection::vec(0u16..60, 0..150)) {
        let s = cms::<u16>(width, depth);
        let mut seen: BTreeMap<u16, u32> = BTreeMap::new();
        let mut last: BTreeMap<u16, u32> = BTreeMap::new();
        for x in &stream {
            s.insert(x);
            *seen.entry(*x).or_insert(0) += 1;
            for (item, truth) in &seen {
                let est = s.count(item);
                prop_assert!(est >= *truth, "{} counted {} but occurred {} times", item, est, truth);
                prop_assert!(est >= last.get(item).copied().unwrap_or(0), "the count of {} went down", item);
                last.insert(*item, est);
            }
        }
        let total = stream.len() as u32;
        for item in 0..60u16 {
            prop_assert!(s.count(&item) <= total, "no item can be counted more often than there were insertions");
        }
    }

    /// Merging the sketch of one stream into the sketch of another counts exactly like the sketch of the two streams together, for every
    /// item; merging is commutative; and `clear` gives back the empty sketch.
    #[test]
    fn s0d_01_merge_equals_the_sketch_of_both_streams(width in 1u32..30, depth in 1u32..4, a in prop::collection::vec(0u16..50, 0..80), b in prop::collection::vec(0u16..50, 0..80)) {
        let (sa, sb, both, rev) = (cms::<u16>(width, depth), cms::<u16>(width, depth), cms::<u16>(width, depth), cms::<u16>(width, depth));
        for x in &a { sa.insert(x); both.insert(x); }
        for x in &b { sb.insert(x); both.insert(x); }
        sa.merge(&sb).unwrap();
        for x in b.iter().chain(&a) { rev.insert(x); }
        for item in 0..50u16 {
            prop_assert_eq!(sa.count(&item), both.count(&item), "item {}", item);
            prop_assert_eq!(rev.count(&item), both.count(&item), "order of the streams does not matter");
        }
        sa.clear();
        for item in 0..50u16 {
            prop_assert_eq!(sa.count(&item), 0);
        }
    }

    /// `top_k`: at most `k` answers, never more than there are candidates, each with the sketch's own count, sorted by count (highest
    /// first, ties in the order of the candidates), and nobody left out has a higher count than the last one returned.
    #[test]
    fn s0d_01_top_k_returns_the_best_candidates_in_order(stream in prop::collection::vec(0u16..20, 0..100), candidates in prop::collection::btree_set(0u16..25, 0..20).prop_map(|s| s.into_iter().rev().collect::<Vec<_>>()), k in 0u16..25) {
        let s = cms::<u16>(16, 3);
        for x in &stream { s.insert(x); }
        let top = s.top_k(k, &candidates);
        prop_assert_eq!(top.len(), (k as usize).min(candidates.len()));
        for (item, count) in &top {
            prop_assert_eq!(*count, s.count(item));
        }
        prop_assert!(top.windows(2).all(|w| w[0].1 >= w[1].1), "sorted by count, highest first");
        if let Some((_, floor)) = top.last() {
            for c in &candidates {
                let included = top.iter().any(|(i, _)| i == c);
                prop_assert!(included || s.count(c) <= *floor, "{} counted {} was left out below {}", c, s.count(c), floor);
            }
        }
        // ties keep the order of the candidates
        for w in top.windows(2) {
            if w[0].1 == w[1].1 {
                let pos = |x: &u16| candidates.iter().position(|c| c == x).unwrap();
                prop_assert!(pos(&w[0].0) <= pos(&w[1].0));
            }
        }
    }

    /// HyperLogLog does not depend on the order of the stream or on repeats (the registers are the same), registers only ever grow,
    /// and the estimate of a stream is the estimate of its distinct values.
    #[test]
    fn s0d_02_registers_depend_only_on_the_set_of_values(values in prop::collection::vec(any::<i64>(), 0..200), bits in 0i16..9, shuffle in any::<u64>()) {
        let once: HyperLogLog<i64> = HyperLogLog::new(bits);
        let mut last = once.registers();
        for v in &values {
            once.add_elem(v);
            let now = once.registers();
            prop_assert!(now.iter().zip(&last).all(|(n, l)| n >= l), "a register went down");
            last = now;
        }
        let mut again: Vec<i64> = values.iter().copied().chain(values.iter().copied()).collect();
        let mut x = shuffle | 1;
        let mut tagged: Vec<(u64, i64)> = again.iter().map(|v| { x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (x >> 20, *v) }).collect();
        tagged.sort();
        again = tagged.into_iter().map(|t| t.1).collect();
        let other: HyperLogLog<i64> = HyperLogLog::new(bits);
        for v in &again { other.add_elem(v); }
        prop_assert_eq!(other.registers(), once.registers());
        once.compute_cardinality();
        other.compute_cardinality();
        prop_assert_eq!(once.cardinality(), other.cardinality());
        prop_assert!(once.registers().iter().all(|r| *r as u32 <= 64 - bits as u32 + 1));
    }

    /// The Presto-style sketch keeps the same information as one register per value: whatever the order, the registers rebuilt from the
    /// dense and the overflow bits are the longest run of trailing zeros of the values that fall in them, for hashes we choose.
    #[test]
    fn s0d_03_dense_and_overflow_bits_hold_the_longest_run(values in prop::collection::vec(any::<u64>(), 0..120), bits in 0i16..6) {
        let h: HyperLogLogPresto<i64> = HyperLogLogPresto::new(bits);
        let b = bits as u32;
        let mut model = vec![0u32; 1usize << bits];
        for v in &values {
            h.add_elem(&(*v as i64));
            let idx = if b == 0 { 0 } else { (*v >> (64 - b)) as usize };
            let rest = 64 - b;
            let tz = if rest == 64 { v.trailing_zeros() } else { (v & ((1u64 << rest) - 1)).trailing_zeros().min(rest) };
            model[idx] = model[idx].max(tz);
        }
        let dense = h.dense_bucket();
        for (i, want) in model.iter().enumerate() {
            let got = dense[i] as u32 | ((h.overflow_bucket_of(i as u16) as u32) << 4);
            prop_assert_eq!(got, *want, "register {}", i);
            prop_assert!(dense[i] < 16, "the dense part is 4 bits");
            prop_assert!(h.overflow_bucket_of(i as u16) < 8, "the overflow part is 3 bits");
        }
    }
}

#[test]
fn s0d_02_the_estimate_tracks_the_number_of_distinct_values_across_sizes() {
    // the course's constant is about 10% above the textbook one for these sizes: the ratio runs high but stays in a band
    for (n, bits) in [(3_000i64, 8i16), (10_000, 8), (30_000, 10), (60_000, 12)] {
        let h: HyperLogLog<i64> = HyperLogLog::new(bits);
        for i in 0..n {
            h.add_elem(&(i * 7919 + 13));
        }
        h.compute_cardinality();
        let ratio = h.cardinality() as f64 / n as f64;
        assert!((0.8..1.4).contains(&ratio), "{n} distinct values with {bits} bits: estimate / truth = {ratio}");
    }
}

#[derive(Clone, Debug)]
enum SetOp {
    Add(u8),
    Remove(u8),
}

fn replica_from(ops: &[SetOp], uids: &mut i64) -> ORSet<u8> {
    let mut r = ORSet::new();
    for op in ops {
        match op {
            SetOp::Add(e) => { r.add(e, *uids); *uids += 1; }
            SetOp::Remove(e) => r.remove(e),
        }
    }
    r
}

fn state(r: &ORSet<u8>) -> BTreeSet<u8> {
    (0..8u8).filter(|e| r.contains(e)).collect()
}

fn ops_strategy() -> impl Strategy<Value = Vec<SetOp>> {
    prop::collection::vec(prop_oneof![3 => (0..8u8).prop_map(SetOp::Add), 2 => (0..8u8).prop_map(SetOp::Remove)], 0..25)
}

proptest! {
    #![proptest_config(pconfig())]

    /// The laws of a CRDT: merge is **commutative**, **associative** and **idempotent**, for replicas that did arbitrary adds and removes
    /// independently (so any order of merging, any number of times, gives one set).
    #[test]
    fn s0d_04_merge_is_commutative_associative_and_idempotent(a in ops_strategy(), b in ops_strategy(), c in ops_strategy()) {
        let mut uid = 0;
        let (ra, rb, rc) = (replica_from(&a, &mut uid), replica_from(&b, &mut uid), replica_from(&c, &mut uid));
        let merged = |x: &ORSet<u8>, y: &ORSet<u8>| { let mut m = x.clone(); m.merge(y); m };
        prop_assert_eq!(state(&merged(&ra, &rb)), state(&merged(&rb, &ra)), "commutative");
        prop_assert_eq!(state(&merged(&merged(&ra, &rb), &rc)), state(&merged(&ra, &merged(&rb, &rc))), "associative");
        prop_assert_eq!(state(&merged(&ra, &ra)), state(&ra), "idempotent");
        let once = merged(&ra, &rb);
        prop_assert_eq!(state(&merged(&once, &rb)), state(&once), "merging the same replica again changes nothing");
        // the listing agrees with `contains`, and lists each element once
        let listed = once.elements();
        prop_assert_eq!(listed.iter().copied().collect::<BTreeSet<_>>(), state(&once));
        prop_assert_eq!(listed.len(), state(&once).len(), "each element once");
    }

    /// **Add wins**: an element added at one replica and removed at another that never saw the add is in the set after they merge; a remove
    /// that did see the add removes it, and adding it again (with a new id) brings it back.
    #[test]
    fn s0d_04_a_remove_only_removes_the_adds_it_has_seen(e in 0..8u8) {
        let (mut a, mut b) = (ORSet::new(), ORSet::new());
        a.add(&e, 1);
        b.remove(&e); // b has never seen the add
        a.merge(&b);
        prop_assert!(a.contains(&e), "the concurrent remove did not see the add");
        let mut c = a.clone();
        c.remove(&e); // c has seen it
        prop_assert!(!c.contains(&e));
        c.add(&e, 2);
        prop_assert!(c.contains(&e), "a new add brings it back");
        a.merge(&c);
        prop_assert!(a.contains(&e));
    }

    /// A network of replicas with random adds, removes, saves, loads and lost messages: after one final full sync (twice, so that nothing
    /// is in flight) **every replica holds the same set**, and it contains every element that was added after the last remove of it on
    /// the replica where it was removed... checked here as agreement plus: an element never removed anywhere is present.
    #[test]
    fn s0d_05_replicas_converge_after_a_sync_whatever_the_network_did(steps in prop::collection::vec((0usize..3, 0u8..4, 0u8..8), 1..60)) {
        let mut net: ORSetDriver<u8> = ORSetDriver::new(3);
        let mut ever_removed: BTreeSet<u8> = BTreeSet::new();
        let mut ever_added: BTreeSet<u8> = BTreeSet::new();
        for (node, what, elem) in steps {
            match what {
                0 => { net.add(node, &elem); ever_added.insert(elem); }
                1 => { net.remove(node, &elem); ever_removed.insert(elem); }
                2 => net.save(node),
                _ => net.load(node),
            }
        }
        net.sync();
        net.sync();
        let states: Vec<BTreeSet<u8>> = (0..3).map(|n| (0..8u8).filter(|e| net.contains(n, e)).collect()).collect();
        prop_assert!(states.windows(2).all(|w| w[0] == w[1]), "the replicas disagree: {:?}", states);
        for e in ever_added.difference(&ever_removed) {
            prop_assert!(states[0].contains(e), "{} was added and never removed anywhere", e);
        }
    }
}

// @@ challenge 0d-c1 begin
mod ch_0d_c1 {
    use proptest::prelude::*;

    use bustub::primer::bloom::BloomFilter;

    #[test]
    fn s0d_c1_there_are_no_false_negatives() {
        let mut f = BloomFilter::new(2000, 5);
        for k in 0..200u64 {
            f.insert(k * 7919);
        }
        assert!((0..200u64).all(|k| f.contains(k * 7919)), "this check: expected `(0..200u64).all(|k| f.contains(k * 7919))`");
    }

    #[test]
    fn s0d_c1_an_empty_filter_contains_nothing_and_inserting_twice_changes_nothing() {
        let mut f = BloomFilter::new(512, 3);
        assert!(!f.contains(1) && f.bits_set() == 0, "this check: expected `!f.contains(1) && f.bits_set() == 0`");
        f.insert(1);
        let once = f.bits_set();
        f.insert(1);
        assert_eq!(f.bits_set(), once, "this check");
        assert!(once <= 3, "this check: expected `once <= 3`");
    }

    #[test]
    fn s0d_c1_the_false_positive_rate_matches_the_theory() {
        // 10 bits per key and 7 hashes: about 0.8% in theory; accept up to 3%
        let mut f = BloomFilter::new(10_000, 7);
        for k in 0..1000u64 {
            f.insert(k);
        }
        let fp = (1_000_000u64..1_005_000).filter(|&k| f.contains(k)).count();
        assert!(fp < 150, "{fp} false positives in 5000 probes");
    }

    #[test]
    fn s0d_c1_union_contains_everything_either_contained_and_checks_the_shape() {
        let (mut a, mut b) = (BloomFilter::new(1024, 4), BloomFilter::new(1024, 4));
        for k in 0..50u64 { a.insert(k); }
        for k in 100..150u64 { b.insert(k); }
        let u = a.union(&b).unwrap();
        assert!((0..50u64).chain(100..150).all(|k| u.contains(k)), "this check: expected `(0..50u64).chain(100..150).all(|k| u.contains(k))`");
        assert_eq!(u.bits_set() , {
            let mut both = BloomFilter::new(1024, 4);
            for k in (0..50u64).chain(100..150) { both.insert(k); }
            both.bits_set()
        }, "this check");
        assert!(a.union(&BloomFilter::new(512, 4)).is_none(), "this check: expected `a.union(&BloomFilter::new(512, 4)).is_none()`");
        assert!(a.union(&BloomFilter::new(1024, 5)).is_none(), "this check: expected `a.union(&BloomFilter::new(1024, 5)).is_none()`");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: whatever is inserted is contained, and the number of set bits is at most `k` per key.
        #[test]
        fn s0d_c1_property_inserted_keys_are_always_found(keys in proptest::collection::vec(any::<u64>(), 0..80), m in 64usize..4000, k in 1u32..8) {
            let mut f = BloomFilter::new(m, k);
            for &x in &keys { f.insert(x); }
            for &x in &keys { prop_assert!(f.contains(x)); }
            prop_assert!(f.bits_set() <= (k as usize * keys.len()).min(m));
        }
    }
}
// @@ challenge 0d-c1 end

// @@ challenge 0d-c2 begin
mod ch_0d_c2 {
    use proptest::prelude::*;

    use bustub::primer::reservoir::Reservoir;

    #[test]
    fn s0d_c2_a_short_stream_is_kept_whole() {
        let mut r = Reservoir::new(5, 1);
        for i in 0..3 {
            r.offer(i);
        }
        let mut s = r.sample().to_vec();
        s.sort();
        assert_eq!((s, r.seen()), (vec![0, 1, 2], 3), "this check");
    }

    #[test]
    fn s0d_c2_the_sample_never_exceeds_k() {
        let mut r = Reservoir::new(4, 9);
        for i in 0..1000 {
            r.offer(i);
            assert!(r.sample().len() <= 4, "this check: expected `r.sample().len() <= 4`");
        }
        assert_eq!((r.sample().len(), r.seen()), (4, 1000), "this check");
    }

    #[test]
    fn s0d_c2_a_reservoir_of_zero_holds_nothing() {
        let mut r = Reservoir::new(0, 3);
        for i in 0..10 {
            r.offer(i);
        }
        assert_eq!((r.sample().len(), r.seen()), (0, 10), "this check");
    }

    #[test]
    fn s0d_c2_the_same_seed_gives_the_same_sample() {
        let run = |seed| {
            let mut r = Reservoir::new(5, seed);
            for i in 0..500 {
                r.offer(i);
            }
            r.sample().to_vec()
        };
        assert_eq!(run(7), run(7), "this check");
        assert_ne!(run(7), run(8), "this check");
    }

    #[test]
    fn s0d_c2_every_item_is_equally_likely_to_be_sampled() {
        // 10 items, k = 3: each should be sampled in about 30% of 20 000 runs
        let n = 10usize;
        let runs = 20_000;
        let mut hits = vec![0usize; n];
        for seed in 0..runs {
            let mut r = Reservoir::new(3, seed as u64 + 1);
            for i in 0..n {
                r.offer(i);
            }
            for &i in r.sample() {
                hits[i] += 1;
            }
        }
        for (i, &h) in hits.iter().enumerate() {
            let p = h as f64 / runs as f64;
            assert!((p - 0.3).abs() < 0.03, "item {i} was sampled with frequency {p}, expected 0.3 (all: {hits:?})");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the sample is a sub-multiset of the stream with `min(k, n)` items.
        #[test]
        fn s0d_c2_property_the_sample_comes_from_the_stream(stream in proptest::collection::vec(0u32..6, 0..40), k in 0usize..8, seed in any::<u64>()) {
            let mut r = Reservoir::new(k, seed);
            for &x in &stream { r.offer(x); }
            prop_assert_eq!(r.sample().len(), k.min(stream.len()));
            let mut pool = stream.clone();
            for x in r.sample() {
                let at = pool.iter().position(|y| y == x);
                prop_assert!(at.is_some(), "{} sampled more often than offered", x);
                pool.remove(at.unwrap());
            }
        }
    }
}
// @@ challenge 0d-c2 end

// @@ challenge 0d-c3 begin
mod ch_0d_c3 {
    use proptest::prelude::*;

    use bustub::primer::merkle::{leaf_hash, node_hash, verify, MerkleTree};

    fn leaves(n: usize) -> Vec<Vec<u8>> {
        (0..n).map(|i| format!("leaf-{i}").into_bytes()).collect()
    }

    #[test]
    fn s0d_c3_roots_of_small_trees_by_hand() {
        assert_eq!(MerkleTree::new(&[]).root(), 0, "this check");
        let one = leaves(1);
        assert_eq!(MerkleTree::new(&one).root(), leaf_hash(&one[0]), "this check");
        let two = leaves(2);
        assert_eq!(MerkleTree::new(&two).root(), node_hash(leaf_hash(&two[0]), leaf_hash(&two[1])), "this check");
        let three = leaves(3);
        let (a, b, c) = (leaf_hash(&three[0]), leaf_hash(&three[1]), leaf_hash(&three[2]));
        assert_eq!(MerkleTree::new(&three).root(), node_hash(node_hash(a, b), node_hash(c, c)), "an odd node is paired with itself");
    }

    #[test]
    fn s0d_c3_every_leaf_has_a_proof_that_verifies() {
        for n in 1..=9 {
            let ls = leaves(n);
            let t = MerkleTree::new(&ls);
            for (i, l) in ls.iter().enumerate() {
                let p = t.proof(i).unwrap();
                assert!(verify(t.root(), l, &p), "n {n}, leaf {i}");
                assert_eq!(p.len(), (n as f64).log2().ceil() as usize, "this check");
            }
            assert!(t.proof(n).is_none(), "this check: expected `t.proof(n).is_none()`");
        }
    }

    #[test]
    fn s0d_c3_a_wrong_leaf_or_a_tampered_proof_fails() {
        let ls = leaves(6);
        let t = MerkleTree::new(&ls);
        let p = t.proof(2).unwrap();
        assert!(!verify(t.root(), b"leaf-3", &p), "a proof is for one leaf");
        let mut bad = p.clone();
        bad[0].0 ^= 1;
        assert!(!verify(t.root(), &ls[2], &bad), "this check: expected `!verify(t.root(), &ls[2], &bad)`");
        let mut flipped = p.clone();
        flipped[1].1 = !flipped[1].1;
        assert!(!verify(t.root(), &ls[2], &flipped), "this check: expected `!verify(t.root(), &ls[2], &flipped)`");
    }

    #[test]
    fn s0d_c3_changing_any_leaf_or_the_order_changes_the_root() {
        let ls = leaves(5);
        let root = MerkleTree::new(&ls).root();
        for i in 0..5 {
            let mut m = ls.clone();
            m[i].push(b'!');
            assert_ne!(MerkleTree::new(&m).root(), root, "leaf {i}");
        }
        let mut swapped = ls.clone();
        swapped.swap(0, 4);
        assert_ne!(MerkleTree::new(&swapped).root(), root, "this check");
        assert_eq!(MerkleTree::new(&ls).root(), root, "the root is a function of the leaves");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every leaf's proof verifies against the root and against nothing else.
        #[test]
        fn s0d_c3_property_proofs_verify_exactly_their_leaf(ls in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..6), 1..12)) {
            let t = MerkleTree::new(&ls);
            for (i, l) in ls.iter().enumerate() {
                let p = t.proof(i).unwrap();
                prop_assert!(verify(t.root(), l, &p));
                let mut other = l.clone();
                other.push(0);
                prop_assert!(!verify(t.root(), &other, &p));
            }
        }
    }
}
// @@ challenge 0d-c3 end

// @@ challenge 0d-c4 begin
mod ch_0d_c4 {
    use proptest::prelude::*;

    use bustub::primer::pn_counter::PnCounter;

    fn counter(ops: &[(u32, bool, u64)]) -> PnCounter {
        let mut c = PnCounter::new();
        for &(n, up, v) in ops {
            if up { c.inc(n, v) } else { c.dec(n, v) }
        }
        c
    }

    #[test]
    fn s0d_c4_values_add_and_subtract() {
        let c = counter(&[(1, true, 5), (1, false, 2), (2, true, 10)]);
        assert_eq!(c.value(), 13, "this check");
        assert_eq!(PnCounter::new().value(), 0, "this check");
        assert_eq!(counter(&[(1, false, 4)]).value(), -4, "this check");
    }

    #[test]
    fn s0d_c4_merging_two_replicas_combines_their_updates() {
        let mut a = counter(&[(1, true, 5)]);
        let b = counter(&[(2, true, 3), (2, false, 1)]);
        a.merge(&b);
        assert_eq!(a.value(), 7, "this check");
        a.merge(&b);
        assert_eq!(a.value(), 7, "merging again changes nothing");
    }

    #[test]
    fn s0d_c4_two_replicas_that_each_saw_a_newer_state_of_the_same_node_keep_the_larger() {
        let old = counter(&[(1, true, 3)]);
        let new = counter(&[(1, true, 8)]);
        let mut m = old.clone();
        m.merge(&new);
        assert_eq!(m.value(), 8, "not 11: the counts of one node are not added together");
    }

    #[test]
    fn s0d_c4_updates_made_after_a_merge_are_not_lost_by_the_next_one() {
        let mut a = counter(&[(1, true, 2)]);
        let mut b = a.clone();
        b.inc(2, 4);
        a.inc(1, 1);
        a.merge(&b);
        b.merge(&a);
        assert_eq!((a.value(), b.value()), (7, 7), "this check");
        assert_eq!(a, b, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: merge is commutative, associative and idempotent, and replicas that exchange everything agree.
        #[test]
        fn s0d_c4_property_the_three_laws(a in proptest::collection::vec((0u32..3, any::<bool>(), 0u64..9), 0..8), b in proptest::collection::vec((0u32..3, any::<bool>(), 0u64..9), 0..8), c in proptest::collection::vec((0u32..3, any::<bool>(), 0u64..9), 0..8)) {
            let (a, b, c) = (counter(&a), counter(&b), counter(&c));
            let merged = |x: &PnCounter, y: &PnCounter| { let mut m = x.clone(); m.merge(y); m };
            prop_assert_eq!(merged(&a, &b), merged(&b, &a));
            prop_assert_eq!(merged(&merged(&a, &b), &c), merged(&a, &merged(&b, &c)));
            prop_assert_eq!(merged(&a, &a), a.clone());
            let all = merged(&merged(&a, &b), &c);
            prop_assert_eq!(merged(&all, &a), all);
        }
    }
}
// @@ challenge 0d-c4 end

// @@ challenge 0d-c5 begin
mod ch_0d_c5 {
    use proptest::prelude::*;

    use bustub::primer::hll_registers::{Registers, SizeMismatch};

    fn sketch(hashes: &[u64]) -> Registers {
        let mut r = Registers::new(4);
        for &h in hashes {
            r.update(h);
        }
        r
    }

    fn mix(i: u64) -> u64 {
        let mut x = i.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 29;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^ (x >> 32)
    }

    #[test]
    fn s0d_c5_merging_takes_the_larger_register_not_the_sum() {
        let a = sketch(&[mix(1), mix(2), mix(3)]);
        let mut b = a.clone();
        b.merge(&a).unwrap();
        assert_eq!(b, a, "merging a sketch with itself changes nothing");
    }

    #[test]
    fn s0d_c5_registers_of_two_sketches_combine_by_maximum() {
        let a = sketch(&(0..20).map(mix).collect::<Vec<_>>());
        let b = sketch(&(10..40).map(mix).collect::<Vec<_>>());
        let mut m = a.clone();
        m.merge(&b).unwrap();
        for i in 0..16 {
            assert_eq!(m.registers()[i], a.registers()[i].max(b.registers()[i]), "this check");
        }
    }

    #[test]
    fn s0d_c5_the_sketch_of_a_stream_is_the_merge_of_the_sketches_of_its_parts() {
        let all: Vec<u64> = (0..200).map(mix).collect();
        let whole = sketch(&all);
        let mut parts = sketch(&all[..70]);
        parts.merge(&sketch(&all[70..])).unwrap();
        assert_eq!(parts, whole, "this check");
        let mut again = parts.clone();
        again.merge(&sketch(&all[..70])).unwrap();
        assert_eq!(again, whole, "merging in a part that was already included changes nothing");
    }

    #[test]
    fn s0d_c5_sketches_of_different_sizes_do_not_merge() {
        let mut a = Registers::new(4);
        assert_eq!(a.merge(&Registers::new(5)), Err(SizeMismatch), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the three laws, and the split-stream equality, for arbitrary hashes.
        #[test]
        fn s0d_c5_property_merge_is_a_semilattice(a in proptest::collection::vec(any::<u64>(), 0..30), b in proptest::collection::vec(any::<u64>(), 0..30), c in proptest::collection::vec(any::<u64>(), 0..30)) {
            let (sa, sb, sc) = (sketch(&a), sketch(&b), sketch(&c));
            let m = |x: &Registers, y: &Registers| { let mut r = x.clone(); r.merge(y).unwrap(); r };
            prop_assert_eq!(m(&sa, &sb), m(&sb, &sa));
            prop_assert_eq!(m(&m(&sa, &sb), &sc), m(&sa, &m(&sb, &sc)));
            prop_assert_eq!(m(&sa, &sa), sa.clone());
            let mut joined = a.clone();
            joined.extend(&b);
            prop_assert_eq!(m(&sa, &sb), sketch(&joined));
        }
    }
}
// @@ challenge 0d-c5 end
