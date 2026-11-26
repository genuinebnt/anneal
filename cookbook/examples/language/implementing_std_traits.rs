//! RECIPE 450 (language): implementing the std traits for your own collection and value types
//! LEVEL: intermediate
//! Use in: library types, newtypes, domain values, small collections.   Run: cargo run --release --example implementing_std_traits
#![allow(dead_code)]
//!
//! WHAT: a tour of the std traits you implement by hand (not derive), each with a law or consistency test:
//! collections (`FromIterator`, `Extend`, three `IntoIterator` impls, `Index`, `Deref`), keys (`Eq`/`Hash`/`Borrow`),
//! ordering (`Ord` and floats), formatting (`Display` flags, `Debug` builders), conversions (`From`/`TryFrom`/`FromStr`),
//! lifecycle (`Drop`, `Clone`/`Copy`), iterators (`size_hint`, `DoubleEndedIterator`, `ExactSizeIterator`),
//! errors (`source`), operators (`Add`/`Sum`/`Product`), and I/O (`Read`/`Write`).
//! WHY: users of your type expect it to compose with `collect()`, `for` loops, `HashMap`, `sort`, `{:>8}` and `?`.
//! Each trait carries a *law* the compiler cannot check; breaking it gives wrong answers, not compile errors.
//! HOW: every section prints a line and asserts the law, including seeded random tests (xorshift, no deps).
//!
//! COMMON USES: sorted/unique collections, case-insensitive map keys (HTTP headers, identifiers), numeric newtypes
//! (money, units), colour/version/id values with a text form, in-memory buffers for tests, error chains in CLIs.
//!
//! Read: The Rust API Guidelines, Interoperability https://rust-lang.github.io/api-guidelines/interoperability.html
//! Read: std::ops::Index https://doc.rust-lang.org/std/ops/trait.Index.html ; The Rust Book ch.15 (Deref, Drop)
//! Read: std::borrow::Borrow https://doc.rust-lang.org/std/borrow/trait.Borrow.html (the Hash/Eq contract)
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::iter::{Product, Sum};
use std::num::ParseIntError;
use std::ops::{Add, Deref, DerefMut, Index, IndexMut, Mul};
use std::rc::Rc;
use std::str::FromStr;

/// Tiny seeded xorshift64 generator so the random tests are reproducible without dependencies.
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

// ---------------------------------------------------------------------------------------------------------------
// 1. Collections: SortedVec keeps an invariant (always sorted), Bag has none.
// ---------------------------------------------------------------------------------------------------------------

/// A Vec that is always sorted. The invariant is why it must NOT implement `DerefMut` or `IntoIterator for &mut`:
/// handing out `&mut [T]` would let callers break the order and make `binary_search` lie.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct SortedVec<T>(Vec<T>);

impl<T: Ord> SortedVec<T> {
    fn new() -> Self {
        SortedVec(Vec::new())
    }
    fn insert(&mut self, x: T) {
        // partition_point is a binary search; inserting after equal elements keeps insertion order stable
        let i = self.0.partition_point(|y| *y <= x);
        self.0.insert(i, x);
    }
}

// FromIterator is what makes `.collect::<SortedVec<_>>()` work. Re-establish the invariant once, at the end.
impl<T: Ord> FromIterator<T> for SortedVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v: Vec<T> = iter.into_iter().collect();
        v.sort(); // stable; O(n log n) once is cheaper than n inserts
        SortedVec(v)
    }
}
// Extend: same idea. std also offers `Extend<&'a T>` for Copy types, so `v.extend(&other)` works; mirror that.
impl<T: Ord> Extend<T> for SortedVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter);
        self.0.sort(); // timsort is close to O(n) on a sorted prefix plus a short tail
    }
}
impl<'a, T: Ord + Copy + 'a> Extend<&'a T> for SortedVec<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}
// Owned iteration: name the std iterator type so callers can store it. (`impl Trait` in assoc types is unstable.)
impl<T> IntoIterator for SortedVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
// Shared iteration: `for x in &sorted`.
impl<'a, T> IntoIterator for &'a SortedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
// Read-only Deref to a slice: gives len, first, iter, binary_search, windows ... for free. Appropriate here because a
// SortedVec "is a sorted slice" for all read purposes. NOT appropriate as general newtype inheritance: Deref is for
// smart pointers / owning wrappers of a borrowed form (Vec -> [T], String -> str), and method resolution through it is
// invisible to readers. There is deliberately no DerefMut and no `&mut` IntoIterator.
impl<T> Deref for SortedVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}

/// Unordered bag: no invariant, so the full trio of `IntoIterator` impls plus `DerefMut` is fine.
#[derive(Clone, PartialEq, Eq, Default)]
struct Bag<T>(Vec<T>);

impl<T> FromIterator<T> for Bag<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Bag(iter.into_iter().collect())
    }
}
impl<T> Extend<T> for Bag<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}
impl<T> IntoIterator for Bag<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl<'a, T> IntoIterator for &'a Bag<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
// `for x in &mut bag` lets callers modify elements in place (but not add or remove: the iterator borrows the Vec).
impl<'a, T> IntoIterator for &'a mut Bag<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}
impl<T> Deref for Bag<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}
impl<T> DerefMut for Bag<T> {
    // a `&mut [T]` cannot change the length, so no invariant can be broken: DerefMut is safe to offer.
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.0
    }
}
// Custom Debug via debug_list: prints like a list and honours `{:#?}` pretty printing for free.
impl<T: fmt::Debug> fmt::Debug for Bag<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

