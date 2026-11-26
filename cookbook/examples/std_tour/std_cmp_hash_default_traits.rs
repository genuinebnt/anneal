//! RECIPE 1007 (std): PartialEq/Eq/PartialOrd/Ord, Ordering, Reverse, sorting by key, Hash/Hasher/BuildHasher, Default.
//! LEVEL: intermediate
//! Use in: sort orders for API results, priority queues, map keys, case-insensitive keys, config structs with defaults.   Run: cargo run --release --example std_cmp_hash_default_traits
//! COMMON USES: ORDER BY with several columns, newest-first listings, BinaryHeap schedulers, dedupe keys,
//! HashMap with a deterministic or faster hasher, `..Default::default()` option structs.
//! METHODS: (everyday first) Ord::cmp, PartialOrd::partial_cmp, PartialEq::eq, Ordering::then, Ordering::then_with, Ordering::reverse, std::cmp::Reverse,
//! METHODS: slice::sort, slice::sort_by, slice::sort_by_key, slice::sort_by_cached_key, slice::sort_unstable, slice::sort_unstable_by,
//! METHODS: slice::sort_unstable_by_key, slice::is_sorted, slice::is_sorted_by_key, Ord::max, Ord::min, Ord::clamp, std::cmp::max, std::cmp::min,
//! METHODS: std::cmp::max_by, std::cmp::min_by, std::cmp::max_by_key, std::cmp::min_by_key, Iterator::max, Iterator::min, Iterator::max_by,
//! METHODS: Iterator::min_by, Iterator::max_by_key, Iterator::min_by_key, f64::total_cmp, f64::max, f64::min, f64::clamp,
//! METHODS: Ordering::is_eq, Ordering::is_ne, Ordering::is_lt, Ordering::is_le, Ordering::is_gt, Ordering::is_ge, Iterator::cmp, Iterator::eq,
//! METHODS: Iterator::partial_cmp, Hash::hash, Hash::hash_slice, Hasher::finish, Hasher::write, BuildHasher::hash_one, BuildHasher::build_hasher,
//! METHODS: std::hash::DefaultHasher, std::hash::RandomState, std::hash::BuildHasherDefault, HashMap::with_hasher, HashMap::with_capacity_and_hasher,
//! METHODS: Default::default, std::mem::take
//! DOCS: https://doc.rust-lang.org/std/cmp/index.html
//! DEFINED IN: core (`core::cmp`, `core::hash` traits, `core::default`, f64::total_cmp). DefaultHasher/RandomState are std (`std::hash`),
//! HashMap is std, the stable slice sorts `sort`/`sort_by*` are alloc (they allocate), `sort_unstable*` are core.
//! See also https://doc.rust-lang.org/std/hash/index.html
//! NOT COVERED (and why): cmp::minmax* and Ordering::then_ignore-style helpers (unstable), PartialOrd::lt/le/gt/ge (these are the
//! `< <= > >=` operators), AssertParamIsEq (internal), SipHasher (deprecated), the per-integer Hasher::write_u8.. (provided methods
//! that forward to write by default).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   sort by one cheap key                  -> sort_by_key(|x| x.a)
//!   sort by an expensive/allocating key    -> sort_by_cached_key (key computed at most once per element)
//!   sort by a borrowed field (String)      -> sort_by(|a, b| a.name.cmp(&b.name))   (sort_by_key cannot return a borrow)
//!   several columns                        -> a.x.cmp(&b.x).then_with(|| ...)   or a tuple key
//!   one column descending                  -> Reverse(key) inside the tuple, or .reverse() on the Ordering
//!   don't care about equal-element order   -> sort_unstable* (no allocation)
//!   sort floats                            -> sort_by(f64::total_cmp)
//!   min-heap                               -> BinaryHeap<Reverse<T>>
//!   keep a value inside bounds             -> clamp
//!   custom equality for a map key          -> impl PartialEq + Eq + Hash so that all three agree (or a normalising newtype)
//!   reproducible hash within one build     -> BuildHasherDefault<MyHasher> (never persist DefaultHasher output)
//!   struct with many optional knobs        -> #[derive(Default)] + `..Default::default()`
//!
//! THE LAWS. Eq: reflexive, symmetric, transitive. Ord must agree with Eq and PartialOrd (a.cmp(b) == Equal iff a == b).
//! Hash: k1 == k2 implies hash(k1) == hash(k2). The compiler checks none of this; breaking it is not undefined behaviour,
//! but maps and sorts then give wrong answers, and sorting with a broken total order may panic.
//!
//! Read: https://doc.rust-lang.org/std/cmp/index.html , https://doc.rust-lang.org/std/hash/index.html , https://doc.rust-lang.org/std/default/trait.Default.html
//! TRY NEXT:
//!   - [S] Swap the order of `major` and `minor` in `Version` and predict which assert in section 1 fails first.
//!   - [S] In section 6 make `BadKey` consistent by lowercasing in `hash`; watch the "lost entry" assert flip.
//!   - [M] Add NULLS LAST ordering for `Option<u32>` without a wrapper type, then with a newtype implementing Ord.
//!   - [M] Count key-function calls for sort_by_key vs sort_by_cached_key on 10, 1k and 100k elements. When does caching lose?
//!   - [A] PROJECT: a scheduler queue: BinaryHeap<Reverse<(deadline, seq, job)>> with an injected clock; seq gives FIFO among equal
//!         deadlines (differential-test it against a stable sort_by_key, using std_vec_vecdeque_methods for the ready queue).
use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash, Hasher, RandomState};

