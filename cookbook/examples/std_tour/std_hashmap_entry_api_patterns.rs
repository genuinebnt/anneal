//! RECIPE 1043 (std-collections): HashMap and HashSet in depth, with the Entry API as the centrepiece.
//! LEVEL: intermediate
//! Use in: counting, grouping, caching, dedup, indexes; look-up page for `HashMap::entry`, `retain`, `HashSet::union`...   Run: cargo run --release --example std_hashmap_entry_api_patterns
//! COMMON USES: word counts, group-by, inverted indexes, memo tables, dedup of event streams, set algebra on permissions/tags, adjacency lists.
//! METHODS: HashMap::new, HashMap::insert, HashMap::get, HashMap::get_mut, HashMap::entry, HashMap::remove, HashMap::contains_key, HashMap::len,
//! HashMap::is_empty, HashMap::iter, HashMap::keys, HashMap::values, HashMap::values_mut, HashMap::iter_mut, Entry::or_insert, Entry::or_insert_with,
//! Entry::or_insert_with_key, Entry::or_default, Entry::and_modify, Entry::key, Entry::insert_entry, OccupiedEntry::get, OccupiedEntry::get_mut,
//! OccupiedEntry::into_mut, OccupiedEntry::insert, OccupiedEntry::remove, OccupiedEntry::remove_entry, VacantEntry::insert, VacantEntry::into_key,
//! HashMap::remove_entry, HashMap::get_key_value, HashMap::retain, HashMap::drain, HashMap::extract_if, HashMap::extend, HashMap::from,
//! HashMap::into_keys, HashMap::into_values, HashMap::with_capacity, HashMap::capacity, HashMap::reserve, HashMap::try_reserve, HashMap::shrink_to_fit,
//! HashMap::shrink_to, HashMap::clear, HashMap::get_disjoint_mut, HashMap::with_hasher, HashMap::hasher, HashMap::with_capacity_and_hasher,
//! HashSet::new, HashSet::insert, HashSet::contains, HashSet::remove, HashSet::take, HashSet::replace, HashSet::get, HashSet::union,
//! HashSet::intersection, HashSet::difference, HashSet::symmetric_difference, HashSet::is_subset, HashSet::is_superset, HashSet::is_disjoint,
//! HashSet::retain, HashSet::drain, HashSet::extend, HashSet::from, std::hash::Hash, std::hash::Hasher, std::hash::BuildHasher,
//! std::hash::BuildHasherDefault, std::hash::DefaultHasher, std::collections::hash_map::RandomState, std::borrow::Borrow
//! DEFINED IN: std: `HashMap`, `HashSet`, `hash_map::{Entry, RandomState, DefaultHasher}` (not in core or alloc; they wrap the hashbrown crate).
//!   core: the traits `Hash`, `Hasher`, `BuildHasher`, the struct `BuildHasherDefault` (`core::hash`) and `Borrow` (`core::borrow`).
//! DOCS: https://doc.rust-lang.org/std/collections/struct.HashMap.html
//! NOT COVERED (and why): HashMap::try_insert (unstable in 1.98.1), HashMap::raw_entry (not in the stable API),
//! HashSet::get_or_insert_with (nightly), HashSet::entry (nightly), deprecated `hash_map::SipHasher`.
//!
//! Cheat sheet (I want to ... -> use ...):
//!   add or overwrite                       -> `insert` (returns the OLD value)
//!   read                                   -> `get(&k)` / `map[&k]` (panics when missing) / `get_key_value`
//!   count things                           -> `*m.entry(k).or_insert(0) += 1`  (or `or_default()`)
//!   group into Vec per key                 -> `m.entry(k).or_default().push(v)`
//!   insert only if absent (lazy value)     -> `entry(k).or_insert_with(|| expensive())`
//!   value depends on key                   -> `or_insert_with_key(|k| ..)`
//!   update if present else insert          -> `entry(k).and_modify(|v| ..).or_insert(init)`
//!   decide differently per case            -> `match m.entry(k) { Occupied(e) => .., Vacant(e) => .. }`
//!   remove and get                         -> `remove(&k)` / `remove_entry`; conditionally: `Entry::Occupied(e).remove()`
//!   remove many by predicate               -> `retain(|k, v| ..)` (or `extract_if` when you want the removed ones)
//!   empty it but keep the allocation       -> `drain()` / `clear()`
//!   look up with &str for String keys      -> `m.get("abc")` (Borrow<str>)
//!   mutate two values at once              -> `get_disjoint_mut([&a, &b])` (1.86)
//!   set algebra                            -> `a.union(&b)`, `intersection`, `difference`, `symmetric_difference`, `&a & &b`
//!   deterministic output                   -> sort keys / collect into BTreeMap; never print iteration order
//!   faster hashing for trusted keys        -> `BuildHasherDefault<MyHasher>` (e.g. FxHash-like) via `with_hasher`
//!
//! TRY NEXT:
//!   - [S] In section 2 replace `and_modify().or_insert()` by `or_insert()` then `+= 1` and assert both give the same counts on 1000 random words.
//!   - [S] What does `HashSet::insert` return when the value already exists AND is "equal but distinguishable"? Predict, then check with `replace` vs `insert` in section 6.
//!   - [M] Add a capacity-bounded `Lru<K,V>` to section 10 using `retain` and a monotonic counter; assert the least recently used key is evicted.
//!   - [M] Implement a custom `Hasher` (FNV-1a) and show the same map content iterates in a DIFFERENT order than RandomState, yet equal contents compare `==`.
//!   - [A] PROJECT: combine with `std_vecdeque_linkedlist_ring_buffers` (LRU via VecDeque+HashMap) and `std_btree_binaryheap_ordered` (leaderboard): a log analyser that counts per-user events with entry, keeps top-k via a heap, and prints deterministic output.
//! Read: HashMap docs https://doc.rust-lang.org/std/collections/struct.HashMap.html ; Entry docs https://doc.rust-lang.org/std/collections/hash_map/enum.Entry.html ; The Rust Performance Book (hashing) https://nnethercote.github.io/perf-book/hashing.html
#![allow(dead_code)]

