//! RECIPE 202 (rust-features): const evaluation and generics at the edge: tables, typed dimensions, const asserts, ZST tricks.
//! LEVEL: advanced
//! Use in: embedded/no-alloc code, protocol and crypto tables, unit/dimension safety, API design with zero-cost abstractions.   Run: cargo run --release --example const_and_generics_advanced
#![allow(dead_code)]
//!
//! What: `const fn` with loops builds lookup tables during compilation (no build.rs, no startup cost); const generics put sizes
//! in the type so a dimension mismatch is a compile error; `const { assert!(..) }` turns invariants into build failures;
//! zero-sized types carry state at the type level; blanket impls fake trait aliases; autoref method resolution fakes
//! specialisation. Why: errors move from runtime to compile time and the checks cost nothing at runtime.
//! How: each section is a small module-like block with asserts in `main`. Only STABLE features are used: arithmetic on const
//! parameters in types (`[T; N + 1]`, `where [(); N]:`) is `generic_const_exprs`, which is still unstable, so it is avoided and
//! the workaround patterns are shown instead.
//!
//! COMMON USES: CRC/hash/base64 tables, fixed-capacity buffers (heapless style), matrix/vector libraries, state machines
//! (typestate), hardware register wrappers, ID newtypes, compile-time config validation.
//! Read: https://doc.rust-lang.org/reference/const_eval.html
//! Read: https://doc.rust-lang.org/std/
use std::fmt::Debug;
use std::marker::PhantomData;

// ---------- 1. const fn lookup tables ----------
const fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        // `for` loops are not allowed in const fn (Iterator is not const), `while` is.
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}
static CRC_TABLE: [u32; 256] = crc32_table(); // computed by the compiler, lives in .rodata
const fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    let mut i = 0;
    while i < data.len() {
        crc = CRC_TABLE[((crc ^ data[i] as u32) & 0xFF) as usize] ^ (crc >> 8);
        i += 1;
    }
    !crc
}
const CHECK: u32 = crc32(b"123456789"); // the whole checksum is evaluated at compile time
const _: () = assert!(CHECK == 0xCBF4_3926); // the standard CRC-32 check value; a typo fails the BUILD

const fn sieve<const N: usize>() -> [bool; N] {
    let mut is_prime = [true; N];
    if N > 0 {
        is_prime[0] = false;
    }
    if N > 1 {
        is_prime[1] = false;
    }
    let mut i = 2;
    while i * i < N {
        if is_prime[i] {
            let mut j = i * i;
            while j < N {
                is_prime[j] = false;
                j += i;
            }
        }
        i += 1;
    }
    is_prime
}
const PRIMES: [bool; 100] = sieve::<100>();
const fn count_true(a: &[bool]) -> usize {
    let (mut i, mut n) = (0, 0);
    while i < a.len() {
        if a[i] {
            n += 1;
        }
        i += 1;
    }
    n
}

// const string parsing: parse "major.minor" at compile time (e.g. from `env!("CARGO_PKG_VERSION")` or `include_str!`).
const fn parse_u32(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.is_empty() {
        return None;
    }
    let (mut i, mut v) = (0, 0u32);
    while i < b.len() {
        let d = b[i];
        if !d.is_ascii_digit() {
            return None;
        }
        v = match v.checked_mul(10) {
            Some(x) => x,
            None => return None,
        };
        v += (d - b'0') as u32;
        i += 1;
    }
    Some(v)
}
const fn major(version: &str) -> u32 {
    let b = version.as_bytes();
    let mut end = 0;
    while end < b.len() && b[end] != b'.' {
        end += 1;
    }
    let (head, _) = b.split_at(end);
    match parse_u32(match str::from_utf8(head) {
        Ok(s) => s,
        Err(_) => "",
    }) {
        // const str::from_utf8: stable
        Some(v) => v,
        None => panic!("bad version"), // a panic in a const evaluation is a compile error
    }
}
const MAJOR: u32 = major("12.4.1");

// ---------- 2. const generics: typed matrix dimensions ----------
#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix<const R: usize, const C: usize>([[i32; C]; R]);
impl<const R: usize, const C: usize> Matrix<R, C> {
    const fn zero() -> Self {
        Matrix([[0; C]; R])
    }
    fn transpose(&self) -> Matrix<C, R> {
        let mut out = Matrix::<C, R>::zero();
        for r in 0..R {
            for c in 0..C {
                out.0[c][r] = self.0[r][c];
            }
        }
        out
    }
    // (R x C) * (C x K) = (R x K): the shared inner dimension C appears twice, so the compiler enforces the rule.
    fn mul<const K: usize>(&self, rhs: &Matrix<C, K>) -> Matrix<R, K> {
        let mut out = Matrix::<R, K>::zero();
        for r in 0..R {
            for k in 0..K {
                for c in 0..C {
                    out.0[r][k] += self.0[r][c] * rhs.0[c][k];
                }
            }
        }
        out
    }
}
// Compile error if inner dimensions differ:
//   let a = Matrix::<2, 3>::zero(); let b = Matrix::<2, 3>::zero();
//   a.mul(&b);
//   error[E0308]: mismatched types: expected `&Matrix<3, _>`, found `&Matrix<2, 3>`   (3 vs 2)

