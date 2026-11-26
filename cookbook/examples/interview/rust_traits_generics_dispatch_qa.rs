//! RECIPE 602 (interview): Traits, generics, dyn dispatch, closures as traits, and the standard conversion/comparison traits.
//! LEVEL: advanced
//! Use in: interview preparation, API design reviews ("generic or dyn?"), explaining E0038/E0277/E0117 errors.   Run: cargo run --release --example rust_traits_generics_dispatch_qa
//! COMMON USES: plugin systems, callback registries, builder/config APIs, operator-overloaded numeric types, error and conversion plumbing.
#![allow(dead_code)]
//! Read: https://www.lurklurk.org/effective-rust/generics.html (Item 12: generics vs trait objects)
//! Read: https://quinedot.github.io/rust-learning/dyn-trait-vs.html
//! Read: https://www.techinterview.org/post/3233474457/rust-interview-questions-2025-ownership-borrowing-lifetimes-traits-async-await-error-handling-smart-pointers-concurrency/
//! Read: https://codeforgeek.com/rust-interview-questions/ and https://github.com/devinterview-io/rust-interview-questions
//!
//! QUESTION BANK
//! Q1. Generics vs `dyn Trait`: what is static vs dynamic dispatch and what is a fat pointer? [Medium] [asked: very common]
//! Q2. When do you choose generics and when trait objects? [Medium] [asked: very common]
//! Q3. What is object safety (dyn compatibility)? Which traits cannot be `dyn`? [Hard] [asked: very common]
//! Q4. `impl Trait` in argument and return position: how do they differ from generics and `Box<dyn>`? [Medium] [asked: very common]
//! Q5. Trait bounds, `where` clauses and conditional method availability. [Easy] [asked: common]
//! Q6. Associated types vs generic parameters on a trait (Iterator::Item vs From<T>). [Medium] [asked: very common]
//! Q7. What is the orphan rule? How does the newtype pattern work around it? [Medium] [asked: very common]
//! Q8. Blanket impls, default methods and supertraits. [Medium] [asked: common]
//! Q9. Fn vs FnMut vs FnOnce: which closure implements which? [Medium] [asked: very common]
//! Q10. Storing and returning closures: `Box<dyn Fn + Send + 'static>`, `impl Fn`, `Box<dyn FnOnce>`. [Medium] [asked: common]
//! Q11. How does operator overloading work? [Easy] [asked: common]
//! Q12. PartialEq vs Eq vs PartialOrd vs Ord: why is f64 not Ord? Derive vs manual. [Medium] [asked: very common]
//! Q13. Default, From/Into and TryFrom: how do they relate? [Easy] [asked: common]
//! Q14. AsRef vs Borrow vs Deref: what is the difference? [Hard] [asked: occasional]
//! Q15. What are `Sized` and `?Sized`? [Hard] [asked: common]
//! Q16. How does `dyn Any` downcasting work? [Medium] [asked: occasional]
//! Q17. What are marker traits (Copy, Send, Sync, Unpin)? [Medium] [asked: common]

use std::any::Any;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fmt::{self, Debug, Display};
use std::marker::{PhantomData, PhantomPinned};
use std::mem::{size_of, size_of_val};
use std::ops::{Add, AddAssign, Index, Mul, Neg};

// ---------------------------------------------------------------------------
// Q1. generics vs dyn
// ---------------------------------------------------------------------------
/// ANSWER: A generic function is MONOMORPHISED: the compiler generates one specialised copy per
/// concrete type, call targets are known statically, can be inlined, and `&T` is a thin pointer.
/// `&dyn Trait` is a FAT pointer {data ptr, vtable ptr}; a call loads the function address from the
/// vtable (an indirect call, usually not inlined) and one copy of the code serves all types. The
/// vtable also holds drop glue, size and alignment.
///
/// REALLY TESTING: do you know both mechanisms and their cost model.
/// WRONG: "dyn is slow" (an indirect call is a few ns; matters only in hot loops) or "generics are runtime".
/// FOLLOW-UP: what is the size of `Box<dyn Trait>`? Two words. Of `&dyn Trait`? Two words.
trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> String {
        "shape".into()
    }
}
struct Sq(f64);
struct Circle(f64);
impl Shape for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn name(&self) -> String {
        "sq".into()
    }
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.0 * self.0
    }
}
fn area_static<T: Shape>(s: &T) -> f64 {
    s.area() // monomorphised: area_static::<Sq> and area_static::<Circle> are separate functions
}
fn area_dyn(s: &dyn Shape) -> f64 {
    s.area() // one function, indirect call through the vtable
}
fn q1_static_vs_dynamic() {
    assert_eq!(area_static(&Sq(2.0)), 4.0);
    assert_eq!(area_dyn(&Circle(1.0)), 3.0);
    assert_eq!(size_of::<&Sq>(), size_of::<usize>());
    assert_eq!(size_of::<&dyn Shape>(), 2 * size_of::<usize>());
    assert_eq!(size_of::<Box<dyn Shape>>(), 2 * size_of::<usize>());
    // Two different monomorphisations are different functions:
    let f1: fn(&Sq) -> f64 = area_static::<Sq>;
    let f2: fn(&Circle) -> f64 = area_static::<Circle>;
    assert_ne!(f1 as usize, f2 as usize);
    // The size of the pointee is recorded in the vtable:
    let shapes: [&dyn Shape; 2] = [&Sq(1.0), &Circle(1.0)];
    assert_eq!(size_of_val(shapes[0]), 8);
    assert_eq!(
        shapes.iter().map(|s| s.name()).collect::<Vec<_>>(),
        ["sq", "shape"]
    );
}

