//! RECIPE 1006 (std): std::ops look-up page: arithmetic and bit operators, Index, Deref, Drop, closure traits, ranges,
//! Bound and ControlFlow.
//! LEVEL: intermediate
//! Use in: unit and vector types, permission flags, typed indexes, RAII guards, callback APIs, range queries.   Run: cargo run --release --example std_ops_traits_overloading
//! COMMON USES: money/duration/vector types, bit-flag sets, grid[(x, y)], guards that undo on drop, retry and visitor
//! callbacks, BTreeMap::range pagination, early exit from internal iteration.
//! METHODS: (everyday first) std::ops::Add, std::ops::AddAssign, std::ops::Sub, std::ops::Index, std::ops::IndexMut, std::ops::Drop, std::mem::drop,
//! METHODS: std::ops::Fn, std::ops::FnMut, std::ops::FnOnce, std::ops::Range, std::ops::RangeInclusive, std::ops::RangeFrom, std::ops::RangeTo,
//! METHODS: std::ops::RangeToInclusive, std::ops::RangeFull, Range::contains, Range::is_empty, RangeInclusive::start, RangeInclusive::end,
//! METHODS: RangeInclusive::into_inner, RangeInclusive::new, RangeBounds::start_bound, RangeBounds::end_bound, RangeBounds::contains,
//! METHODS: std::ops::Bound, Bound::map, Bound::cloned, Bound::as_ref, BTreeMap::range, std::ops::ControlFlow, ControlFlow::is_break,
//! METHODS: ControlFlow::is_continue, ControlFlow::break_value, ControlFlow::continue_value, ControlFlow::map_break, ControlFlow::map_continue,
//! METHODS: ControlFlow::break_ok, ControlFlow::continue_ok, Iterator::try_for_each, Iterator::try_fold, Iterator::step_by, Iterator::rev,
//! METHODS: std::ops::Mul, std::ops::Div, std::ops::Rem, std::ops::Neg, std::ops::Not, std::ops::BitAnd, std::ops::BitOr, std::ops::BitXor,
//! METHODS: std::ops::BitOrAssign, std::ops::BitAndAssign, std::ops::Shl, std::ops::Shr, std::ops::Deref, std::ops::DerefMut,
//! METHODS: std::iter::Sum, std::iter::Product, std::ops::AsyncFn
//! DOCS: https://doc.rust-lang.org/std/ops/index.html
//! DEFINED IN: core (`core::ops`, `core::iter::{Sum, Product}`), re-exported as std::ops. Usable in no_std. BTreeMap is alloc.
//! NOT COVERED (and why): the Try / FromResidual / Residual traits and implementing Fn* by hand (unstable), CoerceUnsized and
//! DispatchFromDyn (unstable), RangeBounds intersect/IntoBounds and OneSidedRange use (unstable or niche), the remaining
//! *Assign traits (same shape as AddAssign), the new `core::range` Range types beyond this mention, Coroutine (unstable).
//! Comparison operators come from PartialEq/PartialOrd: see std_cmp_hash_default_traits.
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   a + b on my type                          -> impl Add (by value; add &T forms if the type is not Copy)
//!   a += b                                    -> impl AddAssign (separate trait, not derived from Add)
//!   container[key]                            -> Index (+ IndexMut); it PANICS on a miss, also offer get() -> Option
//!   cleanup at scope end                      -> Drop
//!   accept a callback called once             -> impl FnOnce
//!   ... many times, may mutate its state      -> impl FnMut
//!   ... many times through &self (shareable)  -> impl Fn
//!   accept `..`, `a..`, `..=b` in my API      -> impl RangeBounds<T>
//!   keyset pagination                         -> map.range((Excluded(last), Unbounded))
//!   stop internal iteration early with a value-> try_for_each + ControlFlow::Break
//!   smart-pointer-like wrapper                -> Deref (ONLY for pointer-like types)
//!
//! Read: https://doc.rust-lang.org/std/ops/index.html , https://rust-lang.github.io/api-guidelines/predictability.html
//! TRY NEXT:
//!   - [S] Delete the `&Vector + &Vector` impl and find which line stops compiling and why.
//!   - [S] Change `_guard` to `_` in `section_drop` and predict the new log before running.
//!   - [M] Add `Sub` and `SubAssign` to `Cents` that return a checked result. Why can `Sub::Output` be Option but SubAssign cannot?
//!   - [M] Write `fn slice_of<R: RangeBounds<usize>>(s, r) -> Option<&[T]>` using to_bounds, and fuzz it against s.get(..).
//!   - [A] PROJECT: a permission system: bit flags from this page, parsed with FromStr (std_num_parsing_formatting), printed with
//!         the Binary traits (std_fmt_display_debug_deep).
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::ops::{
    Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, Bound, ControlFlow, Deref,
    DerefMut, Div, Index, IndexMut, Mul, Neg, Not, Range, RangeBounds, RangeFrom, RangeFull,
    RangeInclusive, RangeTo, RangeToInclusive, Rem, Shl, Shr, Sub,
};