use std::borrow::Borrow;
use std::collections::hash_map::{DefaultHasher, Entry, RandomState};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

const COVERED: &[&str] = &[
    "insert",
    "get",
    "get_mut",
    "entry",
    "remove",
    "contains_key",
    "len",
    "is_empty",
    "iter",
    "keys",
    "values",
    "values_mut",
    "iter_mut",
    "or_insert",
    "or_insert_with",
    "or_insert_with_key",
    "or_default",
    "and_modify",
    "Entry::key",
    "insert_entry",
    "OccupiedEntry::get",
    "OccupiedEntry::get_mut",
    "into_mut",
    "OccupiedEntry::insert",
    "OccupiedEntry::remove",
    "remove_entry",
    "VacantEntry::insert",
    "into_key",
    "get_key_value",
    "retain",
    "drain",
    "extract_if",
    "extend",
    "from",
    "into_keys",
    "into_values",
    "with_capacity",
    "capacity",
    "reserve",
    "try_reserve",
    "shrink_to_fit",
    "shrink_to",
    "clear",
    "get_disjoint_mut",
    "with_hasher",
    "hasher",
    "with_capacity_and_hasher",
    "HashSet::insert",
    "HashSet::contains",
    "HashSet::remove",
    "take",
    "replace",
    "HashSet::get",
    "union",
    "intersection",
    "difference",
    "symmetric_difference",
    "is_subset",
    "is_superset",
    "is_disjoint",
    "HashSet::retain",
    "HashSet::drain",
    "HashSet::extend",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ------------------------------------------------------------ 1. Everyday core
fn s01_everyday_core() {
    let mut stock: HashMap<String, u32> = HashMap::new();
    assert_eq!(stock.insert("apple".into(), 3), None); // insert returns the previous value
    assert_eq!(stock.insert("apple".into(), 5), Some(3));
    stock.insert("pear".into(), 1);
    assert_eq!(stock.get("apple"), Some(&5)); // &str lookup in a String-keyed map (Borrow<str>)
    assert_eq!(stock["pear"], 1); // Index panics on missing keys: only when absence is a bug
    assert!(stock.contains_key("pear") && !stock.contains_key("kiwi"));
    if let Some(n) = stock.get_mut("pear") {
        *n += 10;
    }
    assert_eq!(stock.remove("pear"), Some(11));
    assert_eq!(stock.remove("pear"), None);
    assert_eq!((stock.len(), stock.is_empty()), (1, false));
    assert_eq!(
        stock.get_key_value("apple"),
        Some((&"apple".to_string(), &5))
    );
    assert_eq!(stock.remove_entry("apple"), Some(("apple".to_string(), 5)));
    // PITFALL: map[&k] on a missing key panics
    let m: HashMap<i32, i32> = HashMap::new();
    assert!(std::panic::catch_unwind(|| m[&1]).is_err());
    // from array / extend / collect
    let mut m = HashMap::from([("a", 1), ("b", 2)]);
    m.extend([("c", 3), ("a", 10)]); // extend overwrites existing keys
    assert_eq!((m.len(), m["a"]), (3, 10));
    // iteration: order unspecified. keys/values/iter/values_mut/iter_mut
    for v in m.values_mut() {
        *v += 1;
    }
    for (_, v) in m.iter_mut() {
        *v *= 2;
    }
    let mut pairs: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
    pairs.sort();
    assert_eq!(pairs, [("a", 22), ("b", 6), ("c", 8)]);
    let mut keys: Vec<_> = m.keys().copied().collect();
    keys.sort();
    assert_eq!(keys, ["a", "b", "c"]);
    assert_eq!(m.values().sum::<i32>(), 36);
    println!("1. everyday core ok");
}

// ------------------------------------------------------------ 2. Entry API in depth
fn s02_entry_api() {
    // 2a. counting: the canonical entry idiom (one hash lookup, not two)
    let words = "the cat the dog the end".split(' ');
    let mut freq: HashMap<&str, usize> = HashMap::new();
    for w in words {
        *freq.entry(w).or_insert(0) += 1;
    }
    assert_eq!(freq["the"], 3);
    // 2b. grouping with or_default
    let mut by_len: HashMap<usize, Vec<&str>> = HashMap::new();
    for w in ["a", "bb", "c", "dd", "eee"] {
        by_len.entry(w.len()).or_default().push(w);
    }
    assert_eq!(by_len[&2], ["bb", "dd"]);
    // 2c. or_insert_with: lazy init (the closure runs only on a miss)
    let mut calls = 0;
    let mut cache: HashMap<u32, String> = HashMap::new();
    for k in [1, 2, 1, 1] {
        cache.entry(k).or_insert_with(|| {
            calls += 1;
            format!("v{k}")
        });
    }
    assert_eq!(calls, 2);
    // `or_insert(expensive())` evaluates the argument EVERY time, even on a hit: that is the pitfall.
    let mut evals = 0;
    let mut c2: HashMap<u32, u32> = HashMap::new();
    for k in [1, 1, 1] {
        c2.entry(k).or_insert({
            evals += 1;
            0
        });
    }
    assert_eq!(evals, 3);
    // 2d. or_insert_with_key: value derived from the key without cloning it first
    let mut lens: HashMap<String, usize> = HashMap::new();
    lens.entry("hello".to_string())
        .or_insert_with_key(|k| k.len());
    assert_eq!(lens["hello"], 5);
    // 2e. and_modify + or_insert: "update if present else insert"
    let mut scores: HashMap<&str, i32> = HashMap::new();
    for (n, s) in [("a", 5), ("b", 1), ("a", 7)] {
        scores
            .entry(n)
            .and_modify(|v| *v = (*v).max(s))
            .or_insert(s);
    }
    assert_eq!((scores["a"], scores["b"]), (7, 1));
    // 2f. matching Occupied / Vacant for branching logic and removal
    let mut inv: HashMap<&str, u32> = HashMap::from([("bolt", 2)]);
    fn take_one(inv: &mut HashMap<&'static str, u32>, item: &'static str) -> Result<u32, String> {
        match inv.entry(item) {
            Entry::Occupied(mut e) => {
                let left = e.get() - 1;
                if left == 0 {
                    e.remove();
                } else {
                    *e.get_mut() = left;
                } // remove at zero: no zombie keys
                Ok(left)
            }
            Entry::Vacant(e) => Err(format!("no {} in stock", e.into_key())),
        }
    }
    assert_eq!(take_one(&mut inv, "bolt"), Ok(1));
    assert_eq!(take_one(&mut inv, "bolt"), Ok(0));
    assert!(!inv.contains_key("bolt"));
    assert_eq!(take_one(&mut inv, "bolt"), Err("no bolt in stock".into()));
    // 2g. VacantEntry::insert returns &mut V; OccupiedEntry::insert returns the old value; into_mut extends the borrow
    let mut m: HashMap<&str, Vec<i32>> = HashMap::new();
    if let Entry::Vacant(v) = m.entry("k") {
        v.insert(vec![1]).push(2);
    }
    if let Entry::Occupied(o) = m.entry("k") {
        let old = o.into_mut();
        old.push(3);
    }
    assert_eq!(m["k"], [1, 2, 3]);
    if let Entry::Occupied(mut o) = m.entry("k") {
        assert_eq!(o.insert(vec![]), vec![1, 2, 3]);
    }
    // 2h. insert_entry (since 1.83): set the value and keep an OccupiedEntry handle
    let mut m2: HashMap<&str, i32> = HashMap::new();
    let e = m2.entry("x").insert_entry(9);
    assert_eq!((*e.key(), *e.get()), ("x", 9));
    // 2i. Entry::key reads the key before deciding
    assert_eq!(*m2.entry("y").key(), "y");
    assert!(!m2.contains_key("y")); // `entry` alone does not insert
    // 2j. PITFALL: the entry holds &mut to the map; you cannot use the map while an Entry is alive (compile error), so finish with it.
    println!("2. entry api ok");
}

// ------------------------------------------------------------ 3. Bulk operations: retain, drain, extract_if, into_*, capacity
fn s03_bulk_ops() {
    let mut sessions: HashMap<u32, u64> = (1..=10).map(|id| (id, id as u64 * 100)).collect(); // id -> last_seen
    let now = 600;
    // retain: expire in one pass (instead of collecting keys to remove, then removing: two passes + allocation)
    sessions.retain(|_, seen| *seen + 300 > now);
    assert_eq!(sessions.len(), 7);
    // extract_if (1.88): remove AND receive the removed pairs
    let mut gone: Vec<_> = sessions.extract_if(|_, seen| *seen > 900).collect();
    gone.sort();
    assert_eq!(gone, [(10, 1000)]);
    // drain: take everything, keep capacity for reuse (a per-tick buffer)
    let cap = sessions.capacity();
    let mut drained: Vec<_> = sessions.drain().collect();
    drained.sort();
    assert_eq!(drained.first(), Some(&(4, 400)));
    // PITFALL: `capacity()` is only a lower bound on what fits without reallocating, so never assert it is unchanged. Removals can leave
    // "deleted" slots that lower the reported number (which ones depends on the per-map random hash seed), and `drain`/`clear` reset them.
    // An earlier version of this line asserted `== cap` and failed in about a quarter of runs. The portable claim is `>=`.
    assert!(sessions.is_empty() && sessions.capacity() >= cap);
    // into_keys / into_values consume the map without cloning
    let m = HashMap::from([(1, "a"), (2, "b")]);
    let mut ks: Vec<_> = m.clone().into_keys().collect();
    ks.sort();
    assert_eq!(ks, [1, 2]);
    let mut vs: Vec<_> = m.into_values().collect();
    vs.sort();
    assert_eq!(vs, ["a", "b"]);
    // capacity management
    let mut big: HashMap<u32, u32> = HashMap::with_capacity(1000);
    assert!(big.capacity() >= 1000);
    let before = big.capacity();
    for i in 0..1000 {
        big.insert(i, i);
    }
    assert_eq!(
        big.capacity(),
        before,
        "pre-sizing: no rehash/growth while filling"
    );
    big.retain(|k, _| *k < 10);
    big.shrink_to_fit(); // give memory back after a spike
    assert!(big.capacity() < before);
    big.reserve(500);
    assert!(big.capacity() >= 510);
    assert!(big.try_reserve(10).is_ok());
    big.shrink_to(64);
    assert!(big.capacity() >= 64.min(big.len()));
    big.clear();
    assert!(big.is_empty());
    println!("3. bulk ops ok");
}

// ------------------------------------------------------------ 4. get_disjoint_mut
fn s04_disjoint_mut() {
    // Scenario: transfer between two accounts held in one map (two &mut at once is normally a borrow error)
    let mut acct: HashMap<&str, i64> = HashMap::from([("alice", 100), ("bob", 5)]);
    let [a, b] = acct.get_disjoint_mut(["alice", "bob"]); // since 1.86; panics if keys are equal
    let (a, b) = (a.unwrap(), b.unwrap());
    *a -= 30;
    *b += 30;
    assert_eq!((acct["alice"], acct["bob"]), (70, 35));
    // missing keys yield None, not a panic
    let [x, y] = acct.get_disjoint_mut(["alice", "zed"]);
    assert!(x.is_some() && y.is_none());
    // PITFALL: the same key twice panics (it would alias a &mut)
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut m = HashMap::from([(1, 1)]);
        let _ = m.get_disjoint_mut([&1, &1]);
    }));
    assert!(r.is_err());
    println!("4. get_disjoint_mut ok");
}