// ---------------------------------------------------------------------------
// Q2. choosing
// ---------------------------------------------------------------------------
/// ANSWER: Choose GENERICS when types are known at compile time and speed matters (inlining,
/// zero-cost), when you need trait features that are not object-safe (returning `Self`, generic methods,
/// associated consts), or when you want to constrain two values to the SAME type. Choose `dyn Trait`
/// for HETEROGENEOUS collections (`Vec<Box<dyn Shape>>`), plugin/open sets chosen at runtime, to cut
/// code bloat and compile time, and to keep type signatures simple. Costs: generics => code size and
/// longer compiles; dyn => indirection, no inlining, heap often needed, restricted trait features.
/// Effective Rust item 12 states the same trade-off.
///
/// REALLY TESTING: engineering trade-off reasoning, not dogma.
/// WRONG: "always use generics for performance" / "always dyn to avoid bloat".
/// FOLLOW-UP: can you get the best of both? Yes: generic outer function, thin non-generic inner
/// function (`fn f<P: AsRef<Path>>(p: P) { inner(p.as_ref()) }`).
fn total_area_generic<T: Shape>(items: &[T]) -> f64 {
    items.iter().map(|s| s.area()).sum()
}
fn total_area_dyn(items: &[Box<dyn Shape>]) -> f64 {
    items.iter().map(|s| s.area()).sum()
}
fn pair_same<T: Shape>(_a: &T, _b: &T) {}
fn q2_choosing() {
    let squares = [Sq(1.0), Sq(2.0)];
    assert_eq!(total_area_generic(&squares), 5.0);
    // Heterogeneous: only dyn can do it.
    let mixed: Vec<Box<dyn Shape>> = vec![Box::new(Sq(1.0)), Box::new(Circle(1.0))];
    assert_eq!(total_area_dyn(&mixed), 4.0);
    // FAILING (E0308): generics force a single type: `[Sq(1.0), Circle(1.0)]` mixes types in an array.
    // FAILING (E0308): pair_same(&Sq(1.0), &Circle(1.0));  // expected `&Sq`, found `&Circle`
    pair_same(&Sq(1.0), &Sq(2.0));
    // Generic outer / non-generic inner trick (one copy of the heavy body):
    fn read_len<S: AsRef<str>>(s: S) -> usize {
        fn inner(s: &str) -> usize {
            s.len()
        }
        inner(s.as_ref())
    }
    assert_eq!(read_len("abc") + read_len(String::from("de")), 5);
}

// ---------------------------------------------------------------------------
// Q3. object safety
// ---------------------------------------------------------------------------
/// ANSWER: A trait can be used as `dyn Trait` only if the compiler can build a vtable and call every
/// method without knowing `Self`. It is NOT dyn-compatible when it: (1) requires `Self: Sized`;
/// (2) has a method returning `Self` or taking `Self` by value in args beyond the receiver
/// (unsized/unknown size); (3) has generic methods (infinite vtable); (4) has associated consts;
/// (5) has a static method without a receiver; (6) uses `Self` in supertrait bounds like `PartialEq`/`Clone`
/// (they mention `Self`). Fix by adding `where Self: Sized` to the offending methods (they are
/// then simply not callable on the trait object) or by a different signature (e.g. clone-box).
/// The error is E0038.
///
/// REALLY TESTING: do you know the rules and the escape hatches.
/// WRONG: "traits with generics cannot be objects" (generic TRAITS can; generic METHODS cannot).
/// FOLLOW-UP: how do you clone a `Box<dyn Trait>`? Add `fn box_clone(&self) -> Box<dyn Trait>`.
// FAILING (E0038): the trait cannot be made into an object
//   trait Bad {
//       const ID: u32;                 // associated const
//       fn dup(&self) -> Self;         // returns Self
//       fn gen<T: Debug>(&self, t: T); // generic method
//       fn make() -> u32;              // no receiver
//   }
//   let _: Box<dyn Bad>;               // error[E0038]: the trait `Bad` is not dyn compatible
trait Animal {
    fn name(&self) -> String;
    // Opt out of the vtable with `where Self: Sized`: still available on concrete types.
    fn dup(&self) -> Self
    where
        Self: Sized;
    fn describe<T: Debug>(&self, extra: T) -> String
    where
        Self: Sized,
    {
        format!("{} {:?}", self.name(), extra)
    }
    fn create() -> Self
    where
        Self: Sized;
    // Clone-box: the object-safe replacement for `dup`.
    fn box_clone(&self) -> Box<dyn Animal>;
}
#[derive(Clone)]
struct Dog(u8);
impl Animal for Dog {
    fn name(&self) -> String {
        format!("dog{}", self.0)
    }
    fn dup(&self) -> Self {
        self.clone()
    }
    fn create() -> Self {
        Dog(0)
    }
    fn box_clone(&self) -> Box<dyn Animal> {
        Box::new(self.clone())
    }
}
impl Clone for Box<dyn Animal> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
fn q3_object_safety() {
    let b: Box<dyn Animal> = Box::new(Dog(7)); // compiles thanks to the `where Self: Sized` escapes
    let c = b.clone(); // via box_clone
    assert_eq!(c.name(), "dog7");
    let d = Dog::create().dup();
    assert_eq!(d.describe(42), "dog0 42"); // generic method works on the concrete type
    // b.describe(1);      // error: the `describe` method cannot be invoked on a trait object
    // b.dup();            // same
}

