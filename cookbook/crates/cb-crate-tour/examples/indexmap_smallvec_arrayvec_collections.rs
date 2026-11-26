//! RECIPE 1109 (crates): collection crates: indexmap, smallvec, arrayvec, tinyvec, hashbrown, fast hashers, bitflags, bit-set, bitvec
//! LEVEL: intermediate
//! Use in: deterministic output, allocation-sensitive hot paths, fixed-capacity code, compact flag sets.   Run: cargo run --release -p cb-crate-tour --example indexmap_smallvec_arrayvec_collections
//! COMMON USES: indexmap keeps JSON/TOML key order and gives dense indices to graph nodes; smallvec is common in compilers;
//! rustc-hash is the Rust compiler's hasher; hashbrown is the table behind std's HashMap; bitflags wraps OS and protocol flags.
//! METHODS: IndexMap::insert, IndexMap::insert_full, IndexMap::entry, Entry::or_insert, IndexMap::get_index, IndexMap::get_index_of, IndexMap::get_full, IndexMap::swap_remove, IndexMap::shift_remove, IndexMap::sort_keys, IndexMap::sort_by, IndexMap::binary_search_keys, IndexMap::move_index, IndexMap::retain, IndexMap::pop, IndexSet::insert_full, IndexSet::difference, SmallVec::new, SmallVec::push, SmallVec::spilled, SmallVec::inline_size, SmallVec::into_vec, smallvec::smallvec, ArrayVec::try_push, ArrayVec::remaining_capacity, ArrayVec::is_full, ArrayString::try_push_str, TinyVec::is_inline, hashbrown::HashMap::entry_ref, hashbrown::HashTable::entry, hashbrown::HashTable::find, FxHashMap, FxBuildHasher, ahash::AHashMap, ahash::RandomState, bitflags::bitflags, Flags::from_bits, Flags::from_bits_truncate, Flags::from_bits_retain, Flags::contains, Flags::intersects, Flags::insert, Flags::remove, Flags::toggle, Flags::iter_names, bitflags::parser::from_str, bitflags::parser::to_writer, IndexMap::{insert_sorted, shift_insert, swap_indices, drain, extract_if, get_range, get_disjoint_mut, reverse}, SmallVec::{retain, drain, extend_from_slice, shrink_to_fit, insert}, hashbrown::HashMap::{get_many_mut, extract_if}, bit_set::BitSet::{insert, remove, contains, count, union_with, intersect_with, difference_with, union, intersection, is_subset, is_disjoint, get_ref, make_empty}, bitvec::{bitvec!, bits!, array::BitArray, order::{Lsb0, Msb0}, view::BitView::{view_bits, view_bits_mut}, field::BitField::{load_be, store_be, load_le}, slice::BitSlice::{first_zero, iter_ones, count_ones, split_at, fill}}
//! DOCS: https://docs.rs/indexmap/2/ https://docs.rs/smallvec/1/ https://docs.rs/arrayvec/0.7/ https://docs.rs/tinyvec/1/ https://docs.rs/hashbrown/0.15/ https://docs.rs/rustc-hash/2/ https://docs.rs/ahash/0.8/ https://docs.rs/bitflags/2/ https://docs.rs/bit-set/0.11/ https://docs.rs/bitvec/1/
//! CRATES: indexmap = "2", smallvec = "1", arrayvec = "0.7", tinyvec = "1" (alloc), hashbrown = "0.15" (not the newest line), rustc-hash = "2", ahash = "0.8", bitflags = "2", bit-set = "0.11", bitvec = "1"
//!
//! CHEAT SHEET
//!   hash map that remembers insertion order       -> IndexMap (swap_remove: O(1), reorders; shift_remove: O(n), keeps order)
//!   usually <= N elements, sometimes more          -> SmallVec<[T; N]> (spills to the heap)
//!   never more than N, no heap                     -> ArrayVec<T, N> (try_push returns the item back)
//!   same, without unsafe in the crate              -> tinyvec (elements need Default)
//!   look up by &str, own the String only on insert -> hashbrown entry_ref
//!   key stored elsewhere (index into an arena)     -> hashbrown::HashTable
//!   set of small dense integers (node ids)         -> BitSet (set algebra) ; fixed universe <= 64 -> a u64
//!   individual bits, bit fields in wire formats    -> bitvec (BitSlice over your own bytes; order is Msb0 or Lsb0)
//!   trusted integer keys, speed                    -> FxHashMap ; untrusted keys -> std or ahash with random state
//!
//! PITFALLS: BitSet memory is proportional to the LARGEST element, not to the number of elements; bitvec's Msb0 and Lsb0
//! number the bits of a byte in opposite directions, and load_be/load_le is a second, independent choice. SmallVec's inline buffer makes the VALUE big (moves cost more, so measure); FxHash has no random seed, so an
//! attacker who chooses keys can force collisions; IndexMap `==` ignores order.
//!
//! Read: https://nnethercote.github.io/perf-book/heap-allocations.html ; https://nnethercote.github.io/perf-book/hashing.html
//!
//! TRY NEXT:
//!   - [S] Predict size_of for SmallVec<[u8; 64]> and Option<ArrayVec<u8, 7>>, then print them.
//!   - [M] Build an LRU cache from IndexMap with move_index; explain why it is O(n) and when that is still fine.
//!   - [A] PROJECT: write a string interner twice (IndexSet, then HashTable + one big String) and count allocations.
#![allow(dead_code)]
use arrayvec::{ArrayString, ArrayVec};
use hashbrown::hash_table::{Entry as TableEntry, HashTable};
use indexmap::{IndexMap, IndexSet};
use smallvec::{SmallVec, smallvec};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::time::Instant;
use tinyvec::TinyVec;