// ---------- 3. const assertions, associated consts, default const generics ----------
struct RingBuf<T, const N: usize = 8> {
    items: [Option<T>; N],
    head: usize,
    len: usize,
}
impl<T, const N: usize> RingBuf<T, N> {
    const CAPACITY: usize = N; // associated const
    const MASK: usize = N - 1;
    fn new() -> Self {
        const {
            assert!(
                N.is_power_of_two(),
                "RingBuf capacity must be a power of two"
            )
        } // post-mono check: fails at build time
        RingBuf {
            items: [const { None }; N],
            head: 0,
            len: 0,
        }
    }
    fn push(&mut self, v: T) -> Option<T> {
        let idx = (self.head + self.len) & Self::MASK;
        if self.len == N {
            let old = self.items[self.head].replace(v);
            self.head = (self.head + 1) & Self::MASK;
            old
        } else {
            self.items[idx] = Some(v);
            self.len += 1;
            None
        }
    }
}
// RingBuf::<u8, 6>::new();  // build error[E0080]: evaluation panicked: RingBuf capacity must be a power of two

// ---------- 4. impl<const N: usize> Trait for [T; N] ----------
trait Checksum {
    fn checksum(&self) -> u32;
}
impl<const N: usize> Checksum for [u8; N] {
    fn checksum(&self) -> u32 {
        crc32(self) + N as u32
    }
}
trait Describe {
    fn describe() -> String;
}
impl<T: Debug + Default, const N: usize> Describe for [T; N] {
    fn describe() -> String {
        format!("array of {N} x {:?}", T::default())
    }
}

// ---------- 5. generic_const_exprs workarounds ----------
// Unstable (don't write): `fn concat<const A: usize, const B: usize>(a: [u8; A], b: [u8; B]) -> [u8; A + B]`.
// Workaround 1: make the output size a separate parameter and check it with a const assert (caller names it).
fn concat<const A: usize, const B: usize, const OUT: usize>(a: [u8; A], b: [u8; B]) -> [u8; OUT] {
    const { assert!(A + B == OUT, "OUT must equal A + B") };
    let mut out = [0u8; OUT];
    out[..A].copy_from_slice(&a);
    out[A..].copy_from_slice(&b);
    out
}
// Workaround 2: put the arithmetic in an associated const of a trait implemented per size via a macro.
// Workaround 3: use slices / Vec at the boundary and fixed arrays inside; convert with `try_into`.

// ---------- 6. zero-sized types: typestate and units ----------
struct Meters;
struct Feet;
#[derive(Debug, Clone, Copy)]
struct Length<U>(f64, PhantomData<U>);
impl<U> Length<U> {
    fn new(v: f64) -> Self {
        Length(v, PhantomData)
    }
}
impl<U> std::ops::Add for Length<U> {
    type Output = Length<U>;
    fn add(self, o: Self) -> Self {
        Length::new(self.0 + o.0)
    }
}
impl From<Length<Feet>> for Length<Meters> {
    fn from(f: Length<Feet>) -> Self {
        Length::new(f.0 * 0.3048)
    }
}
// Length<Meters> + Length<Feet> is a compile error (mismatched types): the unit is in the type and free at runtime.
struct Open;
struct Closed;
struct Door<S>(PhantomData<S>);
impl Door<Closed> {
    fn new() -> Self {
        Door(PhantomData)
    }
    fn open(self) -> Door<Open> {
        Door(PhantomData)
    }
}
impl Door<Open> {
    fn walk_through(&self) -> &'static str {
        "welcome"
    }
}

// ---------- 7. trait alias by blanket impl ----------
// `trait alias` is unstable; this is the stable idiom: a trait with supertraits plus a blanket impl.
trait Key: Clone + Ord + std::hash::Hash + Debug {}
impl<T: Clone + Ord + std::hash::Hash + Debug> Key for T {}
fn dedup_sorted<K: Key>(mut v: Vec<K>) -> Vec<K> {
    v.sort();
    v.dedup();
    v
}

