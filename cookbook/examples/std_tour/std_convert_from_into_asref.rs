//! RECIPE 1002 (std): conversion traits: From/Into/TryFrom/AsRef/Borrow/FromStr/Display/Default/FromIterator/IntoIterator/Extend/Deref.
//! LEVEL: intermediate
//! Use in: designing ergonomic APIs, newtypes, validated types, parsing, generic function bounds.   Run: cargo run --release --example std_convert_from_into_asref
//! COMMON USES: `fn open(p: impl AsRef<Path>)`, `fn new(name: impl Into<String>)`, newtype IDs with TryFrom validation,
//! `FromStr` for config enums, `FromIterator` for custom collections, `Borrow<str>` so HashMap<String,_>::get(&str) works.
//! METHODS: (everyday first) From::from, Into::into, TryFrom::try_from, TryInto::try_into, AsRef::as_ref, FromStr::from_str, str::parse, ToString::to_string,
//! METHODS: Default::default, IntoIterator::into_iter, FromIterator::from_iter, Extend::extend, AsMut::as_mut, Borrow::borrow, BorrowMut::borrow_mut,
//! METHODS: ToOwned::to_owned, Deref::deref, DerefMut::deref_mut, std::fmt::Display, i32::try_from, u8::try_from, char::from_u32, char::try_from,
//! METHODS: i64::from, f64::from, usize::try_from, std::path::Path::new, String::from, Vec::from, Box::from, Rc::from, Arc::from
//! DOCS: https://doc.rust-lang.org/std/convert/index.html
//! DEFINED IN: From/Into/TryFrom/TryInto/AsRef/AsMut/Borrow/Deref/Default/FromStr/FromIterator core; ToOwned/ToString alloc; AsRef<Path> and Path std.
//! NOT COVERED (and why): Infallible beyond mention, AsRef for OsStr/CStr (see std_env_args_vars), unsafe conversions (transmute: see unsafe chapter).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   - accept &str or String in a ctor      -> `impl Into<String>`      - accept &str/String/PathBuf/&Path -> `impl AsRef<Path>`
//!   - accept any list of strings           -> `impl IntoIterator<Item = impl AsRef<str>>`
//!   - infallible widening (u8 -> u32)      -> From / Into              - fallible narrowing -> TryFrom / try_into
//!   - truncating cast on purpose           -> `as` (document it)       - validated newtype -> private field + TryFrom
//!   - parse text to my type                -> impl FromStr (then `.parse::<T>()`)
//!   - print my type                        -> impl Display (gives to_string for free)
//!   - HashMap<String,_> lookup by &str     -> Borrow<str> (already implemented by String)
//!   - build my collection from iterator    -> impl FromIterator (+ Extend)
//!   - make a wrapper behave like inner     -> Deref (only for smart-pointer-like types!)
//! Read: https://doc.rust-lang.org/std/convert/trait.From.html , https://rust-lang.github.io/api-guidelines/interoperability.html
//! TRY NEXT:
//!   - [S] Try to write `impl<T> From<Meters> for T` and read the orphan/coherence error; then wrap in a local newtype to fix it.
//!   - [S] Predict `300_i32 as u8`, `-1_i32 as u32`, `3.9_f64 as i32`, `f64::NAN as i32`, then assert; compare each with try_from.
//!   - [M] Add `impl TryFrom<&str> for Email` that rejects empty local parts; property test with a seeded xorshift for 1000 strings.
//!   - [M] Make `Stack<T>` implement FromIterator, Extend, IntoIterator for owned, &, and &mut; check `for x in &mut stack`.
//!   - [A] PROJECT: combine with std_cow_borrow_toowned and std_result_methods: a `Config::from_str` returning a typed error enum and accepting `impl AsRef<str>` lines, borrowing when no escapes occur.
use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const COVERED: &[&str] = &[
    "From",
    "Into",
    "TryFrom",
    "TryInto",
    "AsRef",
    "AsMut",
    "Borrow",
    "BorrowMut",
    "ToOwned",
    "FromStr",
    "ToString",
    "Display",
    "Default",
    "FromIterator",
    "IntoIterator",
    "Extend",
    "Deref",
    "DerefMut",
];

