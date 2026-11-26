//! RECIPE 1026 (std): arrays [T; N], tuples, the unit type (), and "never" (`!`, Infallible, empty enums), plus zero-sized types.
//! LEVEL: intermediate
//! Use in: fixed-size buffers, matrices, composite map keys, multiple return values, APIs that cannot fail, typestate.   Run: cargo run --release --example std_array_tuple_unit_never
//! COMMON USES: [u8; 32] digests, ring buffers sized by a const generic, (tenant, user) cache keys, multi-key sort,
//! Result<(), E>, HashMap<K, ()> as a set, `type Err = Infallible` for a parser that always succeeds.
//! METHODS: (everyday first) array::len, array::iter, array::into_iter, array::map, array::contains, std::array::from_fn, std::array::repeat,
//! METHODS: array::each_ref, array::each_mut, array::as_slice, array::as_mut_slice, std::array::from_ref, std::array::from_mut,
//! METHODS: TryFrom::try_from, TryInto::try_into, slice::first_chunk, slice::concat, slice::as_flattened, slice::windows, Default::default,
//! METHODS: std::mem::replace, std::mem::swap, Iterator::unzip, Iterator::zip, Iterator::enumerate, Ord::cmp, From::from,
//! METHODS: std::convert::Infallible, std::marker::PhantomData, std::mem::size_of, std::mem::align_of, Iterator::sum
//! DOCS: https://doc.rust-lang.org/std/primitive.array.html
//! DEFINED IN: arrays, tuples, unit, `!`, `core::array`, `Infallible`, `PhantomData`, size_of are all core. Vec/HashMap used
//! in the scenarios are alloc/std.
//! Also: https://doc.rust-lang.org/std/primitive.tuple.html , https://doc.rust-lang.org/std/primitive.never.html
//! NOT COVERED (and why): array::try_map/try_from_fn/zip/split_array_* (unstable in 1.98.1), `!` as an ordinary type name
//! (unstable: it is only writable as a return type), hint::unreachable_unchecked is named but not called (unsafe). The slice
//! methods arrays get by coercion are in std_slice_search_sort_binary and std_slice_chunks_windows_split.
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   N computed values                          -> std::array::from_fn(|i| ..)
//!   N copies of a non-Copy value               -> std::array::repeat(v)  (Clone, since 1.91)   or from_fn
//!   N empty/None slots of a non-Copy type      -> [const { None }; N]
//!   transform each element, keep the size      -> map
//!   map without consuming the array            -> each_ref().map(..)
//!   slice -> array of exactly N                -> <[T; N]>::try_from(slice) / slice.try_into()
//!   first N elements as an array               -> slice.first_chunk::<N>()
//!   sort by several keys                       -> sort_by_key(|x| (x.a, Reverse(x.b)))
//!   two vectors from pairs                     -> let (a, b): (Vec<_>, Vec<_>) = it.unzip();
//!   "succeeds or fails, no data"               -> Result<(), E>
//!   "this error cannot happen"                 -> Result<T, Infallible>   then `let Ok(v) = r;`
//!   function that never returns                -> fn f() -> !
//!   a compile-time tag with no storage         -> PhantomData<Tag>
//!
//! LIMITS (std docs): `Default` for arrays exists only up to length 32. Tuples implement Eq/Ord/Hash/Debug/Default only up
//! to arity 12 (Clone/Copy for any length). Arrays implement the comparison traits and Debug for every length.
//!
//! Read: https://doc.rust-lang.org/std/primitive.array.html , https://doc.rust-lang.org/std/primitive.tuple.html ,
//!       https://doc.rust-lang.org/std/primitive.never.html
//! TRY NEXT:
//!   - [S] Change `Window<4>` to `Window<0>` and decide what should happen: make it a compile error with a const assertion.
//!   - [S] Try `<[u8; 33]>::default()` and read the compiler error; then initialise it two other ways.
//!   - [M] Implement `transpose` for non-square matrices with two const parameters and test that transposing twice is identity.
//!   - [M] Add a third state to the `Door` typestate and see which impl blocks the compiler makes you touch.
//!   - [A] PROJECT: a fixed-capacity inline vector over [Option<T>; N], differential-tested against Vec
//!         (std_vec_vecdeque_methods), with its size compared using size_of.
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::marker::PhantomData;
use std::mem::{align_of, size_of};
use std::str::FromStr;