const COVERED: &[&str] = &[
    "cmp",
    "partial_cmp",
    "eq",
    "then",
    "then_with",
    "reverse",
    "Reverse",
    "sort",
    "sort_by",
    "sort_by_key",
    "sort_by_cached_key",
    "sort_unstable",
    "sort_unstable_by",
    "sort_unstable_by_key",
    "is_sorted",
    "is_sorted_by_key",
    "max",
    "min",
    "clamp",
    "max_by",
    "min_by",
    "max_by_key",
    "min_by_key",
    "total_cmp",
    "is_eq",
    "is_ne",
    "is_lt",
    "is_le",
    "is_gt",
    "is_ge",
    "hash",
    "hash_slice",
    "finish",
    "write",
    "hash_one",
    "build_hasher",
    "DefaultHasher",
    "RandomState",
    "BuildHasherDefault",
    "with_hasher",
    "with_capacity_and_hasher",
    "default",
    "take",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

// ---- 1. Everyday core: derived ordering ------------------------------------------------------------------------
/// derive(PartialOrd, Ord) compares fields top to bottom, in DECLARATION order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
struct Version {
    major: u16,
    minor: u16,
    patch: u16,
}
/// For enums, variants compare by discriminant (declaration order unless set explicitly); then by fields.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Low,
    Normal,
    High(u8),
    Critical,
}

fn section_derived() {
    let v = |major, minor, patch| Version {
        major,
        minor,
        patch,
    };
    assert!(v(1, 9, 9) < v(2, 0, 0));
    assert!(v(1, 10, 0) > v(1, 9, 0)); // numeric, unlike comparing the strings "1.10" and "1.9"
    assert!("1.10" < "1.9");
    assert_eq!(v(1, 2, 3).cmp(&v(1, 2, 3)), Ordering::Equal);
    assert!(Priority::Low < Priority::Normal && Priority::High(0) > Priority::Normal);
    assert!(Priority::High(1) < Priority::High(2) && Priority::High(u8::MAX) < Priority::Critical);
    // Built-in orders worth remembering.
    assert!(None < Some(i32::MIN)); // None sorts first
    assert!(Ok::<i32, i32>(9) < Err(0)); // Ok before Err
    assert!(!false && false < true);
    assert!((1, "b") > (1, "a") && (2, "") > (1, "z")); // tuples: lexicographic
    assert!(&[1, 2][..] < &[1, 2, 0][..] && &[2][..] > &[1, 9][..]); // slices: lexicographic, shorter prefix first
    assert!("Z" < "a" && "é" > "z"); // str order is byte order, NOT dictionary order
    println!("1. derived ordering ok");
}

// ---- 2. Multi-column sorting -----------------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq)]
struct Row {
    team: String,
    score: u32,
    seq: u32,
}
fn rows() -> Vec<Row> {
    [
        ("b", 10, 0),
        ("a", 30, 1),
        ("b", 30, 2),
        ("a", 30, 3),
        ("a", 10, 4),
    ]
    .iter()
    .map(|&(t, score, seq)| Row {
        team: t.into(),
        score,
        seq,
    })
    .collect()
}
fn seqs(r: &[Row]) -> Vec<u32> {
    r.iter().map(|r| r.seq).collect()
}