fn collections_demo() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    // law: collect == new + insert one by one; the result is sorted; extend == collect of the concatenation
    for _ in 0..200 {
        let xs: Vec<i32> = (0..rng.below(40))
            .map(|_| rng.below(50) as i32 - 25)
            .collect();
        let ys: Vec<i32> = (0..rng.below(20))
            .map(|_| rng.below(50) as i32 - 25)
            .collect();
        let collected: SortedVec<i32> = xs.iter().copied().collect();
        let mut inserted = SortedVec::new();
        for &x in &xs {
            inserted.insert(x);
        }
        assert_eq!(collected, inserted, "FromIterator and insert agree");
        assert!(
            collected.windows(2).all(|w| w[0] <= w[1]),
            "sorted invariant (via Deref)"
        );

        let mut extended = collected.clone();
        extended.extend(&ys); // Extend<&T>
        let both: SortedVec<i32> = xs.iter().chain(ys.iter()).copied().collect();
        assert_eq!(extended, both, "extend == collect of concatenation");

        // the three IntoIterator impls must agree on contents and order
        let by_ref: Vec<i32> = (&extended).into_iter().copied().collect();
        let by_deref: Vec<i32> = extended.iter().copied().collect();
        let by_val: Vec<i32> = extended.clone().into_iter().collect();
        assert!(by_ref == by_deref && by_deref == by_val);
    }

    let mut bag: Bag<i32> = (1..=4).collect();
    for x in &mut bag {
        *x *= 10; // &mut IntoIterator
    }
    bag[0] = 7; // IndexMut arrives through DerefMut to [T]
    bag.sort_by(|a, b| b.cmp(a)); // slice methods through DerefMut
    bag.extend([5]);
    assert_eq!(format!("{bag:?}"), "[40, 30, 20, 7, 5]");
    assert_eq!(bag.into_iter().sum::<i32>(), 102);
    println!(
        "1. collections: FromIterator/Extend/IntoIterator x3/Deref verified on 200 random cases"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 2. Index / IndexMut and the panic-vs-get convention
// ---------------------------------------------------------------------------------------------------------------

/// Row-major matrix. Convention (same as slices and HashMap): `m[(r, c)]` PANICS when out of range,
/// `m.get(r, c)` returns `Option` for callers that cannot know the bounds in advance.
#[derive(Debug, Clone, PartialEq)]
struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}
impl Matrix {
    fn zeros(rows: usize, cols: usize) -> Self {
        Matrix {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }
    fn offset(&self, r: usize, c: usize) -> Option<usize> {
        // check each axis: `r * cols + c` alone would let (0, cols) alias (1, 0)
        (r < self.rows && c < self.cols).then(|| r * self.cols + c)
    }
    fn get(&self, r: usize, c: usize) -> Option<&f64> {
        self.offset(r, c).map(|i| &self.data[i])
    }
    fn get_mut(&mut self, r: usize, c: usize) -> Option<&mut f64> {
        self.offset(r, c).map(|i| &mut self.data[i])
    }
}
impl Index<(usize, usize)> for Matrix {
    type Output = f64;
    fn index(&self, (r, c): (usize, usize)) -> &f64 {
        // `Index` must return a reference, so it cannot return Option: the panic is the contract.
        self.get(r, c).unwrap_or_else(|| {
            panic!(
                "index ({r}, {c}) out of range for {}x{} matrix",
                self.rows, self.cols
            )
        })
    }
}
impl IndexMut<(usize, usize)> for Matrix {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut f64 {
        let (rows, cols) = (self.rows, self.cols);
        self.get_mut(r, c)
            .unwrap_or_else(|| panic!("index ({r}, {c}) out of range for {rows}x{cols} matrix"))
    }
}

fn index_demo() {
    let mut m = Matrix::zeros(2, 3);
    m[(1, 2)] = 5.0;
    m[(0, 0)] += 1.5;
    assert_eq!(m[(1, 2)], 5.0);
    assert_eq!(m.get(1, 2), Some(&5.0));
    assert_eq!(m.get(0, 3), None, "column 3 must not alias row 1 col 0");
    assert_eq!(m.get(2, 0), None);
    // verify the panic convention without spamming stderr
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(|| {
        let m = Matrix::zeros(2, 2);
        m[(2, 0)]
    });
    std::panic::set_hook(prev);
    assert!(r.is_err(), "out-of-range Index must panic");
    println!(
        "2. index: m[(1,2)]={} get(0,3)={:?}, out-of-range index panics",
        m[(1, 2)],
        m.get(0, 3)
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 3. Hash / Eq consistency, Borrow, AsRef
// ---------------------------------------------------------------------------------------------------------------

type FixedState = BuildHasherDefault<DefaultHasher>; // deterministic hasher so the "bug" demo cannot flake

/// Case-insensitive key done right: Eq and Hash both ignore ASCII case.
#[derive(Debug, Clone)]
struct Ci(String);
impl PartialEq for Ci {
    fn eq(&self, o: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&o.0)
    }
}
impl Eq for Ci {}
impl Hash for Ci {
    fn hash<H: Hasher>(&self, h: &mut H) {
        // The law: a == b  =>  hash(a) == hash(b). Hash the *normalised* bytes, mirroring `str`'s 0xff terminator
        // so ("ab","c") and ("a","bc") style sequences still differ.
        for b in self.0.bytes() {
            h.write_u8(b.to_ascii_lowercase());
        }
        h.write_u8(0xff);
    }
}

/// BROKEN on purpose: Eq ignores case but Hash looks at the raw bytes. Equal keys land in different buckets.
#[derive(Debug, Clone)]
struct BadCi(String);
impl PartialEq for BadCi {
    fn eq(&self, o: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&o.0)
    }
}
impl Eq for BadCi {}
impl Hash for BadCi {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.hash(h); // BUG: "Key" == "KEY" but their hashes differ
    }
}