// ------------------------------------------------------------ 5. Borrowed lookups, Hash/Eq contract, custom keys
#[derive(Debug, Clone)]
struct CaseInsensitive(String);
impl PartialEq for CaseInsensitive {
    fn eq(&self, o: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&o.0)
    }
}
impl Eq for CaseInsensitive {}
impl Hash for CaseInsensitive {
    // CONTRACT: a == b implies hash(a) == hash(b). So hash the LOWERCASED bytes, matching eq.
    fn hash<H: Hasher>(&self, h: &mut H) {
        for b in self.0.bytes() {
            h.write_u8(b.to_ascii_lowercase());
        }
        h.write_u8(0xff); // terminator: prevents ("ab","c") ~ ("a","bc") style collisions in composite keys
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
struct UserId(u32); // newtype keys: cannot mix up user ids and order ids

fn s05_keys() {
    let mut m = HashMap::new();
    m.insert(CaseInsensitive("Content-Type".into()), 1);
    assert_eq!(m.get(&CaseInsensitive("content-TYPE".into())), Some(&1));
    // The contract violation: hashing the raw string while eq ignores case would make this lookup fail randomly.
    #[derive(PartialEq, Eq, Debug)]
    struct Bad(String);
    impl Hash for Bad {
        fn hash<H: Hasher>(&self, h: &mut H) {
            self.0.hash(h)
        }
    } // case-sensitive hash
    let _ = Bad("x".into()); // (not used in a map: the failure is nondeterministic, so we only demonstrate with hashes)
    let h = |s: &str| {
        let mut d = DefaultHasher::new();
        CaseInsensitive(s.into()).hash(&mut d);
        d.finish()
    };
    assert_eq!(h("AbC"), h("aBc")); // equal values, equal hashes
    // Borrow: lookup by a borrowed form without allocating a String
    fn count<K: Borrow<str> + Hash + Eq>(m: &HashMap<K, u32>, k: &str) -> u32 {
        m.get(k).copied().unwrap_or(0)
    }
    let owned: HashMap<String, u32> = HashMap::from([("a".to_string(), 1)]);
    let borrowed: HashMap<&str, u32> = HashMap::from([("a", 1)]);
    assert_eq!((count(&owned, "a"), count(&borrowed, "a")), (1, 1));
    // Tuple / composite keys avoid string concatenation ("user:42:day:3")
    let mut hits: HashMap<(UserId, u8), u32> = HashMap::new();
    *hits.entry((UserId(42), 3)).or_default() += 1;
    assert_eq!(hits[&(UserId(42), 3)], 1);
    // Floats can't be keys directly (no Eq/Hash): key on `to_bits()` or a fixed-point integer
    let mut fm: HashMap<u64, &str> = HashMap::new();
    fm.insert(1.5f64.to_bits(), "x");
    assert_eq!(fm[&1.5f64.to_bits()], "x"); // beware: NaN payloads and -0.0 vs 0.0 are different keys
    assert_ne!(0.0f64.to_bits(), (-0.0f64).to_bits());
    println!("5. keys ok");
}

// ------------------------------------------------------------ 6. HashSet
fn s06_hashset() {
    let a: HashSet<u32> = (1..=5).collect();
    let b: HashSet<u32> = (4..=8).collect();
    assert!(a.contains(&1) && !a.contains(&8));
    let sorted = |it: &mut dyn Iterator<Item = &u32>| {
        let mut v: Vec<u32> = it.copied().collect();
        v.sort();
        v
    };
    assert_eq!(sorted(&mut a.union(&b)), [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(sorted(&mut a.intersection(&b)), [4, 5]);
    assert_eq!(sorted(&mut a.difference(&b)), [1, 2, 3]);
    assert_eq!(sorted(&mut a.symmetric_difference(&b)), [1, 2, 3, 6, 7, 8]);
    // operators build new sets: &a | &b, &a & &b, &a - &b, &a ^ &b
    assert_eq!((&a & &b).len(), 2);
    assert_eq!((&a | &b).len(), 8);
    assert!(HashSet::from([4, 5]).is_subset(&a) && a.is_superset(&HashSet::from([1])));
    assert!(HashSet::from([100]).is_disjoint(&a));
    // insert returns whether it was new: dedup a stream while preserving first-seen order
    let mut seen = HashSet::new();
    let uniq: Vec<_> = [3, 1, 3, 2, 1]
        .into_iter()
        .filter(|x| seen.insert(*x))
        .collect();
    assert_eq!(uniq, [3, 1, 2]);
    // take / get / replace: retrieve the STORED value (matters when Eq ignores some fields: interning)
    #[derive(Debug)]
    struct Sym {
        id: u32,
        note: &'static str,
    }
    impl PartialEq for Sym {
        fn eq(&self, o: &Self) -> bool {
            self.id == o.id
        }
    }
    impl Eq for Sym {}
    impl Hash for Sym {
        fn hash<H: Hasher>(&self, h: &mut H) {
            self.id.hash(h)
        }
    }
    let mut set = HashSet::new();
    set.insert(Sym {
        id: 1,
        note: "first",
    });
    assert!(!set.insert(Sym {
        id: 1,
        note: "second"
    })); // insert keeps the OLD value
    assert_eq!(set.get(&Sym { id: 1, note: "" }).unwrap().note, "first");
    let old = set
        .replace(Sym {
            id: 1,
            note: "third",
        })
        .unwrap(); // replace swaps in the NEW one
    assert_eq!(old.note, "first");
    assert_eq!(set.take(&Sym { id: 1, note: "" }).unwrap().note, "third");
    assert!(set.is_empty());
    let mut s: HashSet<i32> = (0..10).collect();
    s.retain(|x| x % 3 == 0);
    s.extend([100]);
    assert_eq!(s.len(), 5);
    assert_eq!(s.drain().count(), 5);
    assert!(s.remove(&1) == false);
    println!("6. hashset ok");
}

// ------------------------------------------------------------ 7. Nondeterminism and hashers
/// A tiny FNV-1a hasher: fast for short keys, deterministic across runs. NOT DoS-resistant: use only for trusted keys.
#[derive(Default)]
struct Fnv(u64);
impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf29ce484222325;
        }
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x100000001b3);
        }
    }
}
type FnvMap<K, V> = HashMap<K, V, BuildHasherDefault<Fnv>>;

