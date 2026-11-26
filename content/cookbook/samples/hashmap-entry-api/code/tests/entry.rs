//! Behaviour claims made on the page.

use hashmap_entry_api::*;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

#[test]
fn counting_matches_a_simple_oracle() {
    let words = ["a", "b", "c", "d", "e"];
    let mut state = 0x2545_F491_4F6C_DD1D_u64; // seeded, so a failure repeats
    for _ in 0..200 {
        let mut text = Vec::new();
        for _ in 0..(state % 40) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            text.push(words[(state % 5) as usize]);
        }
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let joined = text.join(" ");
        let counts = count_words(&joined);
        for w in words {
            let expected = text.iter().filter(|t| **t == w).count();
            assert_eq!(
                counts.get(w).copied().unwrap_or(0),
                expected,
                "{w:?} in {joined:?}"
            );
        }
        assert!(counts.values().all(|&n| n > 0));
    }
}

#[test]
fn grouping_keeps_input_order_inside_a_group() {
    let g = group_by_len(&["dd", "a", "bb", "c"]);
    assert_eq!(g[&2], ["dd", "bb"]);
    assert_eq!(g[&1], ["a", "c"]);
}

#[test]
fn and_modify_does_not_run_for_the_value_or_insert_adds() {
    let mut m = HashMap::new();
    m.entry("k").and_modify(|n| *n += 100).or_insert(1);
    assert_eq!(m["k"], 1);
    m.entry("k").and_modify(|n| *n += 100).or_insert(1);
    assert_eq!(m["k"], 101);

    let mut best = HashMap::new();
    for s in [5, 9, 2] {
        record_best(&mut best, "a", s);
    }
    assert_eq!(best["a"], 9);
}

#[test]
fn take_one_never_stores_zero() {
    let mut stock = HashMap::from([("bolt".to_string(), 2)]);
    assert_eq!(take_one(&mut stock, "bolt".into()), Some(1));
    assert_eq!(take_one(&mut stock, "bolt".into()), Some(0));
    assert!(!stock.contains_key("bolt"));
    assert_eq!(take_one(&mut stock, "bolt".into()), None);
    assert!(stock.is_empty(), "a vacant entry inserts nothing");
}

#[test]
fn or_insert_evaluates_its_argument_on_every_call() {
    let mut m = HashMap::new();
    let (mut eager, mut lazy) = (0, 0);
    for _ in 0..3 {
        m.entry("a").or_insert({
            eager += 1;
            0
        });
        m.entry("b").or_insert_with(|| {
            lazy += 1;
            0
        });
    }
    assert_eq!((eager, lazy), (3, 1));
}

#[test]
fn the_other_tasks() {
    let mut cache = HashMap::new();
    assert_eq!(remember_len(&mut cache, "héllo".into()), 5);

    let mut events = HashMap::new();
    tally(&mut events, "ann", "login");
    tally(&mut events, "ann", "login");
    tally(&mut events, "bob", "login");
    assert_eq!(events["ann"]["login"], 2);
    assert_eq!(events["bob"]["login"], 1);

    let (mut a, mut b) = (HashMap::new(), HashMap::new());
    for w in ["x", "y", "x"] {
        bump(&mut a, w);
        bump_with_entry(&mut b, w);
    }
    assert_eq!(a, b);
    assert_eq!(a["x"], 2);
    *counter(&mut a, "z") += 7;
    *counter(&mut a, "z") += 1;
    assert_eq!(a["z"], 8);

    let mut numbers = HashMap::new();
    assert_eq!(parsed(&mut numbers, "42".into()).map(|v| *v), Ok(42));
    assert!(parsed(&mut numbers, "forty".into()).is_err());
    assert_eq!(numbers.len(), 1, "a failed parse inserts nothing");

    let mut memo = HashMap::new();
    assert_eq!(collatz_len(6, &mut memo), 9); // 6 3 10 5 16 8 4 2 1
    assert_eq!(memo[&3], 8);
}

#[test]
fn a_panic_in_the_closure_inserts_nothing() {
    let mut m: HashMap<u32, u32> = HashMap::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        m.entry(1).or_insert_with(|| panic!("boom"));
    }));
    assert!(r.is_err());
    assert!(m.is_empty());
}

/// A key whose equality ignores ASCII case, to make "which key is kept" visible.
#[derive(Debug)]
struct NoCase(&'static str);
impl PartialEq for NoCase {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(other.0)
    }
}
impl Eq for NoCase {}
impl Hash for NoCase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

#[test]
fn a_hit_keeps_the_key_already_in_the_map() {
    let mut m = HashMap::from([(NoCase("Apple"), 1)]);
    assert_eq!(m.entry(NoCase("APPLE")).key().0, "Apple");
    m.entry(NoCase("APPLE")).insert_entry(2);
    let (k, v) = m.iter().next().unwrap();
    assert_eq!((k.0, *v, m.len()), ("Apple", 2, 1));
}

fn filled_to_capacity() -> HashMap<u32, u32> {
    let mut m = HashMap::with_capacity(3);
    let mut i = 0;
    while m.len() < m.capacity() {
        m.insert(i, i);
        i += 1;
    }
    m
}

#[test]
fn a_miss_reserves_room_even_if_nothing_is_inserted() {
    let mut hit = filled_to_capacity();
    let before = hit.capacity();
    let _ = hit.entry(0); // key 0 is present
    assert_eq!(hit.capacity(), before);

    let mut miss = filled_to_capacity();
    let (len, before) = (miss.len(), miss.capacity());
    let _ = miss.entry(u32::MAX); // absent, and we insert nothing
    assert_eq!(miss.len(), len);
    assert!(miss.capacity() > before, "{} <= {before}", miss.capacity());
}

/// Every key gets the same hash, so the layout is the same on every run.
#[derive(Default)]
struct SameHash;
impl Hasher for SameHash {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {}
}

#[test]
fn capacity_can_drop_after_removals_and_comes_back_after_clear() {
    let mut m: HashMap<u32, u32, BuildHasherDefault<SameHash>> = HashMap::default();
    m.reserve(100);
    for k in 0..64 {
        m.insert(k, k);
    }
    let full = m.capacity();
    for k in 0..32 {
        m.entry(k).or_default(); // hit: changes nothing
        m.remove(&k);
    }
    assert!(m.capacity() < full, "{} is not below {full}", m.capacity());
    assert!(m.capacity() >= m.len(), "still a valid lower bound");
    m.clear();
    assert!(m.capacity() >= full);
}