/// An owned name whose Hash/Eq are exactly `str`'s (derive does that for a one-field newtype over String), so it
/// may implement `Borrow<str>` and `HashMap<Name, V>::get("literal")` works without allocating a Name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Name(String);
impl std::borrow::Borrow<str> for Name {
    // Borrow's contract: Eq, Ord and Hash of the borrowed form MUST match the owned form. If you cannot promise
    // that (e.g. Ci vs str), do not implement Borrow<str>; AsRef<str> carries no such promise.
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
/// Accept anything string-like without forcing callers to convert: the reason AsRef exists.
fn shout(s: impl AsRef<str>) -> String {
    s.as_ref().to_uppercase()
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

fn hash_demo() {
    let mut rng = Rng(42);
    // random check of the law for the good type: eq => same hash
    for _ in 0..2000 {
        let base: String = (0..rng.below(12))
            .map(|_| (b'a' + rng.below(26) as u8) as char)
            .collect();
        let flipped: String = base
            .chars()
            .map(|c| {
                if rng.below(2) == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect();
        let (a, b) = (Ci(base), Ci(flipped));
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }
    let words: Vec<String> = (0..100).map(|i| format!("Key{i}")).collect();
    let mut good: HashMap<Ci, usize, FixedState> = HashMap::default();
    let mut bad: HashMap<BadCi, usize, FixedState> = HashMap::default();
    for (i, w) in words.iter().enumerate() {
        good.insert(Ci(w.clone()), i);
        bad.insert(BadCi(w.clone()), i);
    }
    let found_good = words
        .iter()
        .filter(|w| good.contains_key(&Ci(w.to_uppercase())))
        .count();
    let found_bad = words
        .iter()
        .filter(|w| bad.contains_key(&BadCi(w.to_uppercase())))
        .count();
    assert_eq!(
        found_good, 100,
        "consistent Hash/Eq: every upper-case lookup succeeds"
    );
    assert!(
        found_bad < 100,
        "inconsistent Hash/Eq: lookups of equal keys fail (found {found_bad}/100)"
    );
    // and the BadCi map even holds "duplicates" of equal keys
    bad.insert(BadCi("KEY1".into()), 999);
    assert!(bad.len() > 100 || bad.get(&BadCi("KEY1".into())).is_some());

    let mut names: HashMap<Name, u32> = HashMap::new();
    names.insert(Name("alice".into()), 1);
    assert_eq!(
        names.get("alice"),
        Some(&1),
        "Borrow<str> lets &str query a map keyed by Name"
    );
    assert_eq!(shout("abc"), "ABC");
    assert_eq!(shout(String::from("x")), "X");
    assert_eq!(shout(Name("n".into())), "N");
    println!(
        "3. hash/eq: good map found {found_good}/100, broken map found only {found_bad}/100 equal keys"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 4. Ordering: why f64 is not Ord, and a total-order wrapper
// ---------------------------------------------------------------------------------------------------------------

/// `f64` is only `PartialOrd` because NaN breaks reflexivity (`NaN != NaN`) and comparability. `[1.0].sort()` does not
/// compile for that reason. This wrapper uses IEEE 754 totalOrder (`total_cmp`): -NaN < -inf < ... < -0 < +0 < ... < inf < +NaN.
#[derive(Debug, Clone, Copy)]
struct Total(f64);
impl PartialEq for Total {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal // derive Eq from Ord so the two can never disagree
    }
}
impl Eq for Total {}
impl PartialOrd for Total {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o)) // the standard pattern: PartialOrd delegates to Ord
    }
}
impl Ord for Total {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}
impl Hash for Total {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.to_bits().hash(h); // total_cmp equal <=> identical bits, so hashing bits is consistent
    }
}

fn ordering_demo() {
    assert_eq!(f64::NAN.partial_cmp(&1.0), None);
    let nan = std::hint::black_box(f64::NAN);
    assert!(nan != nan, "not reflexive, hence not Eq");
    let mut rng = Rng(7);
    // random bit patterns hit NaNs (with payloads), infinities, subnormals and both zeros
    let mut v: Vec<Total> = (0..2000)
        .map(|_| Total(f64::from_bits(rng.next())))
        .collect();
    v.extend([
        Total(0.0),
        Total(-0.0),
        Total(f64::NAN),
        Total(f64::INFINITY),
        Total(f64::NEG_INFINITY),
    ]);
    v.sort();
    assert!(v.windows(2).all(|w| w[0] <= w[1]));
    // sort and binary_search agree: every element is found at an index holding an equal element
    for x in &v {
        let i = v.binary_search(x).expect("present element must be found");
        assert_eq!(v[i], *x);
    }
    // consistency: a < b, a == b, a > b are mutually exclusive and match cmp
    for _ in 0..2000 {
        let a = v[rng.below(v.len() as u64) as usize];
        let b = v[rng.below(v.len() as u64) as usize];
        let flags = [a < b, a == b, a > b];
        assert_eq!(flags.iter().filter(|f| **f).count(), 1);
        assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)));
        assert_eq!(a.cmp(&b), b.cmp(&a).reverse(), "antisymmetry");
    }
    assert!(
        Total(-0.0) < Total(0.0),
        "total order distinguishes signed zeros"
    );
    // without a wrapper, the one-liner for ad hoc sorting:
    let mut raw = vec![3.0, f64::NAN, -1.0];
    raw.sort_by(f64::total_cmp);
    assert_eq!(raw[0], -1.0);
    assert!(raw[2].is_nan());
    println!(
        "4. ordering: {} floats incl. NaN/-0 sorted; binary_search agrees with sort",
        v.len()
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 5. Display honouring formatter flags, custom Debug
// ---------------------------------------------------------------------------------------------------------------

/// Temperature that honours the number-formatting flags like built-in numbers do.
struct Temp(f64);
impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prec = f.precision().unwrap_or(1);
        let digits = format!("{:.*}", prec, self.0.abs());
        // `{:#}` => a "~" prefix meaning approximate. pad_integral does the rest for numeric-like output:
        // width, fill, alignment (default right), `+` sign, and sign-aware `0` padding.
        let prefix = if f.alternate() { "~" } else { "" };
        f.pad_integral(!self.0.is_sign_negative(), prefix, &digits)
    }
}
/// Text-like type: `f.pad` honours width, fill, alignment AND precision (as truncation), like `str`.
struct Tag(&'static str);
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.0)
    }
}
/// A type with a secret: Debug redacts it. `debug_struct` keeps `{:#?}` pretty printing working.
struct Account {
    user: String,
    token: String,
    attempts: u32,
}
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("user", &self.user)
            .field(
                "token",
                &format_args!("<{} chars redacted>", self.token.len()),
            )
            .field("attempts", &self.attempts)
            .finish()
    }
}