fn s07_hashers_and_determinism() {
    // Default RandomState (SipHash 1-3 with random keys): order differs per map instance and per run.
    let a: HashMap<u32, u32> = (0..64).map(|i| (i, i)).collect();
    let b: HashMap<u32, u32> = (0..64).map(|i| (i, i)).collect();
    assert_eq!(a, b); // equality ignores order
    let ka: Vec<_> = a.keys().collect();
    let kb: Vec<_> = b.keys().collect();
    println!(
        "   same contents, same order across two RandomState maps? {} (never rely on it)",
        ka == kb
    );
    // Deterministic output recipe 1: sort. Recipe 2: collect into BTreeMap. Recipe 3: a fixed hasher (tests only).
    let sorted: BTreeMap<_, _> = a.iter().collect();
    assert_eq!(sorted.keys().next(), Some(&&0));
    let f1: FnvMap<u32, u32> = (0..64).map(|i| (i, i)).collect();
    let f2: FnvMap<u32, u32> = (0..64).map(|i| (i, i)).collect();
    assert!(
        f1.keys().eq(f2.keys()),
        "fixed hasher + same insertion history = same order"
    );
    // Hasher choice: BuildHasher::hash_one (1.71) hashes a value with a builder
    let rs = RandomState::new();
    assert_eq!(rs.hash_one("x"), rs.hash_one("x"));
    assert_eq!(
        BuildHasherDefault::<Fnv>::default().hash_one(1u8),
        BuildHasherDefault::<Fnv>::default().hash_one(1u8)
    );
    // with_hasher / hasher / with_capacity_and_hasher
    let mut m = HashMap::with_capacity_and_hasher(8, BuildHasherDefault::<Fnv>::default());
    m.insert("k", 1);
    let _bh = m.hasher();
    let m2: HashMap<&str, i32, RandomState> = HashMap::with_hasher(RandomState::new());
    assert!(m2.is_empty());
    // Take-away: SipHash is the safe default against HashDoS on attacker-controlled keys; swap only with measurements.
    println!("7. hashers ok");
}