const COVERED: &[&str] = &[
    "len",
    "iter",
    "into_iter",
    "map",
    "contains",
    "from_fn",
    "repeat",
    "each_ref",
    "each_mut",
    "as_slice",
    "as_mut_slice",
    "from_ref",
    "from_mut",
    "try_from",
    "try_into",
    "first_chunk",
    "concat",
    "as_flattened",
    "windows",
    "default",
    "replace",
    "swap",
    "unzip",
    "zip",
    "enumerate",
    "cmp",
    "from",
    "Infallible",
    "PhantomData",
    "size_of",
    "align_of",
    "sum",
];

// ---- 1. Everyday core: arrays ----------------------------------------------------------------------------------
fn section_arrays() {
    let a: [i32; 4] = [3, 1, 4, 1];
    assert_eq!(
        (a.len(), a[2], a.get(9), a.contains(&4)),
        (4, 4, None, true)
    );
    assert_eq!(a.iter().sum::<i32>(), 9);
    // [T; N] is a VALUE: if T is Copy, assignment copies all elements.
    let mut b = a;
    b[0] = 99;
    assert_eq!((a[0], b[0]), (3, 99));
    // Sorting and searching come from slices (arrays coerce).
    let mut s = a;
    s.sort_unstable();
    assert_eq!((s, s.binary_search(&4)), ([1, 1, 3, 4], Ok(3)));
    // Since edition 2021, array.into_iter() yields VALUES, so owned elements can be moved out.
    let names = [String::from("a"), String::from("b")];
    let moved: Vec<String> = names.into_iter().collect();
    assert_eq!(moved, ["a", "b"]);
    // Comparisons are element-wise and lexicographic; arrays of equal element types and length only.
    assert!([1, 2, 3] < [1, 3, 0] && [0u8; 40] == [0u8; 40]);
    // Arrays hash like slices and work as map keys: a 4-byte IPv4 address, a 32-byte digest.
    let hits: HashMap<[u8; 4], u32> = [([127, 0, 0, 1], 3)].into();
    assert_eq!(hits.get(&[127, 0, 0, 1]), Some(&3));
    // Destructuring checks the length at compile time.
    let [first, .., last] = a;
    assert_eq!((first, last), (3, 1));
    println!("1. array basics ok");
}

// ---- 2. Building arrays ----------------------------------------------------------------------------------------
fn section_building() {
    assert_eq!([7u8; 4], [7, 7, 7, 7]); // [expr; N] needs a Copy value (or a const item)
    assert_eq!(
        std::array::from_fn::<usize, 5, _>(|i| i * i),
        [0, 1, 4, 9, 16]
    );
    let buckets: [Vec<u8>; 3] = std::array::from_fn(|_| Vec::new());
    assert!(buckets.iter().all(Vec::is_empty));
    // An inline const is evaluated afresh per element, so non-Copy constants work (since 1.79).
    let slots: [Option<String>; 3] = [const { None }; 3];
    assert_eq!(slots, [None, None, None]);
    let hi: [String; 3] = std::array::repeat(String::from("hi")); // since 1.91: clones N-1 times
    assert_eq!(hi, ["hi", "hi", "hi"]);
    // map moves each element and keeps N.
    let lens: [usize; 3] = hi.map(|s| s.len());
    assert_eq!(lens, [2, 2, 2]);
    // each_ref / each_mut: an array of references, so map does not consume the original.
    let mut words = [String::from("ab"), String::from("c")];
    let upper: [String; 2] = words.each_ref().map(|s| s.to_uppercase());
    IntoIterator::into_iter(words.each_mut()).for_each(|s| s.push('!'));
    assert_eq!(
        (upper, words.clone()),
        (
            [String::from("AB"), String::from("C")],
            [String::from("ab!"), String::from("c!")]
        )
    );
    assert_eq!(words.as_slice().len(), 2);
    let mut nums = [1, 2];
    nums.as_mut_slice().reverse();
    assert_eq!(nums, [2, 1]);
    // from_ref / from_mut: view one value as a one-element array.
    let mut x = 5;
    assert_eq!(std::array::from_ref(&x), &[5]);
    std::array::from_mut(&mut x)[0] += 1;
    assert_eq!(x, 6);
    println!("2. building arrays ok");
}

// ---- 3. Slices <-> arrays --------------------------------------------------------------------------------------
fn parse_header(bytes: &[u8]) -> Option<(u16, u32)> {
    let kind: [u8; 2] = *bytes.first_chunk()?;
    // try_into on a sub-slice: succeeds only if the length is exactly 4.
    let len: [u8; 4] = bytes.get(2..6)?.try_into().ok()?;
    Some((u16::from_be_bytes(kind), u32::from_be_bytes(len)))
}