fn formatting_demo() {
    let t = Temp(3.14159);
    assert_eq!(format!("{t}"), "3.1");
    assert_eq!(format!("{t:.2}"), "3.14");
    assert_eq!(format!("{t:8.2}|"), "    3.14|"); // numbers right-align by default
    assert_eq!(format!("{t:<8.2}|"), "3.14    |");
    assert_eq!(format!("{t:*^9.2}"), "**3.14***");
    assert_eq!(format!("{t:+.2}"), "+3.14");
    assert_eq!(format!("{t:#.2}"), "~3.14");
    assert_eq!(format!("{t:08.2}"), "00003.14");
    assert_eq!(format!("{:08.2}", Temp(-3.14159)), "-0003.14");
    assert_eq!(format!("{:+.1}", Temp(-2.0)), "-2.0");
    let tag = Tag("hello");
    assert_eq!(format!("{tag:>7}"), "  hello");
    assert_eq!(format!("{tag:-<7}"), "hello--");
    assert_eq!(format!("{tag:.3}"), "hel");
    assert_eq!(format!("{tag:^9.4}"), "  hell   ");
    // a naive `write!(f, "{}", self.0)` ignores all of the above; that is the common bug.
    struct Naive;
    impl fmt::Display for Naive {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "ab")
        }
    }
    assert_eq!(
        format!("{:>6}", Naive),
        "ab",
        "write! ignores width: pad yourself or use f.pad"
    );

    let a = Account {
        user: "ann".into(),
        token: "s3cr3t-token".into(),
        attempts: 2,
    };
    let dbg = format!("{a:?}");
    assert!(!dbg.contains("s3cr3t") && dbg.contains("12 chars redacted"));
    assert!(format!("{a:#?}").contains("\n    attempts: 2,"));
    println!("5. formatting: {t:*^9.2} {t:+.2} {t:#.2} {tag:-<7}| {dbg}");
}

// ---------------------------------------------------------------------------------------------------------------
// 6. Default, From / TryFrom / FromStr round trip, Error::source
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Rgb(u8, u8, u8);

impl Default for Rgb {
    // derive(Default) would give black (0,0,0) too, but writing it out documents that this is a deliberate choice;
    // use a manual impl whenever the "zero" value is not the right default (e.g. port 8080, timeout 30s).
    fn default() -> Self {
        Rgb(0, 0, 0)
    }
}
impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}
#[derive(Debug, PartialEq, Eq)]
enum ParseRgbError {
    MissingHash,
    Length(usize),
    BadDigit { pos: usize, ch: char },
}
impl fmt::Display for ParseRgbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHash => f.write_str("colour must start with '#'"),
            Self::Length(n) => write!(f, "expected 6 hex digits, found {n}"),
            Self::BadDigit { pos, ch } => write!(f, "invalid hex digit {ch:?} at position {pos}"),
        }
    }
}
impl Error for ParseRgbError {}