// ---------------------------------------------------------------------------
// Q4. impl Trait
// ---------------------------------------------------------------------------
/// ANSWER: In ARGUMENT position `fn f(x: impl Display)` is sugar for an anonymous generic
/// (`fn f<T: Display>(x: T)`), still static dispatch (but callers cannot use turbofish). In RETURN
/// position `fn f() -> impl Iterator<Item = u8>` means "one specific concrete type that I do not
/// name", chosen by the callee and fixed per function; it is still static dispatch with no allocation.
/// Because it is ONE type, returning different types from different branches fails (E0308); use
/// `Box<dyn Trait>` (dynamic) or an enum/`Either` instead.
///
/// REALLY TESTING: the difference between "caller chooses" (arg) and "callee chooses" (return).
/// WRONG: "impl Trait return is dynamic dispatch / boxes".
/// FOLLOW-UP: can `impl Trait` appear in a trait method return? Yes (RPITIT, stable since 1.75).
fn show_all(items: &[impl Display]) -> String {
    items
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
fn evens(limit: u32) -> impl Iterator<Item = u32> {
    (0..limit).filter(|n| n % 2 == 0)
}
// FAILING (E0308): `if` and `else` have incompatible types (two distinct opaque/closure types)
//   fn pick(flag: bool) -> impl Iterator<Item = u32> {
//       if flag { (0..3).map(|x| x) } else { (0..3).filter(|_| true) }   // different types
//   }
// FIXED 1: Box<dyn Iterator> (allocation + dynamic dispatch)
fn pick_boxed(flag: bool) -> Box<dyn Iterator<Item = u32>> {
    if flag {
        Box::new(0..3)
    } else {
        Box::new((0..6).filter(|n| n % 2 == 1))
    }
}
// FIXED 2: make both branches one type
fn pick_same(flag: bool) -> impl Iterator<Item = u32> {
    (0..6u32).filter(move |n| flag || n % 2 == 1)
}
trait Source {
    fn items(&self) -> impl Iterator<Item = u32>; // return-position impl Trait in traits
}
struct Three;
impl Source for Three {
    fn items(&self) -> impl Iterator<Item = u32> {
        1..=3
    }
}
fn q4_impl_trait() {
    assert_eq!(show_all(&[1, 2, 3]), "1,2,3");
    assert_eq!(evens(7).collect::<Vec<_>>(), [0, 2, 4, 6]);
    assert_eq!(pick_boxed(true).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(pick_boxed(false).collect::<Vec<_>>(), [1, 3, 5]);
    assert_eq!(pick_same(true).count(), 6);
    assert_eq!(Three.items().sum::<u32>(), 6);
    // the opaque iterator has the size of the concrete adapter, not a box:
    assert!(size_of_val(&evens(7)) <= 16);
}

// ---------------------------------------------------------------------------
// Q5. bounds / where
// ---------------------------------------------------------------------------
/// ANSWER: A bound `T: A + B` restricts which types a generic accepts and what you may do with them.
/// `where` clauses are equivalent but allow bounds on arbitrary types (`Vec<T>: Debug`,
/// `for<'a> &'a T: Add`) and read better with many bounds. An `impl<T: Display> Wrapper<T> { .. }`
/// block makes methods available CONDITIONALLY, only when T satisfies the bound.
///
/// REALLY TESTING: reading and writing signatures, conditional impls.
/// WRONG: "bounds are checked at runtime".
/// FOLLOW-UP: what is a higher-ranked bound (`for<'a> Fn(&'a str) -> &'a str`)? Works for every lifetime.
struct Wrapper<T>(T);
impl<T> Wrapper<T> {
    fn inner(&self) -> &T {
        &self.0
    }
}
impl<T: Display> Wrapper<T> {
    fn show(&self) -> String {
        format!("[{}]", self.0)
    }
}
fn largest<T>(xs: &[T]) -> Option<&T>
where
    T: PartialOrd,
{
    let mut it = xs.iter();
    let mut best = it.next()?;
    for x in it {
        if x > best {
            best = x;
        }
    }
    Some(best)
}
fn sum_refs<T>(xs: &[T]) -> T
where
    T: Default,
    for<'a> &'a T: Add<&'a T, Output = T>, // bound on a reference type, only expressible in `where`
{
    let mut acc = T::default();
    for x in xs {
        acc = &acc + x;
    }
    acc
}
fn apply_to_all<F>(f: F, items: &[&str]) -> Vec<String>
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    items.iter().map(|s| f(s).to_string()).collect()
}
fn q5_bounds() {
    assert_eq!(Wrapper(5).show(), "[5]");
    assert_eq!(*Wrapper(vec![1]).inner(), vec![1]); // `show` unavailable for Vec<i32> (no Display)
    // Wrapper(vec![1]).show();   // error[E0599]: method `show` exists but its trait bounds were not satisfied
    assert_eq!(largest(&[3, 9, 2]), Some(&9));
    assert_eq!(largest(&[1.5, 0.5]), Some(&1.5));
    assert_eq!(largest::<i32>(&[]), None);
    assert_eq!(apply_to_all(|s| s.trim(), &[" a ", "b "]), ["a", "b"]);
    assert_eq!(sum_refs(&[1, 2, 3]), 6);
}

// ---------------------------------------------------------------------------
// Q6. associated types vs generic parameters
// ---------------------------------------------------------------------------
/// ANSWER: An associated type is an OUTPUT of the impl: each implementing type picks exactly one
/// (`impl Iterator for Counter { type Item = u32; }`), so callers do not repeat it and inference
/// is easy. A generic parameter is an INPUT: a type may implement the trait many times with
/// different parameters (`impl From<u8> for X` and `impl From<String> for X`). Use an associated type
/// when there is a single natural choice per implementor; use a parameter when several impls make sense.
///
/// REALLY TESTING: API design: "one impl per type" vs "many".
/// WRONG: "they are interchangeable".
/// FOLLOW-UP: can a type implement `Iterator` twice with different Items? No (coherence).
trait Container {
    type Item; // one per container
    fn first(&self) -> Option<&Self::Item>;
}
struct Bag(Vec<String>);
impl Container for Bag {
    type Item = String;
    fn first(&self) -> Option<&String> {
        self.0.first()
    }
}
fn first_len<C: Container<Item = String>>(c: &C) -> usize {
    c.first().map_or(0, |s| s.len())
}
trait Convert<T> {
    // many per type
    fn convert(&self) -> T;
}
struct Meters(u32);
impl Convert<u64> for Meters {
    fn convert(&self) -> u64 {
        self.0 as u64
    }
}
impl Convert<String> for Meters {
    fn convert(&self) -> String {
        format!("{}m", self.0)
    }
}
fn q6_assoc_vs_generic() {
    assert_eq!(first_len(&Bag(vec!["abc".into()])), 3);
    let m = Meters(5);
    let a: u64 = m.convert(); // the target type selects the impl
    let s: String = m.convert();
    assert_eq!((a, s.as_str()), (5, "5m"));
    // let x = m.convert();   // error[E0283]: type annotations needed (ambiguous: input parameter)
    struct Counter(u32);
    impl Iterator for Counter {
        type Item = u32;
        fn next(&mut self) -> Option<u32> {
            self.0 += 1;
            (self.0 <= 3).then_some(self.0)
        }
    }
    assert_eq!(Counter(0).collect::<Vec<_>>(), [1, 2, 3]); // Item is inferred, never written
}

// ---------------------------------------------------------------------------
// Q7. orphan rule / newtype
// ---------------------------------------------------------------------------
/// ANSWER: You may implement a trait for a type only if the trait OR the type is local to your
/// crate. So `impl Display for Vec<String>` is rejected (E0117: only traits defined in the current
/// crate can be implemented for types defined outside of the crate). The rule prevents two crates
/// from providing conflicting impls (coherence). Workaround: the NEWTYPE pattern, a local tuple
/// struct wrapping the foreign type (zero runtime cost), possibly with `Deref` to forward methods.
///
/// REALLY TESTING: coherence, and a typical practical pattern.
/// WRONG: "you can never add behaviour to std types" (extension traits and newtypes can).
/// FOLLOW-UP: what is an extension trait? A local trait implemented for a foreign type.
// FAILING (E0117): impl Display for Vec<String> { ... }
struct Names(Vec<String>);
impl Display for Names {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.0.join(", "))
    }
}
impl std::ops::Deref for Names {
    type Target = Vec<String>;
    fn deref(&self) -> &Vec<String> {
        &self.0
    }
}
trait StrExt {
    fn shout(&self) -> String;
}
impl StrExt for str {
    // local trait on foreign type: allowed
    fn shout(&self) -> String {
        self.to_uppercase() + "!"
    }
}
fn q7_orphan_rule() {
    let n = Names(vec!["a".into(), "b".into()]);
    assert_eq!(n.to_string(), "[a, b]");
    assert_eq!(n.len(), 2); // Deref forwards Vec methods
    assert_eq!("hey".shout(), "HEY!");
    assert_eq!(
        size_of::<Names>(),
        size_of::<Vec<String>>(),
        "newtype is free"
    );
}