const COVERED: &[&str] = &[
    "Add",
    "AddAssign",
    "Sub",
    "Index",
    "IndexMut",
    "Drop",
    "drop",
    "Fn",
    "FnMut",
    "FnOnce",
    "Range",
    "RangeInclusive",
    "RangeFrom",
    "RangeTo",
    "RangeToInclusive",
    "RangeFull",
    "contains",
    "is_empty",
    "start",
    "end",
    "into_inner",
    "new",
    "start_bound",
    "end_bound",
    "Bound",
    "map",
    "cloned",
    "as_ref",
    "range",
    "ControlFlow",
    "is_break",
    "is_continue",
    "break_value",
    "continue_value",
    "map_break",
    "map_continue",
    "break_ok",
    "continue_ok",
    "try_for_each",
    "try_fold",
    "step_by",
    "rev",
    "Mul",
    "Div",
    "Rem",
    "Neg",
    "Not",
    "BitAnd",
    "BitOr",
    "BitXor",
    "BitOrAssign",
    "BitAndAssign",
    "Shl",
    "Shr",
    "Deref",
    "DerefMut",
    "Sum",
    "Product",
    "AsyncFn",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

// ---- 1. Everyday core: Add / Sub / AddAssign -------------------------------------------------------------------
/// Money in cents. A newtype stops you adding cents to a user id.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Cents(i64);
impl Add for Cents {
    type Output = Cents;
    fn add(self, o: Cents) -> Cents {
        Cents(self.0 + o.0)
    }
}
impl Sub for Cents {
    type Output = Cents;
    fn sub(self, o: Cents) -> Cents {
        Cents(self.0 - o.0)
    }
}
impl AddAssign for Cents {
    fn add_assign(&mut self, o: Cents) {
        self.0 += o.0;
    }
}
/// Scaling: the right-hand side can be a different type.
impl Mul<i64> for Cents {
    type Output = Cents;
    fn mul(self, k: i64) -> Cents {
        Cents(self.0 * k)
    }
}
impl std::iter::Sum for Cents {
    fn sum<I: Iterator<Item = Cents>>(it: I) -> Cents {
        it.fold(Cents::default(), Add::add)
    }
}
impl<'a> std::iter::Sum<&'a Cents> for Cents {
    fn sum<I: Iterator<Item = &'a Cents>>(it: I) -> Cents {
        it.copied().sum()
    }
}

fn section_everyday() {
    let mut total = Cents(100) + Cents(250) - Cents(50);
    total += Cents(5);
    assert_eq!(total, Cents(305));
    assert_eq!(total * 2, Cents(610));
    let lines = [Cents(1), Cents(2), Cents(3)];
    assert_eq!(lines.iter().sum::<Cents>(), Cents(6)); // needs Sum<&Cents>
    assert_eq!(lines.into_iter().sum::<Cents>(), Cents(6));
    // Operators are method calls: `a + b` is Add::add(a, b) and MOVES non-Copy operands.
    assert_eq!(Add::add(Cents(1), Cents(1)), Cents(2));
    // The std example of a moving operator: String + &str reuses the left buffer.
    let s = String::from("ab") + "cd";
    assert_eq!(s, "abcd");
    println!("1. Add/Sub/AddAssign/Sum ok");
}