impl FromStr for Rgb {
    type Err = ParseRgbError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix('#').ok_or(ParseRgbError::MissingHash)?;
        let n = hex.chars().count();
        if n != 6 {
            return Err(ParseRgbError::Length(n));
        }
        // from_str_radix would accept "+1" and similar, so validate digits ourselves for strictness
        if let Some((i, ch)) = hex.char_indices().find(|(_, c)| !c.is_ascii_hexdigit()) {
            return Err(ParseRgbError::BadDigit { pos: i + 1, ch });
        }
        let p = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("validated above");
        Ok(Rgb(p(0), p(2), p(4)))
    }
}
// Infallible conversions are `From` (and give you `Into` for free). Never implement Into directly.
impl From<[u8; 3]> for Rgb {
    fn from([r, g, b]: [u8; 3]) -> Self {
        Rgb(r, g, b)
    }
}
impl From<Rgb> for u32 {
    fn from(c: Rgb) -> u32 {
        (c.0 as u32) << 16 | (c.1 as u32) << 8 | c.2 as u32
    }
}
// Fallible: not every u32 is a colour, so TryFrom with a real error type.
#[derive(Debug, PartialEq, Eq)]
struct OutOfRange(u32);
impl fmt::Display for OutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x} does not fit in 24 bits", self.0)
    }
}
impl Error for OutOfRange {}
impl TryFrom<u32> for Rgb {
    type Error = OutOfRange;
    fn try_from(v: u32) -> Result<Self, OutOfRange> {
        if v > 0xff_ffff {
            return Err(OutOfRange(v));
        }
        Ok(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}
impl TryFrom<&str> for Rgb {
    type Error = ParseRgbError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse() // delegate: one parser, many entry points
    }
}

/// Wrapping lower-level errors: `source()` exposes the cause so reporters can print the whole chain.
#[derive(Debug)]
enum ConfigError {
    BadColour { line: usize, source: ParseRgbError },
    BadWidth { line: usize, source: ParseIntError },
    Shape(usize),
}
impl fmt::Display for ConfigError {
    // convention: Display describes THIS level only; the cause is reached via source(), not repeated here.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadColour { line, .. } => write!(f, "line {line}: bad colour"),
            Self::BadWidth { line, .. } => write!(f, "line {line}: bad width"),
            Self::Shape(line) => write!(f, "line {line}: expected `colour width`"),
        }
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BadColour { source, .. } => Some(source),
            Self::BadWidth { source, .. } => Some(source),
            Self::Shape(_) => None,
        }
    }
}
fn parse_line(line: usize, s: &str) -> Result<(Rgb, u32), ConfigError> {
    let mut it = s.split_whitespace();
    let (Some(c), Some(w), None) = (it.next(), it.next(), it.next()) else {
        return Err(ConfigError::Shape(line));
    };
    let colour = c
        .parse()
        .map_err(|source| ConfigError::BadColour { line, source })?;
    let width = w
        .parse()
        .map_err(|source| ConfigError::BadWidth { line, source })?;
    Ok((colour, width))
}
fn chain(e: &dyn Error) -> Vec<String> {
    let mut out = vec![e.to_string()];
    let mut cur = e.source();
    while let Some(s) = cur {
        out.push(s.to_string());
        cur = s.source();
    }
    out
}