// ---- 0. Everyday core: ctor/function argument conversions -----------------------------------------------------
struct User {
    name: String,
    tags: Vec<String>,
}
impl User {
    // impl Into<String>: caller can pass "x", String, &String, Cow, char; we allocate only if given &str.
    fn new(name: impl Into<String>) -> Self {
        User {
            name: name.into(),
            tags: vec![],
        }
    }
    // impl IntoIterator<Item = impl AsRef<str>>: Vec<String>, [&str; N], HashSet<&str>, iterators.
    fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.tags
            .extend(tags.into_iter().map(|t| t.as_ref().to_owned()));
        self
    }
}
fn ext_of(p: impl AsRef<Path>) -> Option<String> {
    p.as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_owned)
}
fn shout(s: impl AsRef<str>) -> String {
    s.as_ref().to_uppercase()
}

fn section_everyday() {
    let owned = String::from("Ada");
    let u = User::new("Ada").with_tags(["a", "b"]);
    let v = User::new(owned.clone()).with_tags(vec![String::from("c")]);
    assert_eq!(
        (u.name.as_str(), u.tags.len(), v.tags[0].as_str()),
        ("Ada", 2, "c")
    );
    assert_eq!(ext_of("a/b.txt"), Some("txt".into()));
    assert_eq!(ext_of(PathBuf::from("x.rs")), Some("rs".into()));
    assert_eq!(ext_of(Path::new("noext")), None);
    assert_eq!(shout("abc"), shout(String::from("abc")));
    // PITFALL: generic fns are monomorphised per type: code bloat. Common trick: thin generic wrapper, inner non-generic fn.
    fn open_inner(p: &Path) -> usize {
        p.as_os_str().len()
    }
    fn open(p: impl AsRef<Path>) -> usize {
        open_inner(p.as_ref())
    }
    assert_eq!(open("abc"), 3);
    println!("0. everyday ok");
}

// ---- 1. From / Into and the blanket impl ----------------------------------------------------------------------
#[derive(Debug, PartialEq, Clone, Copy)]
struct Celsius(f64);
#[derive(Debug, PartialEq, Clone, Copy)]
struct Fahrenheit(f64);
impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}
// You implement From; Into comes free through `impl<T, U: From<T>> Into<U> for T`.
// Implement Into directly only when the target is a foreign type (orphan rule), see section 7.

fn section_from_into() {
    let f: Fahrenheit = Celsius(100.0).into();
    assert_eq!(f, Fahrenheit(212.0));
    assert_eq!(Fahrenheit::from(Celsius(0.0)), Fahrenheit(32.0));
    // std From impls worth knowing: lossless widening, String from &str/char/Box<str>, Vec from slice/array/&str.
    assert_eq!(i64::from(7_i32), 7);
    assert_eq!(f64::from(3_u32), 3.0);
    assert_eq!(u32::from('a'), 97);
    assert_eq!(u32::from(true), 1);
    assert_eq!(String::from('x'), "x");
    assert_eq!(Vec::from([1, 2]), vec![1, 2]);
    assert_eq!(Vec::<u8>::from("hi"), b"hi".to_vec());
    let b: Box<str> = "boxed".into();
    let r: std::rc::Rc<str> = "rc".into();
    let a: std::sync::Arc<[i32]> = vec![1, 2].into();
    assert_eq!((&*b, &*r, a.len()), ("boxed", "rc", 2));
    // `?` uses From on errors (see std_result_methods); `Option<T>: From<T>`:
    let o: Option<i32> = 5.into();
    assert_eq!(o, Some(5));
    // From<T> for T exists (identity) and From<!>/Infallible is why `TryFrom` via From has Error = Infallible.
    let n: Result<i64, Infallible> = i64::try_from(5_i32);
    assert_eq!(n, Ok(5));
    println!("1. From/Into ok");
}