// ---- 2. Reference operands and forwarding macros ---------------------------------------------------------------
#[derive(Debug, Clone, PartialEq)]
struct Vector(Vec<f64>);
impl<'b> Add<&'b Vector> for &Vector {
    type Output = Vector;
    fn add(self, o: &'b Vector) -> Vector {
        assert_eq!(self.0.len(), o.0.len(), "dimension mismatch");
        Vector(self.0.iter().zip(&o.0).map(|(a, b)| a + b).collect())
    }
}
/// Write the real impl once on references, then forward the owned combinations to it.
macro_rules! forward_binop {
    ($tr:ident, $m:ident, $t:ty) => {
        impl $tr<$t> for $t {
            type Output = $t;
            fn $m(self, o: $t) -> $t {
                $tr::$m(&self, &o)
            }
        }
        impl $tr<&$t> for $t {
            type Output = $t;
            fn $m(self, o: &$t) -> $t {
                $tr::$m(&self, o)
            }
        }
        impl $tr<$t> for &$t {
            type Output = $t;
            fn $m(self, o: $t) -> $t {
                $tr::$m(self, &o)
            }
        }
    };
}
forward_binop!(Add, add, Vector);
/// `2.0 * v`: allowed by the orphan rule because Vector (a local type) appears as the trait's parameter.
impl Mul<&Vector> for f64 {
    type Output = Vector;
    fn mul(self, v: &Vector) -> Vector {
        Vector(v.0.iter().map(|x| self * x).collect())
    }
}
/// `impl Add for Vec<f64>` would be rejected (foreign trait, foreign type): wrap it in a newtype, as above.
struct Product3(i64);
impl std::iter::Product<i64> for Product3 {
    fn product<I: Iterator<Item = i64>>(it: I) -> Self {
        Product3(it.fold(1, |a, b| a * b))
    }
}

fn section_refs() {
    let (a, b) = (Vector(vec![1.0, 2.0]), Vector(vec![10.0, 20.0]));
    let want = Vector(vec![11.0, 22.0]);
    assert_eq!(&a + &b, want);
    assert_eq!(a.clone() + &b, want);
    assert_eq!(&a + b.clone(), want);
    assert_eq!(a.clone() + b.clone(), want);
    assert_eq!(2.0 * &a, Vector(vec![2.0, 4.0]));
    assert_eq!([2i64, 3, 4].into_iter().product::<Product3>().0, 24);
    assert!(panics(|| {
        let _ = &Vector(vec![1.0]) + &Vector(vec![]);
    }));
    println!("2. reference operands ok");
}

// ---- 3. Neg, Not, Div, Rem and bit operators -------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq)]
struct Perm(u8);
impl Perm {
    const NONE: Perm = Perm(0);
    const READ: Perm = Perm(1);
    const WRITE: Perm = Perm(2);
    const EXEC: Perm = Perm(4);
    const MASK: u8 = 7;
    fn has(self, p: Perm) -> bool {
        self & p == p
    }
}
macro_rules! bit_op {
    ($tr:ident, $m:ident, $op:tt) => {
        impl $tr for Perm {
            type Output = Perm;
            fn $m(self, o: Perm) -> Perm {
                Perm(self.0 $op o.0)
            }
        }
    };
}
bit_op!(BitOr, bitor, |);
bit_op!(BitAnd, bitand, &);
bit_op!(BitXor, bitxor, ^);
impl Not for Perm {
    type Output = Perm;
    fn not(self) -> Perm {
        Perm(!self.0 & Perm::MASK) // keep unused bits zero
    }
}
impl BitOrAssign for Perm {
    fn bitor_assign(&mut self, o: Perm) {
        self.0 |= o.0;
    }
}
impl BitAndAssign for Perm {
    fn bitand_assign(&mut self, o: Perm) {
        self.0 &= o.0;
    }
}
/// A position on a ring of N slots: Neg, Rem and Div with domain meaning.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Angle(i32);
impl Neg for Angle {
    type Output = Angle;
    fn neg(self) -> Angle {
        Angle((-self.0).rem_euclid(360))
    }
}
impl Div<i32> for Angle {
    type Output = Angle;
    fn div(self, k: i32) -> Angle {
        Angle(self.0 / k)
    }
}
impl Rem<i32> for Angle {
    type Output = Angle;
    fn rem(self, k: i32) -> Angle {
        Angle(self.0 % k)
    }
}
struct Bits(u32);
impl Shl<u32> for Bits {
    type Output = Bits;
    fn shl(self, n: u32) -> Bits {
        Bits(self.0.checked_shl(n).unwrap_or(0))
    }
}
impl Shr<u32> for Bits {
    type Output = Bits;
    fn shr(self, n: u32) -> Bits {
        Bits(self.0.checked_shr(n).unwrap_or(0))
    }
}