fn conversion_demo() {
    let mut rng = Rng(99);
    for _ in 0..2000 {
        let c = Rgb(rng.next() as u8, rng.next() as u8, rng.next() as u8);
        assert_eq!(c.to_string().parse::<Rgb>(), Ok(c), "parse(format(x)) == x");
        assert_eq!(Rgb::try_from(u32::from(c)), Ok(c), "u32 round trip");
        assert_eq!(Rgb::from([c.0, c.1, c.2]), c);
        assert_eq!(Rgb::try_from(c.to_string().as_str()), Ok(c));
    }
    assert_eq!("ff0000".parse::<Rgb>(), Err(ParseRgbError::MissingHash));
    assert_eq!("#ff00".parse::<Rgb>(), Err(ParseRgbError::Length(4)));
    assert_eq!(
        "#ff00zz".parse::<Rgb>(),
        Err(ParseRgbError::BadDigit { pos: 5, ch: 'z' })
    );
    assert_eq!(
        "#+1+1+1".parse::<Rgb>(),
        Err(ParseRgbError::BadDigit { pos: 1, ch: '+' })
    );
    assert_eq!(Rgb::try_from(0x1_00_00_00), Err(OutOfRange(0x100_0000)));
    assert_eq!(Rgb::default().to_string(), "#000000");

    assert_eq!(parse_line(1, "#00ff00 3").unwrap(), (Rgb(0, 255, 0), 3));
    let e = parse_line(2, "#00ff0 3").unwrap_err();
    assert_eq!(
        chain(&e),
        vec!["line 2: bad colour", "expected 6 hex digits, found 5"]
    );
    let e = parse_line(3, "#00ff00 -4").unwrap_err();
    assert_eq!(chain(&e).len(), 2, "ParseIntError is the source");
    assert!(parse_line(4, "#00ff00").unwrap_err().source().is_none());
    // trait objects: `?` converts into Box<dyn Error> thanks to the Error impl
    fn boxed() -> Result<(), Box<dyn Error>> {
        parse_line(5, "x 1")?;
        Ok(())
    }
    assert!(boxed().is_err());
    println!(
        "6. conversions: 2000 round trips ok; error chain = {:?}",
        chain(&parse_line(2, "#00ff0 3").unwrap_err())
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 7. Drop order, Clone vs Copy
// ---------------------------------------------------------------------------------------------------------------

type Log = Rc<RefCell<Vec<&'static str>>>;
struct Noisy(&'static str, Log);
impl Drop for Noisy {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}
struct Pair {
    first: Noisy,
    second: Noisy,
}
// Rules: Copy requires Clone and every field Copy; a type that implements Drop (or owns heap data) can never be Copy,
// because a bitwise duplicate would run the destructor twice. Clone for Copy types should just be `*self`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}
// let `Clone` be explicit and meaningful when it is not free: here we count how many deep copies happen.
struct Counted {
    payload: Vec<u8>,
    clones: Rc<RefCell<u32>>,
}
impl Clone for Counted {
    fn clone(&self) -> Self {
        *self.clones.borrow_mut() += 1;
        Counted {
            payload: self.payload.clone(),
            clones: Rc::clone(&self.clones),
        }
    }
}

fn drop_demo() {
    let log: Log = Rc::default();
    {
        let _a = Noisy("local-a", log.clone());
        let _b = Noisy("local-b", log.clone());
        let _pair = Pair {
            first: Noisy("field-1", log.clone()),
            second: Noisy("field-2", log.clone()),
        };
        let _v = vec![Noisy("vec-0", log.clone()), Noisy("vec-1", log.clone())];
        let _t = (Noisy("tuple-0", log.clone()), Noisy("tuple-1", log.clone()));
        let moved = Noisy("moved", log.clone());
        drop(moved); // explicit drop runs immediately
        assert_eq!(*log.borrow(), ["moved"]);
        let _ = Noisy("underscore", log.clone()); // `_` pattern drops at once, `_x` lives to scope end
        assert_eq!(*log.borrow(), ["moved", "underscore"]);
    }
    // locals drop in REVERSE declaration order; fields, tuple and Vec elements drop in DECLARATION order
    assert_eq!(
        *log.borrow(),
        [
            "moved",
            "underscore",
            "tuple-0",
            "tuple-1",
            "vec-0",
            "vec-1",
            "field-1",
            "field-2",
            "local-b",
            "local-a"
        ]
    );

    let p = Point { x: 1, y: 2 };
    let q = p; // copy, p still usable
    assert_eq!(p, q);
    let clones = Rc::new(RefCell::new(0));
    let c = Counted {
        payload: vec![1, 2, 3],
        clones: clones.clone(),
    };
    let d = c.clone();
    let _e = d.clone();
    assert_eq!(*clones.borrow(), 2);
    assert_eq!(c.payload, d.payload);
    // let moved_twice = c; let again = c;  // error[E0382]: use of moved value `c` (Counted is Clone, not Copy)
    println!(
        "7. drop order verified: {:?}",
        RefCell::borrow(&log).iter().skip(2).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 8. Iterator with size_hint, DoubleEndedIterator, ExactSizeIterator
// ---------------------------------------------------------------------------------------------------------------

/// Yields i*i for i in lo..hi. Because it knows its exact length it can promise ExactSize and run backwards.
#[derive(Debug, Clone)]
struct Squares {
    lo: u32,
    hi: u32,
}
impl Iterator for Squares {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        (self.lo < self.hi).then(|| {
            let i = self.lo as u64;
            self.lo += 1;
            i * i
        })
    }
    // size_hint is an optimisation hint for `collect`/`Vec::with_capacity`; ExactSizeIterator makes it a promise.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.hi - self.lo) as usize;
        (n, Some(n))
    }
}
impl DoubleEndedIterator for Squares {
    fn next_back(&mut self) -> Option<u64> {
        (self.lo < self.hi).then(|| {
            self.hi -= 1;
            let i = self.hi as u64;
            i * i
        })
    }
}
// `len()` has a default implementation built on size_hint and it asserts lower == upper: that is the whole contract.
impl ExactSizeIterator for Squares {}
impl std::iter::FusedIterator for Squares {} // once None, always None: true here, and lets adapters skip a guard

fn iterator_demo() {
    let mut rng = Rng(5);
    for _ in 0..500 {
        let lo = rng.below(20) as u32;
        let hi = lo + rng.below(30) as u32;
        let it = Squares { lo, hi };
        assert_eq!(it.len(), (hi - lo) as usize);
        let fwd: Vec<u64> = it.clone().collect();
        assert_eq!(
            fwd.len(),
            it.len(),
            "len matches the number of items actually produced"
        );
        let mut back: Vec<u64> = it.clone().rev().collect();
        back.reverse();
        assert_eq!(
            fwd, back,
            "rev() yields the same items in the opposite order"
        );
        // interleave next / next_back until they meet: each item exactly once, len decreasing by one each step
        let mut it2 = it.clone();
        let mut seen = Vec::new();
        let mut left = it2.len();
        loop {
            let item = if seen.len() % 2 == 0 {
                it2.next()
            } else {
                it2.next_back()
            };
            match item {
                Some(x) => {
                    left -= 1;
                    assert_eq!(it2.len(), left);
                    seen.push(x);
                }
                None => break,
            }
        }
        seen.sort();
        let mut sorted = fwd.clone();
        sorted.sort();
        assert_eq!(seen, sorted);
        assert_eq!(it2.next(), None);
    }
    // payoff of ExactSizeIterator: `enumerate().rev()` and `zip().rev()` only compile with exact sizes
    let ranked: Vec<(usize, u64)> = Squares { lo: 1, hi: 5 }.enumerate().rev().collect();
    assert_eq!(ranked, vec![(3, 16), (2, 9), (1, 4), (0, 1)]);
    println!(
        "8. iterator: 500 random ranges, len/rev/interleaving verified; enumerate().rev() = {ranked:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 9. Auto traits: Send / Sync
// ---------------------------------------------------------------------------------------------------------------

fn assert_send_sync<T: Send + Sync>() {}
fn assert_send<T: Send>() {}

/*  Send/Sync are AUTO traits: a struct is Send/Sync iff all fields are. You almost never implement them; you
*lose* them by adding a field that is not thread-safe. This does NOT compile:

    use std::rc::Rc;
    struct Handle { shared: Rc<i32> }        // Rc has a non-atomic refcount => !Send, !Sync
    fn needs_send<T: Send>(_: T) {}
    needs_send(Handle { shared: Rc::new(1) });
    // error[E0277]: `Rc<i32>` cannot be sent between threads safely
    //   = help: within `Handle`, the trait `Send` is not implemented for `Rc<i32>`

Likewise `Cell`/`RefCell` are Send but !Sync (interior mutability without locks), and a raw pointer field makes a
type neither. Fix by choosing Arc/Mutex/atomics, never by `unsafe impl Send` unless you can prove the invariant. */

fn send_sync_demo() {
    assert_send_sync::<SortedVec<i32>>(); // Vec<i32> inside, so automatically both
    assert_send_sync::<Bag<String>>();
    assert_send_sync::<Matrix>();
    assert_send::<RefCell<i32>>(); // Send but not Sync: movable between threads, not shareable by reference
    // `Counted` holds an Rc, so it is !Send: uncommenting the next line fails to compile (E0277):
    // assert_send::<Counted>();
    let shared = std::sync::Arc::new(std::sync::Mutex::new(Bag::<u32>::default()));
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let shared = shared.clone();
            std::thread::spawn(move || shared.lock().unwrap().extend([t; 3]))
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(shared.lock().unwrap().len(), 12);
    println!("9. auto traits: Bag is Send+Sync, so Arc<Mutex<Bag>> works across 4 threads");
}

// ---------------------------------------------------------------------------------------------------------------
// 10. Operators: Add, Sum, Product
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Money(i64); // cents
impl Add for Money {
    type Output = Money;
    fn add(self, o: Money) -> Money {
        Money(self.0 + o.0)
    }
}
impl<'a> Add<&'a Money> for Money {
    type Output = Money;
    fn add(self, o: &Money) -> Money {
        self + *o
    }
}
// Sum needs an identity (Default/zero); implement for owned and borrowed items so `.iter().sum()` and `.into_iter().sum()` both work.
impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money(0), Add::add)
    }
}
impl<'a> Sum<&'a Money> for Money {
    fn sum<I: Iterator<Item = &'a Money>>(iter: I) -> Money {
        iter.fold(Money(0), Add::add)
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
struct Factor(f64);
impl Mul for Factor {
    type Output = Factor;
    fn mul(self, o: Factor) -> Factor {
        Factor(self.0 * o.0)
    }
}
impl Product for Factor {
    fn product<I: Iterator<Item = Factor>>(iter: I) -> Factor {
        iter.fold(Factor(1.0), Mul::mul) // identity of multiplication
    }
}

fn operator_demo() {
    let mut rng = Rng(1);
    let xs: Vec<Money> = (0..300)
        .map(|_| Money(rng.below(10_000) as i64 - 5_000))
        .collect();
    let total: Money = xs.iter().sum();
    assert_eq!(total, xs.iter().copied().sum::<Money>());
    assert_eq!(total, Money(xs.iter().map(|m| m.0).sum()));
    // laws: identity and associativity
    assert_eq!(
        Vec::<Money>::new().into_iter().sum::<Money>(),
        Money::default()
    );
    let (a, b, c) = (xs[0], xs[1], xs[2]);
    assert_eq!((a + b) + c, a + (b + c));
    assert_eq!(
        Vec::<Factor>::new().into_iter().product::<Factor>(),
        Factor(1.0)
    );
    let p: Factor = [Factor(2.0), Factor(3.0), Factor(0.5)]
        .into_iter()
        .product();
    assert_eq!(p, Factor(3.0));
    println!(
        "10. operators: sum of 300 Money = {:?}, product = {:?}",
        total, p
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 11. io::Write / io::Read for an in-memory type
// ---------------------------------------------------------------------------------------------------------------

/// FIFO byte pipe: write at the back, read from the front. Handy as a test double for a socket.
#[derive(Default)]
struct Pipe {
    buf: VecDeque<u8>,
}
impl Write for Pipe {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend(data);
        Ok(data.len()) // we always accept everything; real sinks may return less, which is why write_all exists
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // nothing is buffered below us
    }
}
impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.buf.len());
        for (slot, byte) in out.iter_mut().zip(self.buf.drain(..n)) {
            *slot = byte;
        }
        Ok(n) // Ok(0) means EOF, so only return 0 when the buffer is empty or `out` is
    }
}

fn io_demo() {
    let mut p = Pipe::default();
    writeln!(p, "line one").unwrap(); // Write gives write!/writeln!/write_all/by_ref for free
    write!(p, "line {}", "two\nthree").unwrap();
    // tiny reads must still reassemble the stream: the Read contract is "some bytes", not "all bytes"
    let mut small = [0u8; 3];
    let mut got = Vec::new();
    let first = p.read(&mut small).unwrap();
    got.extend_from_slice(&small[..first]);
    assert_eq!(first, 3);
    p.read_to_end(&mut got).unwrap(); // Read gives read_to_end/read_to_string/bytes/take/chain
    assert_eq!(String::from_utf8(got).unwrap(), "line one\nline two\nthree");
    assert_eq!(p.read(&mut small).unwrap(), 0, "drained pipe reports EOF");

    // compose with std adapters: BufReader::lines over our Read, and io::copy between two Pipes
    let mut src = Pipe::default();
    for i in 0..5 {
        writeln!(src, "row {i}").unwrap();
    }
    let mut dst = Pipe::default();
    let copied = io::copy(&mut src, &mut dst).unwrap();
    assert_eq!(copied, 30);
    let lines: Vec<String> = BufReader::new(dst).lines().map(Result::unwrap).collect();
    assert_eq!(lines, ["row 0", "row 1", "row 2", "row 3", "row 4"]);
    println!(
        "11. io: Pipe works with writeln!, read_to_end, io::copy and BufReader::lines ({copied} bytes)"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// 12. Checklist
// ---------------------------------------------------------------------------------------------------------------

fn checklist() {
    // From the API Guidelines C-COMMON-TRAITS, C-CONV-TRAITS, C-COLLECT, C-SEND-SYNC, C-GOOD-ERR, C-DEREF:
    // https://rust-lang.github.io/api-guidelines/interoperability.html
    let rows = [
        (
            "Debug",
            "every public type",
            "derive; hand-write to redact secrets (C-DEBUG)",
        ),
        (
            "Clone",
            "owned data, non-resource",
            "derive; Copy only if small, no Drop, all fields Copy",
        ),
        (
            "PartialEq/Eq",
            "values compared by content",
            "derive; Eq unless floats; keep == and Hash in step",
        ),
        (
            "Hash",
            "if used as a map key",
            "must agree with Eq; hash what Eq compares",
        ),
        (
            "PartialOrd/Ord",
            "if there is a natural order",
            "Ord = total, agree with Eq; floats need total_cmp",
        ),
        (
            "Default",
            "when an obvious 'empty' exists",
            "derive if zero values are right, else manual",
        ),
        (
            "Display",
            "user-facing text",
            "use f.pad / pad_integral to honour width, fill, precision",
        ),
        (
            "FromStr",
            "has a canonical text form",
            "round trip with Display; real error type",
        ),
        (
            "From/TryFrom",
            "lossless / fallible conversions",
            "never impl Into; TryFrom for validation",
        ),
        (
            "AsRef/Borrow",
            "cheap view / map-key lookup",
            "Borrow only if Eq+Hash+Ord match the borrowed form",
        ),
        (
            "FromIterator/Extend",
            "any collection",
            "collect() and extend() of the same items agree",
        ),
        (
            "IntoIterator x3",
            "any collection",
            "owned, &, and &mut only if no invariant is at risk",
        ),
        (
            "Deref",
            "smart pointers only",
            "read-only unless any &mut [T] is safe; not for inheritance",
        ),
        (
            "Error",
            "every error type",
            "Display = this level, source() = cause; Send+Sync+'static",
        ),
        (
            "Send/Sync",
            "automatic",
            "check with assert_send_sync::<T>(); avoid unsafe impl",
        ),
        (
            "Read/Write",
            "byte sources and sinks",
            "short reads/writes are legal; Ok(0) read = EOF",
        ),
    ];
    println!("12. which traits should my type implement?");
    println!(
        "    {:<20} {:<32} {}",
        "trait", "implement when", "law / note"
    );
    for (t, when, note) in rows {
        println!("    {t:<20} {when:<32} {note}");
    }
    assert!(rows.len() >= 16);
}

fn main() {
    collections_demo();
    index_demo();
    hash_demo();
    ordering_demo();
    formatting_demo();
    conversion_demo();
    drop_demo();
    iterator_demo();
    send_sync_demo();
    operator_demo();
    io_demo();
    checklist();
    println!("all trait laws verified");
}