// ---------------------------------------------------------------------------
// Q8. blanket impls, default methods, supertraits
// ---------------------------------------------------------------------------
/// ANSWER: A blanket impl implements a trait for every type meeting a bound
/// (`impl<T: Display> Describe for T`; std does `impl<T: Display> ToString for T` and
/// `impl<T, U: From<T>> Into<U> for T`). Default methods supply behaviour in the trait that
/// implementors may override. A supertrait (`trait Pet: Animal + Display`) requires every implementor to
/// also implement the others and lets default methods call them; it is NOT inheritance of data.
/// Coherence forbids overlapping impls.
///
/// REALLY TESTING: how std's conversions work and trait composition vs inheritance.
/// WRONG: "supertraits are like class inheritance".
/// FOLLOW-UP: can you upcast `&dyn Pet` to `&dyn Animal`? Yes, trait upcasting is stable since 1.86.
trait Describe {
    fn describe(&self) -> String;
}
impl<T: Display> Describe for T {
    fn describe(&self) -> String {
        format!("<{self}>")
    }
}
trait Named {
    fn name(&self) -> String;
}
trait Pet: Named + Display {
    fn intro(&self) -> String {
        // default method using both supertraits
        format!("{} says {}", self.name(), self)
    }
}
struct Cat;
impl Named for Cat {
    fn name(&self) -> String {
        "cat".into()
    }
}
impl Display for Cat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "meow")
    }
}
impl Pet for Cat {}
fn q8_blanket_default_super() {
    assert_eq!(5.describe(), "<5>");
    assert_eq!("x".describe(), "<x>");
    assert_eq!(Cat.intro(), "cat says meow");
    let p: &dyn Pet = &Cat;
    let n: &dyn Named = p; // trait upcasting
    assert_eq!(n.name(), "cat");
}