fn section_unary_and_bits() {
    let mut p = Perm::READ | Perm::WRITE;
    assert!(p.has(Perm::READ) && !p.has(Perm::EXEC) && p.has(Perm::NONE));
    p |= Perm::EXEC;
    p &= !Perm::WRITE;
    assert_eq!(p, Perm::READ | Perm::EXEC);
    assert_eq!(p ^ Perm::READ, Perm::EXEC);
    assert_eq!(!Perm::NONE, Perm(7));
    assert_eq!(
        (-Angle(90), Angle(90) / 2, Angle(370) % 360),
        (Angle(270), Angle(45), Angle(10))
    );
    assert_eq!(
        ((Bits(1) << 4).0, (Bits(16) >> 4).0, (Bits(1) << 40).0),
        (16, 1, 0)
    );
    // `!` is bitwise NOT on integers and logical NOT on bool.
    assert_eq!((!0u8, !5i32, !true), (255, -6, false));
    println!("3. unary and bit operators ok");
}

// ---- 4. Index and IndexMut -------------------------------------------------------------------------------------
struct Grid {
    w: usize,
    cells: Vec<u8>,
}
impl Grid {
    fn offset(&self, (x, y): (usize, usize)) -> Option<usize> {
        // Check x separately: x >= w would silently land on the next row.
        (x < self.w && y < self.cells.len() / self.w).then(|| y * self.w + x)
    }
    fn get(&self, at: (usize, usize)) -> Option<&u8> {
        self.offset(at).map(|i| &self.cells[i])
    }
}
impl Index<(usize, usize)> for Grid {
    type Output = u8;
    #[track_caller]
    fn index(&self, at: (usize, usize)) -> &u8 {
        let i = self.offset(at).expect("grid index out of bounds");
        &self.cells[i]
    }
}
impl IndexMut<(usize, usize)> for Grid {
    #[track_caller]
    fn index_mut(&mut self, at: (usize, usize)) -> &mut u8 {
        let i = self.offset(at).expect("grid index out of bounds");
        &mut self.cells[i]
    }
}

/// Accept every range type at once by being generic over std's SliceIndex (you can bound on it, not implement it).
struct Frame(Vec<u8>);
impl<R: std::slice::SliceIndex<[u8]>> Index<R> for Frame {
    type Output = R::Output;
    fn index(&self, r: R) -> &R::Output {
        &self.0[r]
    }
}

fn section_index() {
    let fr = Frame(vec![1, 2, 3, 4]);
    assert_eq!(
        (fr[0], &fr[1..3], &fr[..=1], &fr[2..]),
        (1, &[2, 3][..], &[1, 2][..], &[3, 4][..])
    );
    let mut g = Grid {
        w: 3,
        cells: vec![0; 6],
    };
    g[(2, 1)] = 9;
    g[(2, 1)] += 1;
    assert_eq!(
        (g[(2, 1)], g.get((2, 1)), g.get((3, 0)), g.get((0, 2))),
        (10, Some(&10), None, None)
    );
    assert!(panics(move || {
        let _ = g[(3, 0)];
    }));
    // HashMap and BTreeMap implement Index but NOT IndexMut: `map["new"] = 1` does not compile; use insert/entry.
    let m = BTreeMap::from([("a", 1)]);
    assert_eq!(m["a"], 1);
    assert!(panics(move || {
        let _ = m["zzz"];
    }));
    println!("4. Index/IndexMut ok");
}

// ---- 5. Range types --------------------------------------------------------------------------------------------
fn describe<R: RangeBounds<u32>>(r: R) -> String {
    let lo = match r.start_bound() {
        Included(a) => format!("[{a}"),
        Excluded(a) => format!("({a}"),
        Unbounded => "(-inf".into(),
    };
    let hi = match r.end_bound() {
        Included(b) => format!("{b}]"),
        Excluded(b) => format!("{b})"),
        Unbounded => "inf)".into(),
    };
    format!("{lo},{hi}")
}