fn section_sorting() {
    // ORDER BY score DESC, team ASC (then input order, because sort_by is STABLE).
    let mut a = rows();
    a.sort_by(|x, y| y.score.cmp(&x.score).then_with(|| x.team.cmp(&y.team)));
    assert_eq!(seqs(&a), [1, 3, 2, 4, 0]);
    // Same thing with .reverse() instead of swapping the operands.
    let mut b = rows();
    b.sort_by(|x, y| {
        x.score
            .cmp(&y.score)
            .reverse()
            .then_with(|| x.team.cmp(&y.team))
    });
    assert_eq!(a, b);
    // Same with a tuple key and Reverse. The String must be cloned, so cache the key.
    let mut c = rows();
    c.sort_by_cached_key(|r| (Reverse(r.score), r.team.clone()));
    assert_eq!(a, c);

    // then vs then_with: `then` evaluates its argument eagerly even when it is not needed.
    let calls = Cell::new(0);
    let expensive = || {
        calls.set(calls.get() + 1);
        Ordering::Greater
    };
    assert_eq!(Ordering::Less.then(expensive()), Ordering::Less);
    assert_eq!(calls.get(), 1);
    assert_eq!(Ordering::Less.then_with(expensive), Ordering::Less);
    assert_eq!(calls.get(), 1);
    assert_eq!(Ordering::Equal.then_with(expensive), Ordering::Greater);

    // is_* turn an Ordering into a bool.
    let o = 1.cmp(&2);
    assert!(o.is_lt() && o.is_le() && o.is_ne() && !o.is_eq() && !o.is_gt() && !o.is_ge());
    assert_eq!(o.reverse(), Ordering::Greater);

    // sort_by_key calls the key function repeatedly; sort_by_cached_key calls it at most once per element.
    let count = Cell::new(0usize);
    let key = |x: &u32| {
        count.set(count.get() + 1);
        x.to_string()
    };
    let mut v: Vec<u32> = (0..200).rev().collect();
    v.sort_by_key(key);
    let plain = count.replace(0);
    let mut w: Vec<u32> = (0..200).rev().collect();
    w.sort_by_cached_key(key);
    assert_eq!(v, w);
    assert!(
        count.get() <= 200 && plain > count.get(),
        "plain={plain} cached={}",
        count.get()
    );

    // Unstable sorts: no allocation, but equal elements may be reordered. Give a total key if output must be deterministic.
    let mut u = rows();
    u.sort_unstable_by_key(|r| (r.score, r.seq));
    assert_eq!(seqs(&u), [0, 4, 1, 2, 3]);
    u.sort_unstable_by(|x, y| y.seq.cmp(&x.seq));
    assert_eq!(seqs(&u), [4, 3, 2, 1, 0]);
    assert!(u.is_sorted_by_key(|r| Reverse(r.seq))); // since 1.82
    let mut n = [3, 1, 2];
    assert!(!n.is_sorted());
    n.sort_unstable();
    assert!(n.is_sorted());
    n.sort();

    // Iterator comparison is lexicographic.
    assert_eq!([1, 2].iter().cmp([1, 3].iter()), Ordering::Less);
    assert!([1, 2].iter().eq([1, 2].iter()));
    assert_eq!([1.0].iter().partial_cmp([f64::NAN].iter()), None);
    println!("2. multi-column sorting ok");
}

// ---- 3. Min-heaps with Reverse ---------------------------------------------------------------------------------
fn section_heap() {
    // BinaryHeap is a MAX-heap. Reverse flips the order, giving "earliest deadline first"; seq breaks ties FIFO.
    let mut q = BinaryHeap::new();
    for (seq, (deadline, name)) in [(30, "c"), (10, "a"), (30, "d"), (20, "b")]
        .into_iter()
        .enumerate()
    {
        q.push(Reverse((deadline, seq, name)));
    }
    let order: String = std::iter::from_fn(|| q.pop().map(|Reverse((_, _, n))| n)).collect();
    assert_eq!(order, "abcd");
    // Reverse in BTreeMap keys: newest first iteration.
    let m: BTreeMap<Reverse<u32>, &str> = [(Reverse(1), "old"), (Reverse(9), "new")].into();
    assert_eq!(m.values().next(), Some(&"new"));
    println!("3. Reverse and heaps ok");
}

// ---- 4. max/min and who wins a tie -----------------------------------------------------------------------------
#[derive(Debug, Clone, Copy)]
struct Tagged(u8, &'static str); // compares by .0 only
impl PartialEq for Tagged {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}
impl Eq for Tagged {}
impl PartialOrd for Tagged {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o)) // always delegate, so the two can never disagree (clippy::non_canonical_partial_ord_impl)
    }
}
impl Ord for Tagged {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.cmp(&o.0)
    }
}