// ---------- 8. specialisation-like dispatch via autoref ----------
// `min_specialization`/`specialization` are unstable. Method resolution tries `self`, then `&self`: an INHERENT method on the
// wrapper beats a trait method, and a more-specific receiver type beats a more-general one. Works when T is known at the call site
// (macros or generic-free code), not inside a generic fn (resolution happens before monomorphisation).
struct Wrap<T>(T);
trait Fallback {
    fn kind(&self) -> &'static str {
        "generic"
    }
}
impl<T> Fallback for &Wrap<T> {}
impl Wrap<String> {
    fn kind(&self) -> &'static str {
        "string"
    }
} // inherent: wins when T = String
impl Wrap<u8> {
    fn kind(&self) -> &'static str {
        "byte"
    }
}

// ---------- 9. impl Trait vs dyn Trait vs generics ----------
// | need                               | generics `<T: Tr>` | arg `impl Tr` | return `impl Tr`  | `dyn Tr`            |
// | one concrete type per call site    | yes (monomorph.)   | yes (sugar)   | yes (hides type)  | n/a                 |
// | heterogeneous collection / plugins | no                 | no            | no (one type)     | yes (Vec<Box<dyn>>) |
// | explicit turbofish `f::<T>()`      | yes                | no            | n/a               | n/a                 |
// | code size                          | grows per type     | grows         | one body          | one body + vtable   |
// | speed                              | inlined/static     | static        | static            | indirect call       |
// | object safety required             | no                 | no            | no                | yes                 |
// Rule of thumb: generics/impl by default, `dyn` at plugin boundaries and to cut compile time/code size.
fn total_len(items: &[&dyn AsRef<str>]) -> usize {
    items.iter().map(|s| s.as_ref().len()).sum()
} // dyn: mixed String/&str
fn evens(limit: u32) -> impl Iterator<Item = u32> {
    (0..limit).filter(|n| n % 2 == 0)
} // return impl: hides the adaptor type
fn show_all<I: IntoIterator>(it: I) -> String
where
    I::Item: Debug,
{
    it.into_iter().map(|x| format!("{x:?}")).collect()
} // generic

fn main() {
    // tables
    println!(
        "crc32 = {CHECK:#010x}, primes below 100 = {}, major = {MAJOR}",
        count_true(&PRIMES)
    );
    assert_eq!(crc32(b"hello"), 0x3610_A686);
    assert_eq!(count_true(&PRIMES), 25);
    assert!(PRIMES[97] && !PRIMES[91]);
    assert_eq!((MAJOR, parse_u32("42x"), parse_u32("")), (12, None, None));
    // matrices
    let a = Matrix::<2, 3>([[1, 2, 3], [4, 5, 6]]);
    let b = a.transpose();
    let p: Matrix<2, 2> = a.mul(&b);
    assert_eq!(p.0, [[14, 32], [32, 77]]);
    assert_eq!(b.transpose(), a);
    // ring buffer, default const generic (N = 8)
    let mut rb: RingBuf<u32> = RingBuf::new();
    assert_eq!(RingBuf::<u32>::CAPACITY, 8);
    for i in 0..9 {
        rb.push(i);
    }
    assert_eq!(rb.push(100), Some(1)); // 0 was evicted by 8, 1 now
    let mut small = RingBuf::<char, 2>::new();
    assert_eq!(
        (small.push('a'), small.push('b'), small.push('c')),
        (None, None, Some('a'))
    );
    // trait for [T; N]
    assert_eq!([1u8, 2, 3].checksum(), crc32(&[1, 2, 3]) + 3);
    assert_eq!(<[i32; 4]>::describe(), "array of 4 x 0");
    // concat workaround
    assert_eq!(concat::<2, 3, 5>([1, 2], [3, 4, 5]), [1, 2, 3, 4, 5]);
    // ZSTs
    assert_eq!(size_of::<Length<Meters>>(), size_of::<f64>());
    assert_eq!(size_of::<Door<Open>>(), 0);
    let total: Length<Meters> = Length::<Meters>::new(1.0) + Length::<Feet>::new(10.0).into();
    assert!((total.0 - 4.048).abs() < 1e-9);
    assert_eq!(Door::new().open().walk_through(), "welcome");
    // trait alias
    assert_eq!(dedup_sorted(vec!["b", "a", "b"]), ["a", "b"]);
    // autoref specialisation
    assert_eq!(
        (
            Wrap(String::new()).kind(),
            Wrap(1u8).kind(),
            (&Wrap(1.5f32)).kind()
        ),
        ("string", "byte", "generic")
    );
    // decision table
    let owned = String::from("abc");
    assert_eq!(total_len(&[&owned, &"de"]), 5);
    assert_eq!(evens(7).collect::<Vec<_>>(), [0, 2, 4, 6]);
    assert_eq!(show_all([1, 2]), "12");
    println!("all const/generic checks passed");
}