fn section_ranges() {
    let (a, b, c, d, e, f): (
        Range<u32>,
        RangeInclusive<u32>,
        RangeFrom<u32>,
        RangeTo<u32>,
        RangeToInclusive<u32>,
        RangeFull,
    ) = (2..5, 2..=5, 2.., ..5, ..=5, ..);
    assert_eq!(
        [
            describe(a.clone()),
            describe(b.clone()),
            describe(c.clone()),
            describe(d),
            describe(e),
            describe(f)
        ],
        [
            "[2,5)",
            "[2,5]",
            "[2,inf)",
            "(-inf,5)",
            "(-inf,5]",
            "(-inf,inf)"
        ]
    );
    // A pair of Bounds is a range too: the only way to say "exclusive start".
    assert_eq!(describe((Excluded(2), Included(5))), "(2,5]");
    assert!(
        a.contains(&2)
            && !a.contains(&5)
            && b.contains(&5)
            && c.contains(&u32::MAX)
            && d.contains(&0)
            && e.contains(&5)
            && f.contains(&7)
    );
    assert!((0.0..1.0).contains(&0.5) && !(0.0..1.0).contains(&f64::NAN)); // works for any PartialOrd
    // Empty and reversed ranges iterate zero times: no panic, no counting down.
    #[allow(clippy::reversed_empty_ranges)]
    {
        assert!(
            (5..2).is_empty() && (5..5).is_empty() && !(5..=5).is_empty() && (6..=5).is_empty()
        );
        assert_eq!((5..2).count(), 0);
        assert!((0.0..f64::NAN).is_empty());
    }
    assert_eq!((0..10).step_by(3).collect::<Vec<_>>(), [0, 3, 6, 9]);
    assert_eq!((0..4).rev().collect::<Vec<_>>(), [3, 2, 1, 0]);
    assert_eq!((1..=3).rev().step_by(2).collect::<Vec<_>>(), [3, 1]);
    assert_eq!((0..=u8::MAX).count(), 256); // inclusive ranges can cover the whole type
    let r = RangeInclusive::new(1, 4);
    assert_eq!((*r.start(), *r.end()), (1, 4));
    assert_eq!(r.into_inner(), (1, 4));
    // Range<T> is itself the iterator, so it is not Copy: clone it (or keep (start, end)) to reuse it.

    // Slicing with every range type. Each panics when out of bounds or reversed.
    let v = [0, 1, 2, 3, 4];
    assert_eq!(v[1..3], [1, 2]);
    assert_eq!(v[1..=3], [1, 2, 3]);
    assert_eq!(v[3..], [3, 4]);
    assert_eq!(v[..2], [0, 1]);
    assert_eq!(v[..=2], [0, 1, 2]);
    assert_eq!(v[..], v);
    assert_eq!(v[(Excluded(1), Included(3))], [2, 3]);
    assert!(panics(move || {
        let _ = &v[..std::hint::black_box(6)];
    }));
    assert_eq!(v.get(4..2), None);
    // Bound helpers.
    let bound: Bound<&u32> = Included(&3);
    assert_eq!(bound.cloned(), Included(3));
    assert_eq!(Included(3).map(|x| x * 10), Included(30)); // since 1.77
    assert_eq!(
        Excluded(String::from("k")).as_ref().map(String::as_str),
        Excluded("k")
    );
    println!("5. ranges ok");
}

// ---- 6. BTreeMap::range ----------------------------------------------------------------------------------------
fn page(m: &BTreeMap<u32, &'static str>, after: Option<u32>, limit: usize) -> Vec<u32> {
    let start = after.map_or(Unbounded, Excluded); // Excluded(last seen id) = keyset pagination
    m.range((start, Unbounded))
        .take(limit)
        .map(|(k, _)| *k)
        .collect()
}

fn section_btree_range() {
    let m: BTreeMap<u32, &str> = [(10, "a"), (20, "b"), (30, "c"), (40, "d")].into();
    assert_eq!(page(&m, None, 3), [10, 20, 30]);
    assert_eq!(page(&m, Some(30), 3), [40]);
    assert_eq!(page(&m, Some(25), 1), [30]); // the cursor need not exist any more
    assert!(page(&m, Some(40), 3).is_empty());
    assert_eq!(m.range(..=25).next_back(), Some((&20, &"b"))); // floor: greatest key <= 25
    assert_eq!(m.range(25..).next(), Some((&30, &"c"))); // ceiling
    // Documented panics: start > end, or start == end with both Excluded.
    let m2 = m.clone();
    assert!(panics(move || {
        let _ = m2.range((Included(5), Excluded(std::hint::black_box(1))));
    }));
    assert!(panics(move || {
        let _ = m.range((Excluded(5), Excluded(std::hint::black_box(5))));
    }));
    println!("6. BTreeMap::range ok");
}