fn section_conversions() {
    assert_eq!(parse_header(&[0, 1, 0, 0, 1, 0, 9]), Some((1, 256)));
    assert_eq!(parse_header(&[0, 1, 0]), None);
    assert!(<[u8; 3]>::try_from(&[1u8, 2][..]).is_err()); // wrong length
    assert_eq!(<[u8; 2]>::try_from(&[1u8, 2][..]).ok(), Some([1, 2])); // TryFromSliceError is not PartialEq
    // Vec -> array moves the elements; on failure the Vec comes back.
    let v = vec![String::from("x"), String::from("y")];
    let back = <[String; 3]>::try_from(v).unwrap_err();
    assert_eq!(back.len(), 2);
    let arr: [String; 2] = back.try_into().expect("length checked");
    assert_eq!(Vec::from(arr), ["x", "y"]); // and array -> Vec
    let bx: Box<[u8]> = Box::from([1, 2, 3]);
    assert_eq!(bx.len(), 3);
    println!("3. conversions ok");
}

// ---- 4. Matrices and const generics ----------------------------------------------------------------------------
fn transpose<const R: usize, const C: usize>(m: [[i32; C]; R]) -> [[i32; R]; C] {
    std::array::from_fn(|c| std::array::from_fn(|r| m[r][c]))
}
/// Sliding window of the last N samples, stored inline with no heap allocation.
struct Window<const N: usize> {
    buf: [u32; N],
    next: usize,
    filled: usize,
}
impl<const N: usize> Window<N> {
    const NON_EMPTY: () = assert!(N > 0, "Window needs N > 0"); // evaluated per N, at compile time
    fn new() -> Self {
        let () = Self::NON_EMPTY;
        Window {
            buf: [0; N],
            next: 0,
            filled: 0,
        }
    }
    /// Returns the evicted sample once the window is full.
    fn push(&mut self, x: u32) -> Option<u32> {
        let old = std::mem::replace(&mut self.buf[self.next], x);
        self.next = (self.next + 1) % N;
        if self.filled < N {
            self.filled += 1;
            None
        } else {
            Some(old)
        }
    }
    fn mean(&self) -> Option<f64> {
        (self.filled > 0).then(|| {
            self.buf[..self.filled]
                .iter()
                .map(|&x| f64::from(x))
                .sum::<f64>()
                / self.filled as f64
        })
    }
}

fn section_matrix() {
    let m = [[1, 2, 3], [4, 5, 6]];
    let t = transpose(m);
    assert_eq!(t, [[1, 4], [2, 5], [3, 6]]);
    assert_eq!(transpose(t), m);
    assert_eq!(m.as_flattened(), [1, 2, 3, 4, 5, 6]); // contiguous: no pointer per row
    assert_eq!(m.concat(), vec![1, 2, 3, 4, 5, 6]); // concat copies into a Vec
    assert_eq!(size_of::<[[i32; 3]; 2]>(), 24);
    assert!(m.as_flattened().windows(2).all(|w| w[0] < w[1]));

    let mut w = Window::<4>::new();
    assert_eq!(w.mean(), None);
    let evicted: Vec<_> = (1..=6).map(|i| w.push(i)).collect();
    assert_eq!(evicted, [None, None, None, None, Some(1), Some(2)]);
    assert_eq!(w.mean(), Some(4.5));
    // Default for arrays stops at 32; from_fn works for any N.
    assert_eq!(<[u8; 32]>::default(), [0; 32]);
    let long: [u8; 40] = std::array::from_fn(|_| Default::default());
    assert_eq!(long.len(), 40);
    println!("4. matrices and const generics ok");
}

// ---- 5. Tuples -------------------------------------------------------------------------------------------------
fn min_max(xs: &[i32]) -> Option<(i32, i32)> {
    let (&first, rest) = xs.split_first()?;
    Some(
        rest.iter()
            .fold((first, first), |(lo, hi), &x| (lo.min(x), hi.max(x))),
    )
}
/// A tuple STRUCT is a distinct named type; a plain tuple is structural.
#[derive(Debug, PartialEq)]
struct Meters(f64);
#[derive(Debug, PartialEq)]
struct Seconds(f64);