/// Counts heap allocations on the CURRENT THREAD, so measurements are exact even when tests run in parallel.
struct Counting;
thread_local! { static ALLOCS: Cell<usize> = const { Cell::new(0) }; }
// SAFETY: every call is forwarded unchanged to the system allocator; the only extra work is a thread-local counter
// whose const initialiser does not allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;
fn allocations<T>(f: impl FnOnce() -> T) -> (usize, T) {
    let before = ALLOCS.get();
    let out = std::hint::black_box(f());
    (ALLOCS.get() - before, out)
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn keys<V>(m: &IndexMap<&'static str, V>) -> Vec<&'static str> {
    m.keys().copied().collect()
}
fn indexmap_order_and_indices() {
    let mut m: IndexMap<&str, u32> = ["c", "a", "d", "b"].into_iter().zip(1..).collect();
    assert_eq!(keys(&m), ["c", "a", "d", "b"]);
    assert_eq!(m.insert("a", 20), Some(2));
    assert_eq!(
        keys(&m),
        ["c", "a", "d", "b"],
        "updating keeps the position"
    );
    assert_eq!(m.insert_full("e", 5), (4, None));
    assert_eq!(
        (m.get_index(1), m.get_index_of("d"), m.get_full("b")),
        (Some((&"a", &20)), Some(2), Some((3, &"b", &4)))
    );
    *m.entry("z").or_insert(0) += 1;
    assert_eq!(m.pop(), Some(("z", 1)));

    let mut swap = m.clone();
    swap.swap_remove("a");
    assert_eq!(
        keys(&swap),
        ["c", "e", "d", "b"],
        "the LAST entry moved into the hole"
    );
    let mut shift = m.clone();
    shift.shift_remove("a");
    assert_eq!(keys(&shift), ["c", "d", "b", "e"]);
    assert_eq!(swap, shift, "PITFALL: equality ignores order");

    shift.sort_keys();
    assert_eq!(keys(&shift), ["b", "c", "d", "e"]);
    assert_eq!(shift.binary_search_keys(&"d"), Ok(2));
    assert_eq!(shift.binary_search_keys(&"a"), Err(0));
    shift.sort_by(|_, a, _, b| b.cmp(a)); // by value, descending
    assert_eq!(keys(&shift), ["e", "b", "d", "c"]);
    shift.move_index(3, 0);
    assert_eq!(keys(&shift), ["c", "e", "b", "d"]);
    shift.retain(|_, v| *v > 2);
    assert_eq!(keys(&shift), ["e", "b", "d"]);

    // IndexSet as an interner: stable dense ids, id -> string lookup for free.
    let mut names = IndexSet::new();
    let ids: Vec<usize> = ["x", "y", "x", "z", "y"]
        .iter()
        .map(|s| names.insert_full(*s).0)
        .collect();
    assert_eq!(ids, [0, 1, 0, 2, 1]);
    assert_eq!(names[2], "z");
    let other: IndexSet<&str> = ["y"].into_iter().collect();
    assert_eq!(
        names.difference(&other).copied().collect::<Vec<_>>(),
        ["x", "z"]
    );
    // Order survives serde (feature "serde").
    let text = r#"{"z":1,"a":2}"#;
    let ordered: IndexMap<String, u8> = serde_json::from_str(text).unwrap();
    assert_eq!(serde_json::to_string(&ordered).unwrap(), text);

    // Differential test against a Vec-of-pairs model.
    let mut r = Rng(0x1109);
    let (mut map, mut model) = (IndexMap::new(), Vec::<(u64, u64)>::new());
    for step in 0..5000 {
        let k = r.below(20);
        let found = model.iter().position(|p| p.0 == k);
        match r.below(3) {
            0 => {
                map.insert(k, step);
                match found {
                    Some(i) => model[i].1 = step,
                    None => model.push((k, step)),
                }
            }
            1 => assert_eq!(map.shift_remove(&k), found.map(|i| model.remove(i).1)),
            _ => assert_eq!(map.swap_remove(&k), found.map(|i| model.swap_remove(i).1)),
        }
        assert!(map.iter().map(|(k, v)| (*k, *v)).eq(model.iter().copied()));
    }
}

fn inline_vectors() {
    // Inline up to N: no heap allocation at all until the N+1th push.
    let (n, v) = allocations(|| {
        let mut v: SmallVec<[u32; 8]> = SmallVec::new();
        v.extend(0..8);
        v
    });
    assert_eq!((n, v.spilled(), v.inline_size()), (0, false, 8));
    let (n, v) = allocations(move || {
        let mut v = v;
        v.push(8);
        v
    });
    assert!(
        n >= 1 && v.spilled(),
        "element 9 moved everything to the heap"
    );
    assert_eq!(
        &v[..],
        &(0..9).collect::<Vec<_>>()[..],
        "it derefs to a slice either way"
    );
    assert_eq!(v.into_vec().len(), 9);
    let s: SmallVec<[u8; 4]> = smallvec![1, 2, 3];
    assert!(!s.spilled());
    // 1000 tiny lists: Vec allocates 1000 times, SmallVec never.
    let (vec_allocs, _) = allocations(|| {
        (0..1000u32)
            .map(|i| vec![i, i + 1])
            .map(|v| v.len())
            .sum::<usize>()
    });
    let (small_allocs, _) = allocations(|| {
        (0..1000u32)
            .map(|i| -> SmallVec<[u32; 2]> { smallvec![i, i + 1] })
            .map(|v| v.len())
            .sum::<usize>()
    });
    assert_eq!((vec_allocs, small_allocs), (1000, 0));

    // ArrayVec: capacity is a hard limit and the error returns the rejected value.
    let mut a: ArrayVec<String, 2> = ArrayVec::new();
    a.push("a".into());
    a.push("b".into());
    assert!(a.is_full() && a.remaining_capacity() == 0);
    let rejected = a.try_push("c".into()).unwrap_err().element();
    assert_eq!((rejected.as_str(), a.len()), ("c", 2));
    let mut name = ArrayString::<5>::new();
    assert!(name.try_push_str("héll").is_ok()); // 5 BYTES: é takes two
    assert!(name.try_push_str("o").is_err());
    assert_eq!(name.as_str(), "héll");
    let (n, _) = allocations(|| (0..100u8).collect::<ArrayVec<u8, 128>>().len());
    assert_eq!(n, 0);

    // TinyVec: an enum of Inline(ArrayVec) | Heap(Vec), written without unsafe; items must be Default.
    let mut t: TinyVec<[u16; 4]> = tinyvec::tiny_vec!(1, 2, 3, 4);
    assert!(t.is_inline());
    t.push(5);
    assert!(t.is_heap());

    println!(
        "size_of: Vec<u64> {}, SmallVec<[u64;2]> {}, SmallVec<[u64;32]> {}, ArrayVec<u64,32> {}, TinyVec<[u64;4]> {}",
        size_of::<Vec<u64>>(),
        size_of::<SmallVec<[u64; 2]>>(),
        size_of::<SmallVec<[u64; 32]>>(),
        size_of::<ArrayVec<u64, 32>>(),
        size_of::<TinyVec<[u64; 4]>>()
    );
    assert!(
        size_of::<SmallVec<[u64; 32]>>() >= 256,
        "the inline buffer lives inside the value"
    );
}

fn hashbrown_directly() {
    // entry_ref: look up with &str; the String is allocated only when the key is new.
    let mut counts: hashbrown::HashMap<String, u32> = hashbrown::HashMap::with_capacity(8);
    let words = "a b a c a b".split(' ');
    let (n, _) = allocations(|| {
        for w in words.clone() {
            *counts.entry_ref(w).or_insert(0) += 1;
        }
    });
    assert_eq!(
        (counts["a"], counts["b"], counts["c"], n),
        (3, 2, 1, 3),
        "3 distinct words, 3 allocations"
    );

    // HashTable: you supply hash and equality. Here the table stores only an index, and the strings live ONCE in an arena.
    struct Interner {
        strings: Vec<String>,
        table: HashTable<u32>,
        hasher: rustc_hash::FxBuildHasher,
    }
    impl Interner {
        fn intern(&mut self, s: &str) -> u32 {
            let (strings, hasher) = (&mut self.strings, &self.hasher);
            let hash = hasher.hash_one(s);
            match self.table.entry(
                hash,
                |&i| strings[i as usize] == s,
                |&i| hasher.hash_one(strings[i as usize].as_str()),
            ) {
                TableEntry::Occupied(e) => *e.get(),
                TableEntry::Vacant(e) => {
                    let id = strings.len() as u32;
                    strings.push(s.to_owned());
                    e.insert(id);
                    id
                }
            }
        }
        fn lookup(&self, s: &str) -> Option<u32> {
            self.table
                .find(self.hasher.hash_one(s), |&i| self.strings[i as usize] == s)
                .copied()
        }
    }
    let mut it = Interner {
        strings: vec![],
        table: HashTable::new(),
        hasher: Default::default(),
    };
    let mut oracle = HashMap::new();
    let mut r = Rng(5);
    for _ in 0..5000 {
        let s = format!("k{}", r.below(700));
        let next = oracle.len() as u32;
        assert_eq!(it.intern(&s), *oracle.entry(s).or_insert(next));
    }
    assert!(
        oracle.iter().all(|(s, &id)| it.lookup(s) == Some(id)) && it.lookup("absent").is_none()
    );
}

/// Same operations, different hasher: results must be identical; only speed and DoS resistance differ.
fn count_with<S: BuildHasher + Default, K: Hash + Eq + Clone>(keys: &[K]) -> (usize, u128) {
    let start = Instant::now();
    let mut m: HashMap<K, u32, S> = HashMap::default();
    for k in keys {
        *m.entry(k.clone()).or_insert(0) += 1;
    }
    (m.len(), start.elapsed().as_micros())
}
fn hashers() {
    let keys: Vec<u64> = (0..200_000).map(|i| i % 50_000).collect();
    let std = count_with::<std::collections::hash_map::RandomState, u64>(&keys);
    let fx = count_with::<rustc_hash::FxBuildHasher, u64>(&keys);
    let ah = count_with::<ahash::RandomState, u64>(&keys);
    assert_eq!([std.0, fx.0, ah.0], [50_000; 3]);
    println!(
        "count 200k u64 keys (timing is machine dependent, not asserted): std {}us, FxHash {}us, ahash {}us",
        std.1, fx.1, ah.1
    );
    // Fx is unseeded: the same hash in every process. std and each new ahash state are randomly seeded.
    assert_eq!(
        rustc_hash::FxBuildHasher.hash_one(1u64),
        rustc_hash::FxBuildHasher.hash_one(1u64)
    );
    assert_ne!(
        std::collections::hash_map::RandomState::new().hash_one(1u64),
        std::collections::hash_map::RandomState::new().hash_one(1u64)
    );
    let mut fast: rustc_hash::FxHashMap<u32, u32> = Default::default();
    fast.insert(1, 2);
    let mut a = ahash::AHashMap::new();
    a.insert(1, 2);
    assert_eq!(fast[&1], a[&1]);
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Perm: u8 {
        const READ = 0b001;
        const WRITE = 0b010;
        const EXEC = 0b100;
        const RW = Self::READ.bits() | Self::WRITE.bits();
    }
}
fn flags() {
    let mut p = Perm::READ | Perm::EXEC;
    assert!(
        p.contains(Perm::READ) && !p.contains(Perm::RW),
        "contains = ALL bits"
    );
    assert!(p.intersects(Perm::RW), "intersects = ANY bit");
    p.insert(Perm::WRITE);
    p.remove(Perm::EXEC);
    assert_eq!(p, Perm::RW);
    p.toggle(Perm::READ);
    assert_eq!((p, p.bits()), (Perm::WRITE, 2));
    assert_eq!(Perm::all() - Perm::WRITE, Perm::READ | Perm::EXEC);
    // Three ways to read untrusted bits.
    assert_eq!(
        Perm::from_bits(0b1001),
        None,
        "strict: unknown bit rejected"
    );
    assert_eq!(Perm::from_bits_truncate(0b1001), Perm::READ);
    let kept = Perm::from_bits_retain(0b1001);
    assert_eq!(
        (kept.bits(), kept.contains(Perm::READ)),
        (0b1001, true),
        "retain unknown bits for round trips"
    );
    assert_eq!(
        (Perm::READ | Perm::EXEC)
            .iter_names()
            .map(|(n, _)| n)
            .collect::<Vec<_>>(),
        ["READ", "EXEC"]
    );
    let mut s = String::new();
    bitflags::parser::to_writer(&(Perm::READ | Perm::WRITE), &mut s).unwrap();
    assert_eq!(
        bitflags::parser::from_str::<Perm>(&s).unwrap(),
        Perm::RW,
        "{s}"
    );
    assert!(bitflags::parser::from_str::<Perm>("READ | NOPE").is_err());
    assert_eq!(size_of::<Perm>(), 1);
}

fn indexmap_positional_editing() {
    let mut m: IndexMap<u32, &str> = [(10, "a"), (30, "c"), (40, "d")].into_iter().collect();
    // Keep a map sorted while inserting. O(n) per call: for bulk loads, extend then sort_keys once.
    assert_eq!(m.insert_sorted(20, "b"), (1, None));
    assert_eq!(m.keys().copied().collect::<Vec<_>>(), [10, 20, 30, 40]);
    m.shift_insert(0, 99, "z");
    assert_eq!(m.get_index(0), Some((&99, &"z")));
    m.swap_indices(0, 4);
    assert_eq!(m.keys().copied().collect::<Vec<_>>(), [40, 10, 20, 30, 99]);
    m.reverse();
    assert_eq!(m.keys().copied().collect::<Vec<_>>(), [99, 30, 20, 10, 40]);
    // A range is a Slice that you can index and compare, which HashMap has no notion of.
    let middle = m.get_range(1..3).unwrap();
    assert_eq!(middle.keys().copied().collect::<Vec<_>>(), [30, 20]);
    assert!(
        m.get_range(4..9).is_none(),
        "out of bounds is None, not a panic"
    );
    // extract_if removes the matching entries within a range and yields them; both sides keep their order.
    let taken: Vec<u32> = m.extract_if(.., |k, _| *k > 25).map(|(k, _)| k).collect();
    assert_eq!(
        (taken, m.keys().copied().collect::<Vec<_>>()),
        (vec![99, 30, 40], vec![20, 10])
    );
    let drained: Vec<_> = m.drain(..1).collect();
    assert_eq!((drained, m.len()), (vec![(20, "b")], 1));
    // Two mutable values at once, e.g. moving money between accounts.
    let mut accounts: IndexMap<&str, i64> = [("a", 100), ("b", 5)].into_iter().collect();
    let [Some(from), Some(to)] = accounts.get_disjoint_mut(["a", "b"]) else {
        panic!("both exist")
    };
    *from -= 40;
    *to += 40;
    assert_eq!((accounts["a"], accounts["b"]), (60, 45));

    // The same two operations on hashbrown 0.15 (the method is named get_many_mut in this version).
    let mut h: hashbrown::HashMap<u8, u8> = (0..10).map(|k| (k, k)).collect();
    let [Some(x), Some(y)] = h.get_many_mut([&1, &2]) else {
        panic!()
    };
    std::mem::swap(x, y);
    assert_eq!((h[&1], h[&2]), (2, 1));
    let mut even: Vec<u8> = h.extract_if(|k, _| k % 2 == 0).map(|(k, _)| k).collect();
    even.sort_unstable(); // a hash map has no order of its own
    assert_eq!((even, h.len()), (vec![0, 2, 4, 6, 8], 5));

    // SmallVec has the Vec editing surface, inline or spilled.
    let mut v: SmallVec<[u8; 8]> = smallvec![1, 2, 3, 4];
    v.extend_from_slice(&[5, 6]);
    v.insert(0, 0);
    v.retain(|x| *x % 2 == 0);
    assert_eq!(&v[..], [0, 2, 4, 6]);
    assert_eq!(v.drain(1..3).collect::<Vec<_>>(), [2, 4]);
    v.extend(0..20);
    assert!(v.spilled());
    v.truncate(2);
    assert!(
        v.spilled(),
        "shrinking the length does not move back inline"
    );
    v.shrink_to_fit();
    assert!(!v.spilled(), "shrink_to_fit does");
}

/// Scenario: "which jobs can run on this machine?" and other questions about sets of small dense ids.
fn bit_sets() {
    use bit_set::BitSet;
    let ids = |s: &BitSet| s.iter().collect::<Vec<usize>>();
    let a: BitSet = [1, 3, 5, 64, 200].into_iter().collect();
    let b: BitSet = [3, 4, 5].into_iter().collect();
    // Lazy set operations return iterators in increasing order; *_with versions modify in place.
    assert_eq!(a.intersection(&b).collect::<Vec<_>>(), [3, 5]);
    assert_eq!(a.union(&b).collect::<Vec<_>>(), [1, 3, 4, 5, 64, 200]);
    assert_eq!(a.difference(&b).collect::<Vec<_>>(), [1, 64, 200]);
    assert_eq!(
        a.symmetric_difference(&b).collect::<Vec<_>>(),
        [1, 4, 64, 200]
    );
    let mut c = b.clone();
    c.union_with(&a); // grows as needed
    assert_eq!(ids(&c), [1, 3, 4, 5, 64, 200]);
    c.intersect_with(&b);
    assert_eq!(c, b, "equality is about members, not about capacity");
    assert!(b.is_subset(&a.union(&b).collect()) && !b.is_subset(&a) && !a.is_disjoint(&b));
    let mut s = BitSet::new();
    assert!(
        s.insert(7) && !s.insert(7),
        "insert says whether it was new"
    );
    assert!(s.remove(7) && !s.remove(7));
    // PITFALL: one huge element allocates for everything below it, and removing it does not give the memory back.
    s.insert(1_000_000);
    s.remove(1_000_000);
    assert_eq!((s.count(), s.is_empty()), (0, true));
    assert!(s.get_ref().capacity() > 1_000_000);
    s.make_empty();

    // Differential test against std's BTreeSet.
    use std::collections::BTreeSet;
    let mut r = Rng(0xB175E7);
    for _ in 0..300 {
        let mut gen_set = || {
            (0..r.below(40))
                .map(|_| r.below(300) as usize)
                .collect::<BTreeSet<_>>()
        };
        let (x, y) = (gen_set(), gen_set());
        let (mut bx, by): (BitSet, BitSet) =
            (x.iter().copied().collect(), y.iter().copied().collect());
        assert!(bx.intersection(&by).eq(x.intersection(&y).copied()));
        assert!(bx.union(&by).eq(x.union(&y).copied()));
        assert_eq!(bx.is_subset(&by), x.is_subset(&y));
        bx.difference_with(&by);
        assert!(bx.iter().eq(x.difference(&y).copied()));
        assert_eq!(bx.count(), x.difference(&y).count());
    }
}

fn bit_vectors() {
    use bitvec::prelude::*;
    // The same byte, numbered from either end.
    let byte = [0b1000_0001u8, 0b0100_0000];
    let msb = byte.view_bits::<Msb0>(); // network order: bit 0 is the TOP bit of byte 0
    let lsb = byte.view_bits::<Lsb0>(); // bit 0 is the BOTTOM bit of byte 0
    assert_eq!(msb.iter_ones().collect::<Vec<_>>(), [0, 7, 9]);
    assert_eq!(lsb.iter_ones().collect::<Vec<_>>(), [0, 7, 14]);

    // Scenario: parse the first two bytes of an IPv4 header (RFC 791): version:4, ihl:4, dscp:6, ecn:2.
    let header = [0x45u8, 0xB9];
    let (version_ihl, tos) = header.view_bits::<Msb0>().split_at(8);
    let version: u8 = version_ihl[..4].load_be();
    let ihl: u8 = version_ihl[4..].load_be();
    let (dscp, ecn): (u8, u8) = (tos[..6].load_be(), tos[6..].load_be());
    assert_eq!((version, ihl, dscp, ecn), (4, 5, 46, 1));
    // Writing a field that straddles a byte boundary, without hand-written shifts and masks.
    let mut out = [0u8; 2];
    out.view_bits_mut::<Msb0>()[4..16].store_be(0xABCu16);
    assert_eq!(out, [0x0A, 0xBC]);
    // load_le over an Lsb0 view of the same bytes is ordinary little-endian integer layout.
    let n: u16 = [0x34u8, 0x12].view_bits::<Lsb0>().load_le();
    assert_eq!(n, 0x1234);

    // Scenario: a free-slot bitmap, the core of slab and page allocators.
    let mut used: BitVec = bitvec![0; 10];
    let alloc = |used: &mut BitVec| {
        let slot = used.first_zero()?;
        used.set(slot, true);
        Some(slot)
    };
    let got: Vec<usize> = std::iter::from_fn(|| alloc(&mut used)).collect();
    assert_eq!(got, (0..10).collect::<Vec<_>>(), "None once full");
    used.set(3, false);
    used.set(7, false);
    assert_eq!((used.count_ones(), used.count_zeros()), (8, 2));
    assert_eq!(alloc(&mut used), Some(3));
    used[4..8].fill(false); // bulk operations work on any sub-slice, aligned or not
    assert_eq!(used.iter_zeros().collect::<Vec<_>>(), [4, 5, 6, 7]);
    used.push(true);
    assert_eq!(used.len(), 11, "it also grows like a Vec");
    assert!(used.get(11).is_none());

    // Fixed size, no heap: BitArray. `bits!` and `bitarr!` build literals.
    let mut fixed = bitarr![u8, Msb0; 0; 8];
    fixed.set(1, true);
    assert_eq!(fixed.into_inner(), [0b0100_0000]);
    assert_eq!(bits![1, 0, 1].count_ones(), 2);
    // Memory: one bit per flag instead of one byte.
    let packed: BitVec<u64> = BitVec::repeat(false, 64_000);
    assert_eq!(packed.as_raw_slice().len() * size_of::<u64>(), 8_000);
    assert_eq!(vec![false; 64_000].len() * size_of::<bool>(), 64_000);
}

// A flags type that must accept bits it does not know about (kernel or protocol flags that grow over time).
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct WireFlags: u8 {
        const ACK = 0b0000_0001;
        /// Declares that ALL bits may be meaningful, defined by the other side.
        const _ = !0;
    }
}
fn externally_defined_flags() {
    assert_eq!(
        WireFlags::from_bits(0b1001).map(|f| f.bits()),
        Some(0b1001),
        "strict parsing now accepts unknown bits"
    );
    assert_eq!(
        WireFlags::from_bits_truncate(0b1001).bits(),
        0b1001,
        "nothing is truncated away"
    );
    // And complement/all cover every bit, not only the named ones.
    assert_eq!((!WireFlags::ACK).bits(), 0b1111_1110);
    assert_eq!(
        (!Perm::READ).bits(),
        0b110,
        "without `_`, `!` stays inside the declared bits"
    );
    assert_eq!(
        WireFlags::from_bits_retain(0b11)
            .iter_names()
            .map(|(n, _)| n)
            .collect::<Vec<_>>(),
        ["ACK"]
    );
}

pub fn main() {
    indexmap_order_and_indices();
    indexmap_positional_editing();
    inline_vectors();
    hashbrown_directly();
    hashers();
    flags();
    externally_defined_flags();
    bit_sets();
    bit_vectors();
    println!("collections: all sections passed");
}