// ---- 7. Drop ---------------------------------------------------------------------------------------------------
type Log = RefCell<Vec<&'static str>>;
struct Guard<'a>(&'static str, &'a Log);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}
struct Pair<'a> {
    first: Guard<'a>,
    second: Guard<'a>,
}

fn section_drop() {
    let log = Log::default();
    {
        let _a = Guard("a", &log);
        let _b = Guard("b", &log);
        let _ = Guard("underscore", &log); // `_` does not bind: dropped at once
        let _guard = Guard("named-underscore", &log); // `_name` binds: lives to the end of the scope
        log.borrow_mut().push("end of scope");
    }
    // Locals drop in REVERSE declaration order.
    assert_eq!(
        *log.borrow(),
        ["underscore", "end of scope", "named-underscore", "b", "a"]
    );
    log.borrow_mut().clear();
    {
        let _p = Pair {
            first: Guard("field1", &log),
            second: Guard("field2", &log),
        };
    }
    assert_eq!(*log.borrow(), ["field1", "field2"]); // struct fields drop in DECLARATION order
    log.borrow_mut().clear();
    let early = Guard("early", &log);
    drop(early); // mem::drop just takes ownership
    #[allow(unused_assignments)] // the value is never read on purpose: we only watch when it drops
    let mut slot = Guard("old", &log);
    slot = Guard("new", &log); // assignment drops the previous value
    log.borrow_mut().push("after assign");
    drop(slot);
    assert_eq!(*log.borrow(), ["early", "old", "after assign", "new"]);
    // A type that implements Drop cannot be Copy, and you cannot move a field out of it by destructuring.
    println!("7. Drop order ok");
}

// ---- 8. Closure traits -----------------------------------------------------------------------------------------
fn apply_fn(f: impl Fn(u32) -> u32) -> u32 {
    f(f(1))
}
fn apply_mut(mut f: impl FnMut() -> u32) -> u32 {
    f();
    f()
}
fn apply_once(f: impl FnOnce() -> String) -> String {
    f()
}
async fn call_async(f: impl AsyncFn(u32) -> u32) -> u32 {
    f(1).await // AsyncFn* traits since 1.85 (not run here: std has no executor)
}
fn make_adder(n: u32) -> impl Fn(u32) -> u32 {
    move |x| x + n // `move` so the closure owns n and can outlive this frame
}

fn section_closures() {
    let k = 10;
    assert_eq!(apply_fn(|x| x + k), 21); // captures by shared reference
    let mut count = 0;
    assert_eq!(
        apply_mut(|| {
            count += 1; // mutates a capture: FnMut, not Fn
            count
        }),
        2
    );
    assert_eq!(count, 2);
    let owned = String::from("x");
    assert_eq!(apply_once(move || owned), "x"); // moves the capture OUT: callable once
    // Hierarchy: every Fn is FnMut, every FnMut is FnOnce. So ask for the WEAKEST bound you need.
    assert_eq!(apply_mut(|| 4), 4);
    assert_eq!(apply_once(|| "y".to_string()), "y");
    let boxed: Vec<Box<dyn Fn(u32) -> u32>> = vec![Box::new(make_adder(1)), Box::new(|x| x * 3)];
    assert_eq!(boxed.iter().fold(2, |acc, f| f(acc)), 9);
    let mut counter = {
        let mut n = 0;
        Box::new(move || {
            n += 1;
            n
        }) as Box<dyn FnMut() -> i32>
    };
    assert_eq!((counter(), counter()), (1, 2));
    let once: Box<dyn FnOnce() -> u8> = Box::new(|| 1);
    assert_eq!(once(), 1);
    // Non-capturing closures coerce to plain function pointers.
    let fp: fn(u32) -> u32 = |x| x + 1;
    assert_eq!(apply_fn(fp), 3);
    let _not_polled = call_async(async |x| x + 1);
    println!("8. closure traits ok");
}

// ---- 9. Deref --------------------------------------------------------------------------------------------------
/// A smart pointer that counts mutable accesses: a legitimate use of Deref.
struct Tracked<T> {
    value: T,
    writes: u32,
}
impl<T> Deref for Tracked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T> DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.writes += 1;
        &mut self.value
    }
}