// ---------------------------------------------------------------------------
// Q9. Fn / FnMut / FnOnce
// ---------------------------------------------------------------------------
/// ANSWER: The compiler implements the traits by HOW THE BODY USES CAPTURES. `Fn`: only reads
/// captures (callable many times through `&self`). `FnMut`: mutates captures (`&mut self`).
/// `FnOnce`: moves a capture out/consumes it (`self`). They form a hierarchy: `Fn: FnMut: FnOnce`, so
/// every closure is `FnOnce`, and an `Fn` closure can be passed where `FnMut`/`FnOnce` is expected.
/// `move` changes HOW captured (by value) not which trait: a `move` closure that only reads is
/// still `Fn`. Accept the weakest bound you need (`FnOnce` if you call once).
///
/// REALLY TESTING: the capture-mode vs trait distinction (most candidates conflate `move` with `FnOnce`).
/// WRONG: "move closures are FnOnce".
/// FOLLOW-UP: why can you not call an FnOnce twice? The first call consumes the closure (E0382).
fn call_fn(f: impl Fn() -> usize) -> usize {
    f() + f()
}
fn call_mut(mut f: impl FnMut() -> usize) -> usize {
    f() + f()
}
fn call_once(f: impl FnOnce() -> String) -> String {
    f()
}
fn q9_fn_traits() {
    let name = String::from("ab");
    let reads = || name.len(); // borrows: Fn
    assert_eq!(call_fn(reads), 4);
    assert_eq!(call_mut(reads), 4); // Fn is also FnMut
    assert_eq!(call_once(|| name.clone()), "ab"); // and FnOnce

    let mut count = 0;
    let bumps = || {
        count += 1;
        count
    }; // FnMut
    assert_eq!(call_mut(bumps), 3); // 1 + 2
    assert_eq!(count, 2);
    // call_fn(|| { count += 1; count })   // error[E0594]: cannot assign to `count`, as it is a captured variable in a `Fn` closure

    let owned = String::from("moved");
    let consumes = move || owned; // returns captured String by value: FnOnce only
    assert_eq!(call_once(consumes), "moved");
    // call_once(consumes);                 // error[E0382]: use of moved value: `consumes`

    let s = String::from("cap");
    let mv = move || s.len(); // `move` but only reads: still Fn
    assert_eq!(call_fn(mv), 6);
    // `fn` pointers implement all three:
    fn plain() -> usize {
        1
    }
    assert_eq!(call_fn(plain), 2);
}

// ---------------------------------------------------------------------------
// Q10. storing / returning closures
// ---------------------------------------------------------------------------
/// ANSWER: Every closure has its own anonymous type, so a collection of different closures needs
/// type erasure: `Vec<Box<dyn Fn(i32) -> i32 + Send + 'static>>`. `Send` lets it move to another
/// thread, `'static` (default for Box<dyn>) says it holds no short borrows. Return ONE closure with
/// `impl Fn(i32) -> i32` (static, no alloc; needs `move` when it captures locals, E0373 otherwise)
/// and several different ones with `Box<dyn Fn>`. `Box<dyn FnOnce()>` works directly since 1.35 (the
/// box is consumed by the call; before that you needed the `FnBox` workaround) which is how thread
/// pools store jobs.
///
/// REALLY TESTING: type erasure for callbacks, thread-pool job queues.
/// WRONG: "closures are function pointers" (only non-capturing ones coerce to `fn`).
/// FOLLOW-UP: when `FnMut` in a Box: `Box<dyn FnMut()>` needs a `mut` binding to call.
fn adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n // FAILING without `move` (E0373): closure may outlive the current function, but it borrows `n`
}
fn op(kind: &str) -> Box<dyn Fn(i32) -> i32 + Send + 'static> {
    match kind {
        "double" => Box::new(|x| x * 2),
        "neg" => Box::new(|x| -x),
        _ => Box::new(|x| x),
    }
}
fn q10_storing_closures() {
    let add5 = adder(5);
    assert_eq!(add5(1), 6);
    let pipeline: Vec<Box<dyn Fn(i32) -> i32 + Send>> =
        vec![op("double"), op("neg"), Box::new(adder(100))];
    assert_eq!(pipeline.iter().fold(4, |acc, f| f(acc)), 92);
    // moved to another thread because of `Send`:
    let handle = std::thread::spawn(move || pipeline.iter().fold(1, |a, f| f(a)));
    assert_eq!(handle.join().unwrap(), 98);

    // Box<dyn FnOnce>: a job queue.
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut jobs: Vec<Box<dyn FnOnce() + Send>> = Vec::new();
    for i in 0..3 {
        let log = log.clone();
        let name = format!("job{i}");
        jobs.push(Box::new(move || log.lock().unwrap().push(name))); // moves `name` out: FnOnce
    }
    for j in jobs {
        j(); // consumes the box
    }
    assert_eq!(*log.lock().unwrap(), ["job0", "job1", "job2"]);

    let mut counter = 0;
    let mut tick: Box<dyn FnMut() -> i32> = Box::new(move || {
        counter += 1;
        counter
    });
    assert_eq!((tick(), tick()), (1, 2));
    // Only non-capturing closures coerce to fn pointers:
    let fp: fn(i32) -> i32 = |x| x + 1;
    assert_eq!(fp(1), 2);
}