fn section_min_max() {
    let (a, b) = (Tagged(1, "first"), Tagged(1, "second"));
    // Documented tie rules: max returns the SECOND/LAST, min returns the FIRST.
    assert_eq!((a.max(b).1, a.min(b).1), ("second", "first"));
    assert_eq!(
        (std::cmp::max(a, b).1, std::cmp::min(a, b).1),
        ("second", "first")
    );
    assert_eq!(std::cmp::max_by(a, b, |x, y| x.0.cmp(&y.0)).1, "second");
    assert_eq!(std::cmp::min_by(a, b, |x, y| x.0.cmp(&y.0)).1, "first");
    assert_eq!(std::cmp::max_by_key(a, b, |x| x.0).1, "second");
    assert_eq!(std::cmp::min_by_key(a, b, |x| x.0).1, "first");
    let all = [a, b];
    assert_eq!(
        (all.iter().max().unwrap().1, all.iter().min().unwrap().1),
        ("second", "first")
    );
    assert_eq!(all.iter().max_by_key(|t| t.0).unwrap().1, "second");
    assert_eq!(all.iter().min_by_key(|t| t.0).unwrap().1, "first");
    assert_eq!(all.iter().max_by(|x, y| x.cmp(y)).unwrap().1, "second");
    assert_eq!(all.iter().min_by(|x, y| x.cmp(y)).unwrap().1, "first");
    // So "first of the maxima" needs a Reverse trick.
    let first_max = all.iter().min_by_key(|t| Reverse(t.0)).unwrap();
    assert_eq!(first_max.1, "first");
    assert_eq!(Vec::<u8>::new().iter().max(), None);

    // clamp: panics if min > max.
    assert_eq!(
        (15.clamp(0, 10), (-3).clamp(0, 10), 5.clamp(0, 10)),
        (10, 0, 5)
    );
    assert!(panics(|| {
        let _ = 1.clamp(std::hint::black_box(2), 0);
    }));
    println!("4. min/max tie rules and clamp ok");
}

// ---- 5. Floats: PartialOrd only --------------------------------------------------------------------------------
fn section_floats() {
    let nan = f64::NAN;
    #[allow(clippy::eq_op)]
    {
        assert!(nan != nan);
    }
    assert_eq!(1.0_f64.partial_cmp(&nan), None);
    assert!(!(nan < 1.0) && !(nan >= 1.0));
    // f64 is not Ord, so .sort() and .max() do not compile. partial_cmp().unwrap() panics on the first NaN it compares.
    assert!(panics(|| {
        let mut v = [1.0, f64::NAN, 0.0];
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }));
    // total_cmp (since 1.62): IEEE 754 totalOrder: -NaN < -inf < .. < -0.0 < +0.0 < .. < +inf < +NaN.
    let mut v = [2.0, f64::NAN, -0.0, 0.0, f64::NEG_INFINITY];
    v.sort_by(f64::total_cmp);
    assert_eq!(v[0], f64::NEG_INFINITY);
    assert!(v[1].is_sign_negative() && v[1] == 0.0 && v[2].is_sign_positive());
    assert!(v[3] == 2.0 && v[4].is_nan());
    // total_cmp distinguishes -0.0 from 0.0 even though == says they are equal.
    assert!((-0.0f64) == 0.0 && (-0.0f64).total_cmp(&0.0).is_lt());
    // f64::max/min IGNORE a single NaN (unlike the comparison operators).
    assert_eq!(
        (nan.max(1.0), 1.0f64.max(nan), nan.min(1.0)),
        (1.0, 1.0, 1.0)
    );
    // f64::clamp keeps NaN, and panics if a bound is NaN or min > max.
    assert!(nan.clamp(0.0, 1.0).is_nan());
    assert!(panics(|| {
        let _ = 0.5f64.clamp(std::hint::black_box(f64::NAN), 1.0);
    }));
    let best = [0.3, 0.9, 0.1].into_iter().max_by(f64::total_cmp);
    assert_eq!(best, Some(0.9));
    println!("5. float ordering ok");
}