fn section_tuples() {
    assert_eq!(min_max(&[3, -1, 7]), Some((-1, 7)));
    assert_eq!(min_max(&[]), None);
    let nested = (1, ("two", [3, 4]), 5);
    let (a, (b, [c, d]), ..) = nested;
    assert_eq!(
        (a, b, c, d, nested.2, (nested.1).0),
        (1, "two", 3, 4, 5, "two")
    );
    // Destructuring assignment swaps and updates without temporaries.
    let (mut x, mut y) = (0u64, 1u64);
    for _ in 0..10 {
        (x, y) = (y, x + y);
    }
    assert_eq!(x, 55);
    std::mem::swap(&mut x, &mut y);
    assert_eq!((x, y), (89, 55));
    let one = (5,); // the comma makes it a tuple; (5) is just 5
    assert_eq!(one.0, 5);

    // Tuples compare lexicographically: the cheapest multi-key sort.
    let mut rows = vec![("ann", 30, 2), ("bob", 25, 1), ("cat", 30, 1)];
    rows.sort_by_key(|&(_, score, id)| (Reverse(score), id));
    assert_eq!(
        rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        ["cat", "ann", "bob"]
    );
    assert_eq!((1, "z").cmp(&(2, "a")), std::cmp::Ordering::Less);
    // Composite keys.
    let mut quota: HashMap<(u32, &str), u32> = HashMap::new();
    *quota.entry((1, "read")).or_default() += 1;
    assert_eq!(quota[&(1, "read")], 1);
    // enumerate/zip produce tuples; unzip splits them again.
    let (idx, vals): (Vec<usize>, Vec<char>) = "abc".chars().enumerate().unzip();
    assert_eq!((idx, vals), (vec![0, 1, 2], vec!['a', 'b', 'c']));
    let paired: Vec<(i32, &str)> = [1, 2].into_iter().zip(["a", "b", "c"]).collect(); // stops at the shorter
    assert_eq!(paired, [(1, "a"), (2, "b")]);
    // From<[T; N]> for tuples, and back (since 1.71): only for same-typed fields.
    let t: (u8, u8, u8) = [1, 2, 3].into();
    assert_eq!((t, <[u8; 3]>::from(t)), ((1, 2, 3), [1, 2, 3]));
    assert_eq!(<(u8, String, bool)>::default(), (0, String::new(), false));
    assert_ne!(format!("{:?}", Meters(1.0)), format!("{:?}", Seconds(1.0))); // and the compiler never mixes them
    println!("5. tuples ok");
}

// ---- 6. Unit () ------------------------------------------------------------------------------------------------
fn validate(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("empty name".into());
    }
    Ok(())
}

fn section_unit() {
    // () is the value of blocks, loops without a value, and statements.
    let u: () = {
        let _ = 1;
    };
    assert_eq!(u, ());
    assert_eq!(size_of::<()>(), 0);
    assert_eq!(validate("a"), Ok(()));
    // Collecting Results of () checks them all and stops at the first error, without building a Vec of data.
    let all: Result<(), String> = ["a", "b"].iter().try_for_each(|n| validate(n));
    assert!(all.is_ok());
    let collected: Result<(), String> = ["a", "", "c"].iter().map(|n| validate(n)).collect();
    assert_eq!(collected, Err("empty name".to_string()));
    // HashSet<T> is std's HashMap with () values: the value type costs nothing.
    let mut set: HashMap<&str, ()> = HashMap::new();
    assert_eq!(set.insert("k", ()), None);
    assert_eq!(set.insert("k", ()), Some(()));
    assert_eq!(size_of::<(u64, ())>(), size_of::<u64>());
    let units = vec![(); 1000]; // a Vec of zero-sized values never allocates
    assert_eq!((units.len(), units.capacity()), (1000, usize::MAX));
    println!("6. unit ok");
}

// ---- 7. Never: `!`, Infallible, empty enums --------------------------------------------------------------------
/// `-> !` means control never comes back. `!` coerces to any type, so it fits in any match arm.
fn fail(msg: &str) -> ! {
    panic!("{msg}")
}
fn digit_or_die(c: char) -> u32 {
    match c.to_digit(10) {
        Some(d) => d,
        None => fail("not a digit"), // type is `!`, accepted where u32 is expected
    }
}
struct Name(String);
impl FromStr for Name {
    type Err = Infallible; // every string is a valid name
    fn from_str(s: &str) -> Result<Self, Infallible> {
        Ok(Name(s.to_string()))
    }
}
/// An enum with no variants has no values.
enum Never {}
fn absurd<T>(n: Never) -> T {
    match n {} // no arms needed: this code can never run
}
/// Generic code that works for fallible and infallible sources alike.
fn load<E>(source: impl Fn() -> Result<u32, E>) -> Result<u32, E> {
    source().map(|v| v + 1)
}