// ---- 2. Numeric conversions: From vs as vs try_from -----------------------------------------------------------
fn section_numeric() {
    // From: only when lossless. u8->u32 yes; u32->u8 does not compile.
    let widened: u32 = u32::from(200_u8);
    assert_eq!(widened, 200);
    // `as`: always compiles, silently wraps/saturates/truncates. Know the rules, use it for deliberate bit work.
    assert_eq!(300_i32 as u8, 44); // modulo 256
    assert_eq!(-1_i32 as u32, u32::MAX);
    assert_eq!(3.9_f64 as i32, 3); // truncates toward zero
    assert_eq!(f64::NAN as i32, 0); // NaN -> 0
    assert_eq!(1e20_f64 as i32, i32::MAX); // saturates
    assert_eq!(-1.0_f64 as u8, 0);
    // try_from: checked narrowing; use at trust boundaries (lengths from the wire, user indexes).
    assert!(u8::try_from(300_i32).is_err());
    assert_eq!(u8::try_from(200_i32), Ok(200));
    assert!(usize::try_from(-1_i64).is_err());
    let r: Result<u16, _> = 70_000_u32.try_into();
    assert_eq!(
        r.unwrap_err().to_string(),
        "out of range integral type conversion attempted"
    );
    // char conversions.
    assert_eq!(char::from_u32(0x41), Some('A'));
    assert_eq!(char::from_u32(0xD800), None); // surrogate: not a char
    assert_eq!(char::try_from(0x1F600_u32).map(|c| c.len_utf8()), Ok(4));
    assert_eq!(char::from(65_u8), 'A');
    println!("2. numeric ok");
}

// ---- 3. TryFrom for validated types --------------------------------------------------------------------------
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct Port(u16);
#[derive(Debug, PartialEq)]
enum PortError {
    Zero,
    TooBig(u32),
}
impl TryFrom<u32> for Port {
    type Error = PortError;
    fn try_from(v: u32) -> Result<Self, PortError> {
        match u16::try_from(v) {
            Ok(0) => Err(PortError::Zero),
            Ok(p) => Ok(Port(p)),
            Err(_) => Err(PortError::TooBig(v)),
        }
    }
}
fn section_tryfrom() {
    assert_eq!(Port::try_from(8080), Ok(Port(8080)));
    assert_eq!(Port::try_from(0), Err(PortError::Zero));
    assert_eq!(Port::try_from(70000), Err(PortError::TooBig(70000)));
    // TryInto comes free; type annotation picks the impl.
    let p: Result<Port, _> = 443_u32.try_into();
    assert_eq!(p, Ok(Port(443)));
    // Design: keep the field private in a module so every Port is valid ("parse, don't validate").
    println!("3. TryFrom ok");
}

// ---- 4. FromStr + Display + ToString -------------------------------------------------------------------------
#[derive(Debug, PartialEq, Clone, Copy)]
enum Level {
    Debug,
    Info,
    Warn,
}
#[derive(Debug, PartialEq)]
struct ParseLevelError(String);
impl FromStr for Level {
    type Err = ParseLevelError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "debug" => Ok(Level::Debug),
            "info" => Ok(Level::Info),
            "warn" | "warning" => Ok(Level::Warn),
            other => Err(ParseLevelError(other.to_owned())),
        }
    }
}
impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // f.pad respects width/fill/precision flags ({:>8}); write_str ignores them.
        f.pad(match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
        })
    }
}
fn section_fromstr_display() {
    assert_eq!("WARNING".parse::<Level>(), Ok(Level::Warn));
    assert_eq!(Level::from_str("nope"), Err(ParseLevelError("nope".into())));
    // Display gives ToString via a blanket impl; never implement ToString directly (clippy::to_string_trait_impl).
    assert_eq!(Level::Info.to_string(), "info");
    assert_eq!(format!("[{:>6}]", Level::Info), "[  info]");
    // Round trip property.
    for l in [Level::Debug, Level::Info, Level::Warn] {
        assert_eq!(l.to_string().parse::<Level>(), Ok(l));
    }
    println!("4. FromStr/Display ok");
}