// ---------------------------------------------------------------------------
// Q11. operator overloading
// ---------------------------------------------------------------------------
/// ANSWER: Operators are sugar for traits in `std::ops`: `a + b` is `Add::add(a, b)`, `-a` is `Neg`,
/// `a += b` is `AddAssign`, `a[i]` is `Index`, `*` is `Mul`. Implement them on your own types (orphan rule
/// applies: `impl Mul<V2> for f64` is legal because V2 is local, `impl Add for Vec<i32>` is not). `Output`
/// is an associated type so `Vector * f64` may return a different type. `==` and `<` are
/// `PartialEq`/`PartialOrd`, not `std::ops`. Operators take operands BY VALUE, so implement them for `&T`
/// too (or make T `Copy`) to avoid moving the operands.
///
/// REALLY TESTING: whether you know the trait mapping and the by-value consequence.
/// WRONG: "Rust has no operator overloading".
/// FOLLOW-UP: `&a + &b`: implement `Add for &V`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct V2 {
    x: f64,
    y: f64,
}
impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2 {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}
impl Mul<f64> for V2 {
    type Output = V2;
    fn mul(self, k: f64) -> V2 {
        V2 {
            x: self.x * k,
            y: self.y * k,
        }
    }
}
impl Mul<V2> for f64 {
    // legal: V2 is a local type parameter
    type Output = V2;
    fn mul(self, v: V2) -> V2 {
        v * self
    }
}
impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        V2 {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl<'a> Add<&'a V2> for &'a V2 {
    type Output = V2;
    fn add(self, o: &V2) -> V2 {
        *self + *o
    }
}
struct Grid(Vec<i32>);
impl Index<usize> for Grid {
    type Output = i32;
    fn index(&self, i: usize) -> &i32 {
        &self.0[i]
    }
}
fn q11_operators() {
    let a = V2 { x: 1.0, y: 2.0 };
    let b = V2 { x: 3.0, y: 4.0 };
    assert_eq!(a + b, V2 { x: 4.0, y: 6.0 });
    assert_eq!(a * 2.0, 2.0 * a);
    assert_eq!(-a, V2 { x: -1.0, y: -2.0 });
    let mut c = a;
    c += b;
    assert_eq!(c, a + b);
    assert_eq!(&a + &b, c);
    assert_eq!(Grid(vec![5, 6])[1], 6);
    // For non-Copy operands `a + b` moves them: String + &str consumes the left side.
    let s = String::from("a");
    let t = s + "b";
    assert_eq!(t, "ab");
    // String + String is E0308 (needs &str): `t + t2` -> `t + &t2`
}

// ---------------------------------------------------------------------------
// Q12. equality and ordering
// ---------------------------------------------------------------------------
/// ANSWER: `PartialEq` is `==` that may not be reflexive; `Eq` is a marker promising `a == a` (full
/// equivalence). `PartialOrd` gives `partial_cmp -> Option<Ordering>` (some pairs incomparable);
/// `Ord` demands a total order (`cmp -> Ordering`) consistent with `Eq`. `f64` has NaN (`NaN != NaN`,
/// `partial_cmp` is None), so it is `PartialEq + PartialOrd` only: not `Eq`/`Ord`/`Hash`, hence it cannot be a
/// `HashMap` key, be `sort()`ed or live in a `BTreeSet`. Use `sort_by(f64::total_cmp)`, `partial_cmp().unwrap()`
/// or a wrapper. `derive` compares FIELD BY FIELD in declaration order (lexicographic); implement manually
/// for custom keys. `Ord`, `PartialOrd`, `Eq`, `PartialEq` and `Hash` must agree.
///
/// REALLY TESTING: why floats are special, and the derive semantics.
/// WRONG: "f64 is not Ord because of rounding" (it is NaN).
/// FOLLOW-UP: how do you get a min-heap from `BinaryHeap`? `Reverse<T>` or a reversed `Ord`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct Version {
    major: u32,
    minor: u32,
} // derive: compares major, then minor
#[derive(Debug, PartialEq, Eq)]
struct ByLen(String);
impl PartialOrd for ByLen {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for ByLen {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.len().cmp(&o.0.len()).then_with(|| self.0.cmp(&o.0)) // consistent with Eq
    }
}
fn q12_eq_ord() {
    let nan = f64::NAN;
    assert!(nan != nan);
    assert_eq!(nan.partial_cmp(&1.0), None);
    assert!(!(nan < 1.0) && !(nan > 1.0) && !(nan == 1.0));
    // FAILING (E0277): let mut v = vec![2.0, 1.0]; v.sort();    // `f64: Ord` is not satisfied
    // FAILING (E0599): let m: HashMap<f64, u8> = HashMap::new(); m.insert(1.0, 1);   // insert exists but `f64: Eq + Hash` is not satisfied
    let mut v = vec![2.5, -1.0, 9.0, f64::NAN, 0.0];
    v.sort_by(f64::total_cmp); // total order: NaN sorts last
    assert_eq!(&v[..4], [-1.0, 0.0, 2.5, 9.0]);
    assert!(v[4].is_nan());
    let mut w = vec![3.0, 1.0, 2.0];
    w.sort_by(|a, b| a.partial_cmp(b).unwrap()); // fine when NaN is excluded
    assert_eq!(w, [1.0, 2.0, 3.0]);

    assert!(Version { major: 1, minor: 9 } < Version { major: 2, minor: 0 });
    assert_eq!(
        Version { major: 1, minor: 2 }.cmp(&Version { major: 1, minor: 2 }),
        Ordering::Equal
    );
    let set: BTreeSet<ByLen> = ["ccc", "a", "bb", "aa"]
        .iter()
        .map(|s| ByLen(s.to_string()))
        .collect();
    assert_eq!(
        set.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(),
        ["a", "aa", "bb", "ccc"]
    );
    assert_eq!(
        Version { major: 1, minor: 0 }
            .max(Version { major: 0, minor: 9 })
            .major,
        1
    );
}