fn section_never() {
    assert_eq!(digit_or_die('7'), 7);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let died = std::panic::catch_unwind(|| digit_or_die('x')).is_err();
    std::panic::set_hook(hook);
    assert!(died);
    // break, continue, return and `loop {}` without break also have type `!`.
    let vals: Vec<u32> = ["1", "x", "2"]
        .iter()
        .filter_map(|s| {
            let Ok(v) = s.parse() else { return None };
            Some(v)
        })
        .collect();
    assert_eq!(vals, [1, 2]);

    // Since 1.82 `Ok` alone is an exhaustive pattern for Result<T, Infallible>: no unwrap, no unreachable!.
    let Ok(name) = Name::from_str("ann");
    assert_eq!(name.0, "ann");
    let Ok(v) = load(|| Ok::<u32, Infallible>(1));
    assert_eq!(v, 2);
    assert_eq!(load(|| Err::<u32, &str>("io")), Err("io"));
    // Infallible is what TryFrom uses for conversions that cannot fail (blanket impl over From).
    let r: Result<u64, Infallible> = u64::try_from(5u32);
    let Ok(wide) = r;
    assert_eq!(wide, 5);
    assert_eq!((size_of::<Infallible>(), size_of::<Never>()), (0, 0));
    assert_eq!(size_of::<Result<u64, Infallible>>(), size_of::<u64>()); // the Err side costs nothing
    // std::hint::unreachable_unchecked() is also `!`, but reaching it is undefined behaviour: use unreachable!().
    println!("7. never types ok");
}

// ---- 8. Zero-sized types and PhantomData -----------------------------------------------------------------------
struct Open;
struct Closed;
/// Typestate: the state is a type parameter, so wrong-order calls do not compile.
struct Door<State> {
    opened_times: u32,
    _state: PhantomData<State>,
}
impl Door<Closed> {
    fn new() -> Self {
        Door {
            opened_times: 0,
            _state: PhantomData,
        }
    }
    fn open(self) -> Door<Open> {
        Door {
            opened_times: self.opened_times + 1,
            _state: PhantomData,
        }
    }
}
impl Door<Open> {
    fn close(self) -> Door<Closed> {
        Door {
            opened_times: self.opened_times,
            _state: PhantomData,
        }
    }
}

fn section_zst() {
    let d = Door::new().open().close().open().close();
    assert_eq!(d.opened_times, 2);
    // Door::new().close();   // does not compile: no `close` on Door<Closed>
    assert_eq!(size_of::<Door<Open>>(), size_of::<u32>());
    assert_eq!(
        (
            size_of::<PhantomData<()>>(),
            size_of::<PhantomData<[u64; 99]>>(),
            size_of::<[u64; 0]>(),
            size_of::<Open>()
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(align_of::<[u64; 0]>(), align_of::<u64>()); // zero size, but the alignment remains
    println!("8. zero-sized types ok");
}

// ---- 9. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. More than two or three tuple fields, or same-typed fields: use a struct with names.
    // 2. Large arrays live on the stack and are copied on move: Box them or use Vec (clippy::large_stack_arrays, pedantic).
    let boxed: Box<[u8]> = vec![0u8; 1 << 20].into_boxed_slice(); // built on the heap directly
    assert_eq!(boxed.len(), 1 << 20);
    // 3. Return Result<(), E> rather than bool, so callers get a reason and #[must_use] applies.
    // 4. Use [T; N] when the length is part of the type's meaning (keys, digests): wrong sizes fail at compile time.
    // 5. try_into at the boundary once, then pass &[u8; N] inward: no more length checks.
    // 6. A newtype per unit beats (f64, f64).
    // 7. `type Err = Infallible` is clearer than Result<T, ()> for "cannot fail" (clippy::result_unit_err).
    // 8. Index loops over arrays: iterate or zip (clippy::needless_range_loop).
    let (a, mut b) = ([1, 2, 3], [0; 3]);
    b.iter_mut().zip(a).for_each(|(o, i)| *o = i * 2);
    assert_eq!(b, [2, 4, 6]);
    println!("9. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_arrays();
    section_building();
    section_conversions();
    section_matrix();
    section_tuples();
    section_unit();
    section_never();
    section_zst();
    section_production_tips();
    println!(
        "covered {} array/tuple/unit/never names; all asserts passed",
        COVERED.len()
    );
}