// ------------------------------------------------------------ 8. Scenarios: inverted index, graph adjacency, word freq top
fn s08_scenarios() {
    // 8a. inverted index: term -> sorted doc ids, with HashSet intersection for AND queries
    let docs = [
        "rust is fast",
        "go is simple",
        "rust and go",
        "fast and simple",
    ];
    let mut index: HashMap<&str, HashSet<usize>> = HashMap::new();
    for (id, d) in docs.iter().enumerate() {
        for term in d.split(' ') {
            index.entry(term).or_default().insert(id);
        }
    }
    let and = |a: &str, b: &str| -> Vec<usize> {
        let (x, y) = (index.get(a), index.get(b));
        let mut v: Vec<_> = match (x, y) {
            (Some(x), Some(y)) => x.intersection(y).copied().collect(),
            _ => vec![],
        };
        v.sort();
        v
    };
    assert_eq!(and("rust", "go"), [2]);
    assert_eq!(and("fast", "simple"), [3]);
    assert_eq!(and("rust", "zig"), Vec::<usize>::new());
    // 8b. graph adjacency
    let mut g: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in [("a", "b"), ("a", "c"), ("b", "c")] {
        g.entry(a).or_default().push(b);
        g.entry(b).or_default(); // make sure sink nodes exist as keys
    }
    assert_eq!(g["c"].len(), 0);
    assert_eq!(g.len(), 3);
    // 8c. LRU-ish counters with a logical clock (injected, no wall time)
    let mut last_used: HashMap<&str, u64> = HashMap::new();
    for (tick, k) in ["x", "y", "x", "z"].iter().enumerate() {
        last_used.insert(k, tick as u64);
    }
    let victim = last_used.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k);
    assert_eq!(victim, Some("y"));
    // 8d. diff two maps
    let old = HashMap::from([("a", 1), ("b", 2), ("c", 3)]);
    let new = HashMap::from([("b", 2), ("c", 4), ("d", 5)]);
    let removed: HashSet<_> = old.keys().filter(|k| !new.contains_key(*k)).collect();
    let added: HashSet<_> = new.keys().filter(|k| !old.contains_key(*k)).collect();
    let changed: HashSet<_> = old
        .iter()
        .filter(|(k, v)| new.get(*k).is_some_and(|nv| nv != *v))
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        (removed, added, changed),
        (
            HashSet::from([&"a"]),
            HashSet::from([&"d"]),
            HashSet::from([&"c"])
        )
    );
    println!("8. scenarios ok");
}