// ---------------------------------------------------------------------------
// Q13. Default / From / Into / TryFrom
// ---------------------------------------------------------------------------
/// ANSWER: `Default::default()` builds a "zero/empty" value (derive when every field is Default;
/// combine with struct-update syntax `..Default::default()`). `From<T> for U` is the infallible
/// conversion; implement `From`, and `Into<U> for T` comes for free through a blanket impl (so accept
/// `impl Into<String>` in parameters, implement `From`, never implement `Into` by hand). `TryFrom`/`TryInto` are
/// the fallible versions returning `Result<_, Self::Error>`. `?` uses `From` to convert error types.
///
/// REALLY TESTING: idiomatic conversions, `From` vs `Into` direction.
/// WRONG: "implement Into for your type" (implement From).
/// FOLLOW-UP: `as` vs `try_from` for integer narrowing (as truncates silently).
#[derive(Debug, Default, PartialEq)]
struct Config {
    name: String,
    retries: u32,
    verbose: bool,
}
struct Celsius(f64);
struct Fahrenheit(f64);
impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}
#[derive(Debug, PartialEq)]
struct Even(u32);
impl TryFrom<u32> for Even {
    type Error = String;
    fn try_from(n: u32) -> Result<Self, String> {
        if n % 2 == 0 {
            Ok(Even(n))
        } else {
            Err(format!("{n} is odd"))
        }
    }
}
fn greet(name: impl Into<String>) -> String {
    let n: String = name.into();
    format!("hi {n}")
}
fn q13_default_from_into() {
    let c = Config {
        retries: 3,
        ..Default::default()
    };
    assert_eq!(
        c,
        Config {
            name: String::new(),
            retries: 3,
            verbose: false
        }
    );
    let f: Fahrenheit = Celsius(100.0).into(); // Into from the blanket impl
    assert_eq!(f.0, 212.0);
    assert_eq!(Even::try_from(4), Ok(Even(4)));
    let e: Result<Even, _> = 5u32.try_into();
    assert_eq!(e, Err("5 is odd".to_string()));
    assert_eq!(greet("a"), "hi a");
    assert_eq!(greet(String::from("b")), "hi b");
    assert_eq!(u8::try_from(300i32).is_err(), true);
    assert_eq!(300i32 as u8, 44, "`as` silently truncates");
    assert_eq!(i64::from(7i32), 7); // lossless widening via From
}

// ---------------------------------------------------------------------------
// Q14. AsRef / Borrow / Deref
// ---------------------------------------------------------------------------
/// ANSWER: `AsRef<T>` is a cheap reference-to-reference conversion for generic APIs (`fn f(p: impl
/// AsRef<Path>)` accepts &str, String, PathBuf). `Borrow<T>` is stricter: the borrowed form must hash
/// and compare IDENTICALLY to the owned form, which is what lets `HashMap<String, V>::get(&str)` work.
/// `Deref` is the smart-pointer trait that powers auto-deref and deref coercion (`&String -> &str`,
/// `&Box<T> -> &T`) and the `*` operator. Do not use `Deref` to emulate inheritance.
///
/// REALLY TESTING: why `map.get("literal")` works with String keys.
/// WRONG: "AsRef and Borrow are the same" (Borrow has the Eq/Hash consistency contract).
/// FOLLOW-UP: why does `HashMap<String, V>::get` take `&Q where String: Borrow<Q>`?
fn total_len<S: AsRef<str>>(items: &[S]) -> usize {
    items.iter().map(|s| s.as_ref().len()).sum()
}
fn lookup<K, Q>(m: &HashMap<K, i32>, k: &Q) -> Option<i32>
where
    K: Borrow<Q> + std::hash::Hash + Eq,
    Q: std::hash::Hash + Eq + ?Sized,
{
    m.get(k).copied()
}
fn q14_asref_borrow_deref() {
    assert_eq!(total_len(&["a", "bc"]), 3);
    assert_eq!(total_len(&[String::from("abc")]), 3);
    let mut m: HashMap<String, i32> = HashMap::new();
    m.insert("key".to_string(), 7);
    assert_eq!(m.get("key"), Some(&7)); // &str lookup, no allocation of a String
    assert_eq!(lookup(&m, "key"), Some(7));
    // Deref coercion chain: &Box<String> -> &String -> &str
    let b = Box::new(String::from("deref"));
    fn len(s: &str) -> usize {
        s.len()
    }
    assert_eq!(len(&b), 5);
    let path_like: &std::path::Path = "a/b".as_ref();
    assert_eq!(path_like.components().count(), 2);
}

// ---------------------------------------------------------------------------
// Q15. Sized / ?Sized
// ---------------------------------------------------------------------------
/// ANSWER: `Sized` means the size is known at compile time. Every generic parameter has an implicit
/// `T: Sized` bound because locals, arguments and returns need a fixed size. Dynamically sized types
/// (DSTs) are `str`, `[T]` and `dyn Trait`; they exist only behind a pointer (`&str`, `Box<[T]>`) that
/// carries the extra length/vtable (fat pointer). `T: ?Sized` relaxes the bound so a generic can accept
/// DSTs (as `&T`/`Box<T>`). `Box<dyn Trait>` works because `Box<T: ?Sized>`.
///
/// REALLY TESTING: understanding of fat pointers and the implicit bound.
/// WRONG: "str is a slice of Strings".
/// FOLLOW-UP: why can't you write `let s: str = *"abc";`? Unknown size (E0277).
fn describe_size<T: ?Sized + Debug>(t: &T) -> (String, usize) {
    (format!("{t:?}"), size_of_val(t))
}
fn q15_sized() {
    assert_eq!(describe_size("héllo"), ("\"héllo\"".to_string(), 6)); // str: size = byte length
    assert_eq!(
        describe_size(&[1u16, 2, 3][..]),
        ("[1, 2, 3]".to_string(), 6)
    );
    assert_eq!(describe_size(&7u64), ("7".to_string(), 8));
    let dynv: &dyn Debug = &5u8;
    assert_eq!(describe_size(dynv), ("5".to_string(), 1));
    // FAILING (E0277) without ?Sized:  fn bad<T: Debug>(t: &T) {}  bad("abc")  // `str` is not Sized
    assert_eq!(size_of::<&str>(), 2 * size_of::<usize>());
    assert_eq!(size_of::<&[u8]>(), 2 * size_of::<usize>());
    let boxed: Box<[i32]> = vec![1, 2, 3].into_boxed_slice();
    assert_eq!(size_of_val(&*boxed), 12);
}