// ---- 6. Hash must agree with Eq --------------------------------------------------------------------------------
/// Case-insensitive ASCII key: eq and hash look at the SAME normalised bytes.
#[derive(Debug, Clone)]
struct HeaderName(String);
impl PartialEq for HeaderName {
    fn eq(&self, o: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&o.0)
    }
}
impl Eq for HeaderName {}
impl Hash for HeaderName {
    fn hash<H: Hasher>(&self, h: &mut H) {
        for b in self.0.bytes() {
            h.write_u8(b.to_ascii_lowercase());
        }
        h.write_u8(0xff); // terminator keeps the encoding prefix-free, so ("ab","c") differs from ("a","bc")
    }
}
/// BROKEN on purpose: equality ignores case but the hash does not.
#[derive(Debug)]
struct BadKey(&'static str);
impl PartialEq for BadKey {
    fn eq(&self, o: &Self) -> bool {
        self.0.eq_ignore_ascii_case(o.0)
    }
}
impl Eq for BadKey {}
impl Hash for BadKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.hash(h);
    }
}

fn section_hash_eq() {
    let mut headers = HashMap::new();
    headers.insert(HeaderName("Content-Type".into()), "json");
    assert_eq!(
        headers.get(&HeaderName("content-type".into())),
        Some(&"json")
    );
    headers.insert(HeaderName("CONTENT-TYPE".into()), "text");
    assert_eq!(headers.len(), 1);

    // The failing example: the two keys are ==, yet their hashes differ, so lookup misses.
    // A fixed hasher makes the demonstration deterministic.
    let state: BuildHasherDefault<DefaultHasher> = BuildHasherDefault::default();
    assert!(BadKey("A") == BadKey("a"));
    assert_ne!(state.hash_one(BadKey("A")), state.hash_one(BadKey("a")));
    let mut bad = HashSet::with_hasher(state);
    bad.insert(BadKey("Key"));
    assert!(
        !bad.contains(&BadKey("KEY")),
        "equal key not found: the law is broken"
    );

    // Check the law directly on random inputs (cheap property test for any custom Hash).
    let rs = RandomState::new();
    let mut x = 7u64;
    for _ in 0..2000 {
        let mut word = || {
            (0..3)
                .map(|_| {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                    ['a', 'A', 'b', 'B'][(x >> 33) as usize % 4]
                })
                .collect::<String>()
        };
        let (p, q) = (HeaderName(word()), HeaderName(word()));
        if p == q {
            assert_eq!(rs.hash_one(&p), rs.hash_one(&q));
        }
    }
    println!("6. Hash/Eq consistency ok");
}

// ---- 7. Hasher, BuildHasher, custom hashers --------------------------------------------------------------------
/// FNV-1a. Fast for tiny keys, NOT resistant to hash flooding: never use it on attacker-chosen keys.
#[derive(Clone)]
struct Fnv(u64);
impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}
impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
type FnvMap<K, V> = HashMap<K, V, BuildHasherDefault<Fnv>>;

fn section_hashers() {
    // Manual use of a Hasher.
    let mut h = Fnv::default();
    h.write(b"a");
    assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c); // published FNV-1a 64 value for "a"
    // finish() does not reset the state: further writes continue the stream.
    let before = h.finish();
    assert_eq!(h.finish(), before);
    h.write(b"b");
    assert_ne!(h.finish(), before);

    // hash_one (since 1.71) = build_hasher + hash + finish.
    let build = BuildHasherDefault::<DefaultHasher>::default();
    let mut manual = build.build_hasher();
    "abc".hash(&mut manual);
    assert_eq!(manual.finish(), build.hash_one("abc"));
    // DefaultHasher::new() instances agree with each other in one binary. The algorithm is unspecified:
    // never persist these values or compare them across Rust releases.
    let once = |s: &str| {
        let mut h = DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    };
    assert_eq!(once("k"), once("k"));
    // RandomState: randomly keyed per instance (HashDoS resistance), so iteration order varies between maps and runs.
    let r = RandomState::new();
    assert_eq!(r.hash_one(1u8), r.clone().hash_one(1u8));
    assert_ne!(
        RandomState::new().hash_one(12345u64),
        RandomState::new().hash_one(12345u64)
    );
    // Hash::hash_slice hashes a run of items; unlike [T]::hash it writes NO length prefix.
    let mut s1 = Fnv::default();
    u8::hash_slice(&[1, 2, 3], &mut s1);
    let mut s2 = Fnv::default();
    s2.write(&[1, 2, 3]);
    assert_eq!(s1.finish(), s2.finish());
    // Borrow works because String and str hash identically.
    assert_eq!(build.hash_one(String::from("x")), build.hash_one("x"));

    let mut m: FnvMap<&str, u32> = FnvMap::default();
    m.insert("a", 1);
    let mut m2 = HashMap::with_capacity_and_hasher(16, BuildHasherDefault::<Fnv>::default());
    m2.insert("a", 1);
    assert_eq!(m, m2);
    let m3: HashMap<u8, u8, _> = HashMap::with_hasher(RandomState::new());
    assert!(m3.is_empty());
    // Deterministic OUTPUT should not depend on the hasher: sort, or use BTreeMap.
    let ordered: BTreeMap<_, _> = m.iter().collect();
    assert_eq!(ordered.len(), 1);
    println!("7. hashers ok");
}