// ------------------------------------------------------------ 9. Randomised differential test vs a Vec oracle
fn s09_differential() {
    let mut rng = Rng(0xC0FFEE);
    let mut m: HashMap<u8, u32> = HashMap::new();
    let mut oracle: Vec<(u8, u32)> = Vec::new(); // association list
    for step in 0..5000u32 {
        let k = rng.below(40) as u8;
        match rng.below(4) {
            0 => {
                let old = m.insert(k, step);
                let o = oracle.iter_mut().find(|(ok, _)| *ok == k);
                match o {
                    Some(e) => {
                        assert_eq!(old, Some(e.1));
                        e.1 = step
                    }
                    None => {
                        assert_eq!(old, None);
                        oracle.push((k, step))
                    }
                }
            }
            1 => {
                let got = m.remove(&k);
                let pos = oracle.iter().position(|(ok, _)| *ok == k);
                assert_eq!(got, pos.map(|p| oracle.swap_remove(p).1));
            }
            2 => {
                *m.entry(k).or_insert(0) += 1;
                match oracle.iter_mut().find(|(ok, _)| *ok == k) {
                    Some(e) => e.1 += 1,
                    None => oracle.push((k, 1)),
                }
            }
            _ => assert_eq!(
                m.get(&k).copied(),
                oracle.iter().find(|(ok, _)| *ok == k).map(|e| e.1)
            ),
        }
        assert_eq!(m.len(), oracle.len());
    }
    println!("9. differential ok");
}