// ---- 5. AsRef / AsMut / Borrow / ToOwned ---------------------------------------------------------------------
fn count_bytes<T: AsRef<[u8]>>(x: T) -> usize {
    x.as_ref().len()
}
fn zero<T: AsMut<[u8]>>(mut x: T) -> T {
    x.as_mut().fill(0);
    x
}
fn section_asref_borrow() {
    // BorrowMut: the mutable twin of Borrow. Generic code can edit "something that is, or owns, a [i32]".
    fn bump<B: std::borrow::BorrowMut<[i32]>>(mut b: B) -> B {
        std::borrow::BorrowMut::borrow_mut(&mut b)
            .iter_mut()
            .for_each(|x| *x += 1);
        b
    }
    assert_eq!(bump(vec![1, 2]), vec![2, 3]);
    assert_eq!(bump([5]), [6]);
    assert_eq!(count_bytes("abc"), 3);
    assert_eq!(count_bytes(vec![1u8, 2]), 2);
    assert_eq!(count_bytes([0u8; 4]), 4);
    assert_eq!(zero(vec![1u8, 2]), vec![0, 0]);
    // Borrow<str> for String is what makes map.get("key") work with String keys, no allocation.
    let mut m: HashMap<String, i32> = HashMap::new();
    m.insert("k".to_string(), 1);
    assert_eq!(m.get("k"), Some(&1)); // K: Borrow<Q>, Q = str
    let key = "k".to_string();
    let as_str: &str = Borrow::<str>::borrow(&key);
    assert_eq!(as_str, "k");
    // Why Borrow and not AsRef: Borrow promises Eq/Hash/Ord agree with the owned type; AsRef promises nothing.
    // A type whose Hash differs from its as_ref() would break maps if lookups used AsRef.
    // ToOwned: the generalisation of Clone for unsized types: str -> String, [T] -> Vec<T>, Path -> PathBuf.
    let s: String = "abc".to_owned();
    let v: Vec<i32> = [1, 2][..].to_owned();
    let p: PathBuf = Path::new("a").to_owned();
    assert_eq!((s.len(), v.len(), p.as_os_str().len()), (3, 2, 1));
    println!("5. AsRef/Borrow/ToOwned ok");
}