// ---------------------------------------------------------------------------
// Q16. dyn Any
// ---------------------------------------------------------------------------
/// ANSWER: `Any` is implemented for every `'static` type and exposes `type_id()`. `&dyn Any` /
/// `Box<dyn Any>` erase the concrete type but let you test and recover it: `downcast_ref::<T>()`,
/// `downcast_mut`, `is::<T>()`, `Box<dyn Any>::downcast::<T>()` (returns the box back on failure).
/// Used for plugin registries, heterogeneous maps (type-map), and panic payloads. It is a last resort:
/// prefer enums or a trait with the needed methods because downcasting moves errors to runtime.
///
/// REALLY TESTING: dynamic typing escape hatch and its `'static` requirement.
/// WRONG: "Any works for non-'static types" / "Any can call methods of the type".
/// FOLLOW-UP: how does `catch_unwind` give you the panic message? `Box<dyn Any + Send>` downcast to `&str`/`String`.
fn q16_any() {
    let items: Vec<Box<dyn Any>> = vec![
        Box::new(5i32),
        Box::new(String::from("s")),
        Box::new(1.5f64),
    ];
    let mut ints = 0;
    let mut strs = 0;
    for it in &items {
        if let Some(i) = it.downcast_ref::<i32>() {
            ints += i;
        } else if it.is::<String>() {
            strs += 1;
        }
    }
    assert_eq!((ints, strs), (5, 1));
    let b: Box<dyn Any> = Box::new(String::from("own"));
    match b.downcast::<i32>() {
        Ok(_) => panic!("wrong type"),
        Err(b) => assert_eq!(*b.downcast::<String>().unwrap(), "own"),
    }
    assert_ne!(std::any::TypeId::of::<i32>(), std::any::TypeId::of::<u32>());
    // panic payloads are Box<dyn Any + Send>
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(|| panic!("payload text"));
    std::panic::set_hook(prev);
    let payload = r.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"payload text"));
}

// ---------------------------------------------------------------------------
// Q17. marker traits
// ---------------------------------------------------------------------------
/// ANSWER: Marker traits have no methods; they only record a property the compiler uses. `Copy`: bitwise
/// copy is a valid duplicate (requires Clone, no Drop). `Send`/`Sync`: thread-transfer / sharing (auto traits).
/// `Sized`: known size. `Unpin`: the type does not rely on its address staying fixed, so `Pin<&mut T>` can be
/// freely moved out (auto trait; `PhantomPinned` opts out; async state machines are `!Unpin`). `Eq` is a marker on
/// top of `PartialEq`. Auto traits are implemented automatically when all fields have them; `unsafe impl` to override.
///
/// REALLY TESTING: do you know that traits can carry guarantees without code.
/// WRONG: "Unpin means the type cannot be pinned".
/// FOLLOW-UP: why does `Pin<Box<T>>` give a stable address? The heap allocation never moves; `!Unpin` stops moving out.
struct P<T>(PhantomData<T>);
trait NotUnpin {
    const UNPIN: bool = false;
}
impl<T> NotUnpin for P<T> {}
impl<T: Unpin> P<T> {
    const UNPIN: bool = true;
}
trait NotCopy {
    const COPY: bool = false;
}
impl<T> NotCopy for P<T> {}
impl<T: Copy> P<T> {
    const COPY: bool = true;
}
fn q17_marker_traits() {
    assert!(P::<i32>::COPY && P::<&str>::COPY && P::<(u8, f32)>::COPY);
    assert!(!P::<String>::COPY && !P::<Vec<u8>>::COPY && !P::<&mut u8>::COPY);
    assert!(P::<String>::UNPIN && P::<Box<i32>>::UNPIN);
    assert!(!P::<PhantomPinned>::UNPIN);
    struct SelfRef {
        data: String,
        _pin: PhantomPinned,
    }
    assert!(
        !P::<SelfRef>::UNPIN,
        "one !Unpin field makes the struct !Unpin"
    );
    // Any async block is !Unpin: pin it before polling (see the async QA file).
    let fut = async { 1 };
    fn unpin_of<T>(_: &T) -> bool {
        P::<T>::UNPIN
    }
    assert!(!unpin_of(&fut));
    let pinned = Box::pin(fut);
    assert!(
        P::<std::pin::Pin<Box<()>>>::UNPIN,
        "Pin<Box<T>> is Unpin: the box pointer can move, the target cannot"
    );
    drop(pinned);
}

fn main() {
    q1_static_vs_dynamic();
    println!("Q1: ok");
    q2_choosing();
    println!("Q2: ok");
    q3_object_safety();
    println!("Q3: ok");
    q4_impl_trait();
    println!("Q4: ok");
    q5_bounds();
    println!("Q5: ok");
    q6_assoc_vs_generic();
    println!("Q6: ok");
    q7_orphan_rule();
    println!("Q7: ok");
    q8_blanket_default_super();
    println!("Q8: ok");
    q9_fn_traits();
    println!("Q9: ok");
    q10_storing_closures();
    println!("Q10: ok");
    q11_operators();
    println!("Q11: ok");
    q12_eq_ord();
    println!("Q12: ok");
    q13_default_from_into();
    println!("Q13: ok");
    q14_asref_borrow_deref();
    println!("Q14: ok");
    q15_sized();
    println!("Q15: ok");
    q16_any();
    println!("Q16: ok");
    q17_marker_traits();
    println!("Q17: ok");
}