fn section_deref() {
    let mut t = Tracked {
        value: vec![1, 2],
        writes: 0,
    };
    assert_eq!((t.len(), t[0], t.writes), (2, 1, 0)); // methods and indexing auto-deref
    t.push(3);
    assert_eq!(t.writes, 1);
    fn sum(s: &[i32]) -> i32 {
        s.iter().sum()
    }
    assert_eq!(sum(&t), 6); // deref coercion: &Tracked<Vec<i32>> -> &Vec<i32> -> &[i32]
    println!("9. Deref ok");
}

// ---- 10. ControlFlow -------------------------------------------------------------------------------------------
#[derive(Debug)]
enum Node {
    Leaf(i32),
    Branch(Vec<Node>),
}
/// A visitor where `?` propagates Break up through the recursion.
fn find_first(n: &Node, pred: &impl Fn(i32) -> bool, visited: &mut u32) -> ControlFlow<i32> {
    match n {
        Node::Leaf(v) => {
            *visited += 1;
            if pred(*v) {
                ControlFlow::Break(*v)
            } else {
                ControlFlow::Continue(())
            }
        }
        Node::Branch(children) => {
            for c in children {
                find_first(c, pred, visited)?;
            }
            ControlFlow::Continue(())
        }
    }
}

fn section_control_flow() {
    use Node::{Branch, Leaf};
    let tree = Branch(vec![Leaf(1), Branch(vec![Leaf(8), Leaf(3)]), Leaf(10)]);
    let mut visited = 0;
    let r = find_first(&tree, &|v| v > 5, &mut visited);
    assert_eq!((r, visited), (ControlFlow::Break(8), 2));
    assert!(r.is_break() && !r.is_continue());
    assert_eq!(r.break_value(), Some(8)); // since 1.83
    assert_eq!(r.continue_value(), None);
    assert_eq!(r.map_break(|v| v * 2), ControlFlow::Break(16));
    let c: ControlFlow<i32, u8> = ControlFlow::Continue(2);
    assert_eq!(c.map_continue(|x| x + 1), ControlFlow::Continue(3));
    assert_eq!((r.break_ok(), r.continue_ok()), (Ok(8), Err(8))); // since 1.96
    // try_for_each / try_fold stop at the first Break.
    let first_over = [3, 5, 9, 11].iter().try_for_each(|&x| {
        if x > 4 {
            ControlFlow::Break(x)
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(first_over, ControlFlow::Break(5));
    // "Sum until the budget is exhausted": Break carries the partial result.
    let budget = [4, 4, 4, 4].iter().try_fold(0, |acc, &x| {
        if acc + x > 10 {
            ControlFlow::Break(acc)
        } else {
            ControlFlow::Continue(acc + x)
        }
    });
    assert_eq!(budget, ControlFlow::Break(8));
    println!("10. ControlFlow ok");
}

// ---- 11. Production tips and tricks ----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Overload only when the meaning is obvious (API guidelines C-OVERLOAD); `+` must not do I/O or surprise.
    // 2. Deref only for smart pointers (C-DEREF). Using it as inheritance exposes every method of the inner type.
    // 3. If you implement Add, consider AddAssign too (clippy::assign_op_pattern suggests `a += b`).
    // 4. Check arithmetic in domain types: panic or wrap is a decision (checked_add returning Option is often better).
    assert_eq!(Cents(i64::MAX).0.checked_add(1), None);
    // 5. Pair every Index with a non-panicking get().
    // 6. Drop must not panic and cannot return errors: offer an explicit close()/commit() -> Result.
    // 7. RAII guards restore state on early return, `?` and panic alike.
    // 8. Never bind a lock or span guard to `_`: it is released immediately (clippy::let_underscore_lock).
    // 9. Accept impl RangeBounds<usize> instead of (start, end) pairs, and validate it.
    // 10. Prefer generic `impl Fn` for hot paths; Box<dyn Fn> when you must store mixed closures.
    // 11. suspicious operator impls (using `-` inside Add) are flagged by clippy::suspicious_arithmetic_impl.
    println!("11. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_refs();
    section_unary_and_bits();
    section_index();
    section_ranges();
    section_btree_range();
    section_drop();
    section_closures();
    section_deref();
    section_control_flow();
    section_production_tips();
    println!("covered {} ops names; all asserts passed", COVERED.len());
}