// ---- 8. Default ------------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Default)]
enum Mode {
    #[default]
    Safe,
    Fast,
}
#[derive(Debug, Default, PartialEq)]
struct Stats {
    hits: u64,
    errors: Vec<String>,
    mode: Mode,
    label: Option<String>,
}
/// Manual Default when the zero values are wrong.
#[derive(Debug, PartialEq)]
struct PoolConfig {
    max_conns: u32,
    timeout_ms: u64,
    tls: bool,
}
impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            max_conns: 10,
            timeout_ms: 5_000,
            tls: true,
        }
    }
}

fn section_default() {
    let s = Stats::default();
    assert_eq!(
        (s.hits, s.errors.len(), &s.mode, &s.label),
        (0, 0, &Mode::Safe, &None)
    );
    // Struct update syntax: override a few fields. The base expression is evaluated even if you only keep part of it.
    let c = PoolConfig {
        max_conns: 50,
        ..Default::default()
    };
    assert_eq!(
        c,
        PoolConfig {
            max_conns: 50,
            timeout_ms: 5_000,
            tls: true
        }
    );
    // mem::take: move the value out behind &mut and leave Default::default() in its place.
    let mut st = Stats {
        errors: vec!["e1".into()],
        ..Stats::default()
    };
    let drained = std::mem::take(&mut st.errors);
    assert_eq!((drained.len(), st.errors.len()), (1, 0));
    assert_eq!(<(u8, String, bool)>::default(), (0, String::new(), false));
    assert_eq!(<[u8; 3]>::default(), [0; 3]);
    assert_eq!(None::<u8>.unwrap_or_default(), 0);
    println!("8. Default ok");
}

// ---- 9. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Derive PartialEq, Eq, PartialOrd, Ord, Hash TOGETHER; hand-write all of them or none
    //    (clippy::derived_hash_with_manual_eq, derive_ord_xor_partial_ord).
    // 2. Write partial_cmp as Some(self.cmp(other)) (clippy::non_canonical_partial_ord_impl).
    // 3. Reordering struct fields silently changes a derived sort order: add a test that pins it.
    assert!(
        Version {
            major: 1,
            ..Default::default()
        } > Version {
            minor: 99,
            patch: 99,
            ..Default::default()
        }
    );
    // 4. Paginated APIs need a TOTAL order: end the key with a unique id, or rows jump between pages.
    let mut page = [(5, 2u64), (5, 1), (4, 3)];
    page.sort_unstable_by_key(|&(score, id)| (Reverse(score), id));
    assert_eq!(page, [(5, 1), (5, 2), (4, 3)]);
    // 5. Never mutate a key's hashed or compared fields while it is in a map or set (possible via Cell/RefCell).
    // 6. Keep RandomState for keys that come from users; switch hashers only with a benchmark and trusted keys.
    // 7. Do not iterate a HashMap into user-visible output or golden tests: sort first.
    let h: HashMap<u8, u8> = [(3, 0), (1, 0), (2, 0)].into();
    let mut keys: Vec<_> = h.keys().copied().collect();
    keys.sort_unstable();
    assert_eq!(keys, [1, 2, 3]);
    // 8. sort_by_key with `|x| x.name.clone()` allocates on every comparison (clippy::unnecessary_sort_by is the
    //    opposite hint); use sort_by with borrows, or sort_by_cached_key.
    let mut names = vec![String::from("b"), String::from("a")];
    names.sort_by(|a, b| a.cmp(b));
    assert_eq!(names, ["a", "b"]);
    // 9. Floats in keys: store an integer (micros, cents) so the type gets Ord + Hash for free.
    // 10. Default should be SAFE: if "all zeros" means "no timeout" or "TLS off", write Default by hand.
    assert!(PoolConfig::default().tls);
    println!("9. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_derived();
    section_sorting();
    section_heap();
    section_min_max();
    section_floats();
    section_hash_eq();
    section_hashers();
    section_default();
    section_production_tips();
    println!(
        "covered {} cmp/hash/default names; all asserts passed",
        COVERED.len()
    );
}