// ---- 6. Default, FromIterator, IntoIterator, Extend ----------------------------------------------------------
#[derive(Debug, Default, PartialEq)]
struct Bag {
    items: Vec<String>,
}
impl<S: Into<String>> FromIterator<S> for Bag {
    fn from_iter<I: IntoIterator<Item = S>>(it: I) -> Self {
        Bag {
            items: it.into_iter().map(Into::into).collect(),
        }
    }
}
impl<S: Into<String>> Extend<S> for Bag {
    fn extend<I: IntoIterator<Item = S>>(&mut self, it: I) {
        self.items.extend(it.into_iter().map(Into::into));
    }
}
// Three IntoIterator impls: owned, shared ref, mut ref. This is what makes `for x in &bag` and `for x in bag` work.
impl IntoIterator for Bag {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
impl<'a> IntoIterator for &'a Bag {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}
impl<'a> IntoIterator for &'a mut Bag {
    type Item = &'a mut String;
    type IntoIter = std::slice::IterMut<'a, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}
fn section_collections() {
    let mut b: Bag = ["a", "b"].into_iter().collect();
    b.extend(vec![String::from("c")]);
    for s in &mut b {
        s.push('!');
    }
    let refs: Vec<&String> = (&b).into_iter().collect();
    assert_eq!(refs.len(), 3);
    let owned: Vec<String> = b.into_iter().collect();
    assert_eq!(owned, vec!["a!", "b!", "c!"]);
    assert_eq!(Bag::default(), Bag { items: vec![] });
    // Default tips: derive when all fields are Default; `..Default::default()` for struct update.
    #[derive(Debug, Default)]
    struct Opts {
        retries: u32,
        name: String,
        verbose: bool,
    }
    let o = Opts {
        retries: 3,
        ..Default::default()
    };
    assert_eq!((o.retries, o.name.as_str(), o.verbose), (3, "", false));
    println!("6. collections traits ok");
}

// ---- 7. Newtypes, orphan rule, Deref -------------------------------------------------------------------------
// Orphan rule: you may implement a foreign trait only for a local type (or with a local type parameter).
// `impl From<Meters> for f64` is allowed (Meters is local); `impl Display for Vec<u8>` is not. Workaround: newtype.
struct Meters(f64);
impl From<Meters> for f64 {
    fn from(m: Meters) -> f64 {
        m.0
    }
}
struct Hex(Vec<u8>);
impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}
// Deref: only for smart-pointer-like wrappers where &Wrapper -> &Inner is the natural view.
struct SortedVec(Vec<i32>);
impl Deref for SortedVec {
    type Target = [i32]; // expose only the read-only slice API, not Vec::push (which would break the invariant)
    fn deref(&self) -> &[i32] {
        &self.0
    }
}
struct Guarded(Vec<i32>);
impl Deref for Guarded {
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> {
        &self.0
    }
}
impl DerefMut for Guarded {
    fn deref_mut(&mut self) -> &mut Vec<i32> {
        &mut self.0
    }
}
fn section_newtypes() {
    let x: f64 = Meters(2.5).into();
    assert_eq!(x, 2.5);
    assert_eq!(Hex(vec![0, 255, 16]).to_string(), "00ff10");
    let mut sv = SortedVec(vec![1, 3, 5]);
    sv.0.sort();
    assert_eq!(sv.binary_search(&3), Ok(1)); // slice method via auto-deref
    assert_eq!(sv.len(), 3);
    let mut g = Guarded(vec![]);
    g.push(1); // DerefMut gives full Vec API
    assert_eq!(g.len(), 1);
    // Deref coercion at call sites: &String -> &str, &Vec<T> -> &[T], &Box<T> -> &T.
    fn takes_str(s: &str) -> usize {
        s.len()
    }
    assert_eq!(takes_str(&String::from("abc")), 3);
    println!("7. newtypes/Deref ok");
}

// ---- 8. Production tips and tricks ---------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Public ctor args: `impl Into<String>` for owned storage, `&str` when you only read (clippy::needless_pass_by_value).
    // 2. Don't implement Into (implement From); clippy::from_over_into.
    // 3. `as` between numeric types hides bugs: clippy::cast_possible_truncation / cast_sign_loss (pedantic). Use try_from.
    let len: usize = 5_000_000_000;
    assert!(u32::try_from(len).is_err() || usize::BITS == 32);
    // 4. `.to_string()` on a &str allocates the same as to_owned; use `.to_owned()`/`String::from` (clippy::str_to_string, restriction).
    // 5. Avoid `impl AsRef<str>` on hot recursion paths: monomorphisation bloat; use inner fn (section 0).
    // 6. Deref abuse (inheritance emulation) is an anti-pattern; delegate explicitly or use AsRef (API guideline C-DEREF).
    // 7. Newtype IDs prevent mixing UserId and OrderId: From<u64> for each, no Into between them.
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct UserId(u64);
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct OrderId(u64);
    impl From<u64> for UserId {
        fn from(v: u64) -> Self {
            UserId(v)
        }
    }
    impl From<u64> for OrderId {
        fn from(v: u64) -> Self {
            OrderId(v)
        }
    }
    let u: UserId = 7.into();
    let o: OrderId = 7.into();
    assert_eq!((u.0, o.0), (7, 7)); // `u == o` would not compile: that's the point.
    // 8. FromStr error types should implement Error + Display so `?` works at the app boundary.
    // 9. `impl From<X> for Y` where conversion can fail is a bug: TryFrom (never panic in From).
    // 10. Provide both `From<&str>` and `From<String>` only if the String case can avoid an allocation.
    // 11. `Into<Option<T>>` args let callers write `f(5)` or `f(None)` (builder ergonomics).
    fn timeout(t: impl Into<Option<u32>>) -> u32 {
        t.into().unwrap_or(30)
    }
    assert_eq!((timeout(5), timeout(None)), (5, 30));
    // 12. Accept `&[T]`/`impl IntoIterator` rather than `&Vec<T>` (clippy::ptr_arg).
    fn total(xs: &[i32]) -> i32 {
        xs.iter().sum()
    }
    assert_eq!(total(&vec![1, 2]), total(&[1, 2]));
    println!("8. production tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_from_into();
    section_numeric();
    section_tryfrom();
    section_fromstr_display();
    section_asref_borrow();
    section_collections();
    section_newtypes();
    section_production_tips();
    println!("all {} conversion traits exercised; done", COVERED.len());
}