// ------------------------------------------------------------ 10. Production tips and tricks
fn s10_production_tips() {
    // 1. entry() instead of get-then-insert: one hash, no race window in single-thread code (clippy::map_entry).
    let mut m: HashMap<u32, u32> = HashMap::new();
    // slow/buggy: if !m.contains_key(&1) { m.insert(1, 0); } *m.get_mut(&1).unwrap() += 1;
    *m.entry(1).or_insert(0) += 1;
    assert_eq!(m[&1], 1);

    // 2. or_insert_with for expensive/allocating defaults (clippy::or_fun_call): or_insert(Vec::new()) allocates nothing, but String::from(..)/format! would run every call.
    let mut c: HashMap<u8, String> = HashMap::new();
    c.entry(1).or_insert_with(|| "x".repeat(3));
    assert_eq!(c[&1], "xxx");

    // 3. Pre-size with with_capacity when the count is known: asserted no growth in section 3.
    let m = HashMap::<u32, u32>::with_capacity(100);
    assert!(m.capacity() >= 100);

    // 4. Lookup with &str on String keys: never allocate a String just to call get (clippy::unnecessary_to_owned).
    let m: HashMap<String, i32> = HashMap::from([("k".into(), 1)]);
    assert_eq!(m.get("k"), Some(&1));

    // 5. retain instead of collect-keys-then-remove loop (clippy::manual_retain applies to Vec/HashMap).
    let mut m: HashMap<i32, i32> = (0..10).map(|i| (i, i)).collect();
    m.retain(|k, _| k % 2 == 0);
    assert_eq!(m.len(), 5);

    // 6. Deterministic iteration for logs/tests/snapshots: sort or BTreeMap (HashMap order differs between runs by design).
    let mut keys: Vec<_> = m.keys().copied().collect();
    keys.sort_unstable();
    assert_eq!(keys, [0, 2, 4, 6, 8]);

    // 7. Use `HashSet::insert`'s bool for dedup instead of contains+insert (clippy::set_contains_or_insert).
    let mut seen = HashSet::new();
    assert!(seen.insert("a") && !seen.insert("a"));

    // 8. Newtype keys (UserId vs OrderId) so a swapped argument is a compile error, not an empty lookup.
    let mut owners: HashMap<UserId, &str> = HashMap::new();
    owners.insert(UserId(1), "ann");
    assert_eq!(owners.get(&UserId(1)), Some(&"ann"));

    // 9. A Mutex<HashMap> guard must not be held across I/O or .await: copy out what you need, drop the guard, then act.
    let shared = std::sync::Mutex::new(HashMap::from([(1, "v".to_string())]));
    let v = shared.lock().unwrap().get(&1).cloned(); // guard dropped at the end of this statement
    assert_eq!(v.as_deref(), Some("v"));

    // 10. For read-mostly shared maps use `Arc<HashMap>` and swap the Arc on update (copy-on-write) instead of locking every read.
    let snapshot = std::sync::Arc::new(HashMap::from([(1, 1)]));
    let reader = std::sync::Arc::clone(&snapshot);
    let mut next = (*snapshot).clone();
    next.insert(2, 2);
    let snapshot = std::sync::Arc::new(next);
    assert_eq!((reader.len(), snapshot.len()), (1, 2)); // old readers keep a consistent view

    // 11. HashDoS: keep RandomState for attacker-controlled keys; a faster hasher is for internal ids (measure first).
    // 12. Don't use f32/f64 as keys; use integers (cents), `to_bits`, or `ordered-float` (external crate).
    // 13. `HashMap<K, ()>` is a HashSet: use HashSet. `HashMap<K, bool>` is usually a HashSet too.
    // 14. Large values: store `Arc<V>`/index into a Vec rather than moving big structs on rehash (growth moves entries).
    println!("10. production tips ok");
}

fn main() {
    let mut s = COVERED.to_vec();
    s.sort();
    s.dedup();
    assert_eq!(s.len(), COVERED.len(), "COVERED has duplicates");
    s01_everyday_core();
    s02_entry_api();
    s03_bulk_ops();
    s04_disjoint_mut();
    s05_keys();
    s06_hashset();
    s07_hashers_and_determinism();
    s08_scenarios();
    s09_differential();
    s10_production_tips();
    println!("all hashmap sections passed");
}

#[cfg(test)]
mod tests {
    #[test]
    fn covered_unique() {
        let mut v = super::COVERED.to_vec();
        v.sort();
        let n = v.len();
        v.dedup();
        assert_eq!(n, v.len());
    }
}
