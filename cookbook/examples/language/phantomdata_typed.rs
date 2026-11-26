//! RECIPE 104 (types): PhantomData: typed ids, units, permissions, variance, auto-traits, drop check.
//! LEVEL: intermediate
//! Use in: typed indices, units of measure, type-state APIs, FFI handle wrappers.   Run: cargo run --release --example phantomdata_typed
#![allow(dead_code)]
//!
//! WHAT: `PhantomData<T>` is a zero-sized field that tells the compiler "this type behaves as if it holds a T".
//! WHY: a struct may not have an unused type parameter (error E0392), and you often want a parameter that exists
//! only at compile time (a tag) or that really describes ownership of something held behind a raw pointer.
//! HOW: the choice of `PhantomData<..>` form controls three things: variance, auto-traits (Send/Sync) and drop check.
//!   - `PhantomData<T>`          covariant in T, Send/Sync as T, drop check: "I own and may drop a T".
//!   - `PhantomData<fn() -> T>`  covariant, ALWAYS Send+Sync, owns nothing (best for pure tags like ids).
//!   - `PhantomData<*const T>`   covariant, neither Send nor Sync (opts out of auto-traits).
//!   - `PhantomData<fn(T)>`      contravariant; `PhantomData<Cell<T>>` / `*mut T` invariant.
//! GOTCHA: `#[derive(Clone)]` on `Id<T>` adds `T: Clone`, so `Id<NotClone>` would not be Clone. Write impls by hand.
//! COMMON USES: typed arena/graph indices, units-of-measure wrappers, read-only vs read-write handles,
//!   raw-pointer containers (`Vec`-like types use PhantomData<T> for drop check), FFI handles tagged by the C type.
//! Read: https://doc.rust-lang.org/std/marker/struct.PhantomData.html and https://doc.rust-lang.org/nomicon/phantom-data.html
use std::cell::Cell;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::{Add, Mul, Sub};

// ---- 1. typed ids with manual impls (no bounds on T) ----
struct Id<T> {
    raw: u32,
    _tag: PhantomData<fn() -> T>,
}
impl<T> Id<T> {
    fn new(raw: u32) -> Self {
        Id {
            raw,
            _tag: PhantomData,
        }
    }
}
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> {
    fn eq(&self, o: &Self) -> bool {
        self.raw == o.raw
    }
}
impl<T> Eq for Id<T> {}
impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.raw.hash(h)
    }
}
impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Id<{}>({})",
            std::any::type_name::<T>().rsplit("::").next().unwrap(),
            self.raw
        )
    }
}
struct User; // deliberately NOT Clone/Debug/Hash/Eq
struct Order;

// ---- 2. units of measure ----
struct Meters;
struct Feet;
struct Length<U> {
    v: f64,
    _u: PhantomData<U>,
}
impl<U> Clone for Length<U> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<U> Copy for Length<U> {}
impl<U> Length<U> {
    fn new(v: f64) -> Self {
        Length { v, _u: PhantomData }
    }
}
impl<U> Add for Length<U> {
    type Output = Length<U>;
    fn add(self, o: Self) -> Self {
        Length::new(self.v + o.v)
    }
}
impl<U> Sub for Length<U> {
    type Output = Length<U>;
    fn sub(self, o: Self) -> Self {
        Length::new(self.v - o.v)
    }
}
impl<U> Mul<f64> for Length<U> {
    type Output = Length<U>;
    fn mul(self, k: f64) -> Self {
        Length::new(self.v * k)
    }
}
impl From<Length<Feet>> for Length<Meters> {
    fn from(f: Length<Feet>) -> Self {
        Length::new(f.v * 0.3048)
    }
}

// ---- 3. permission markers ----
struct ReadOnly;
struct ReadWrite;
struct Handle<P> {
    data: Vec<u8>,
    _p: PhantomData<P>,
}
impl<P> Handle<P> {
    fn read(&self, i: usize) -> u8 {
        self.data[i]
    }
}
impl Handle<ReadWrite> {
    fn write(&mut self, i: usize, b: u8) {
        self.data[i] = b;
    }
    fn freeze(self) -> Handle<ReadOnly> {
        Handle {
            data: self.data,
            _p: PhantomData,
        }
    }
}

// ---- 4. auto traits and 5. drop check ----
struct SendTag<T>(PhantomData<fn() -> T>);
struct NotSendTag<T>(PhantomData<*const T>);
fn assert_send_sync<X: Send + Sync>() {}

/// A raw-pointer owner: PhantomData<T> tells drop check it will drop a T.
struct MyBox<T> {
    ptr: *mut T,
    _own: PhantomData<T>,
}
impl<T> MyBox<T> {
    fn new(t: T) -> Self {
        MyBox {
            ptr: Box::into_raw(Box::new(t)),
            _own: PhantomData,
        }
    }
    fn get(&self) -> &T {
        // SAFETY: ptr came from Box::into_raw and is valid until drop.
        unsafe { &*self.ptr }
    }
}
impl<T> Drop for MyBox<T> {
    fn drop(&mut self) {
        // SAFETY: ptr came from Box::into_raw and is dropped exactly once here.
        unsafe { drop(Box::from_raw(self.ptr)) }
    }
}
// Variance: a covariant tag lets `Tag<&'static str>` shrink to `Tag<&'a str>`.
struct Cov<T>(PhantomData<fn() -> T>);
struct Inv<T>(PhantomData<Cell<T>>);
fn shrink<'a>(c: Cov<&'static str>) -> Cov<&'a str> {
    c // OK: covariant
}
// fn shrink_inv<'a>(c: Inv<&'static str>) -> Inv<&'a str> { c } // ERROR: lifetime may not live long enough (invariant)

fn main() {
    let a: Id<User> = Id::new(7);
    let b = a; // Copy even though User is not Clone
    assert_eq!(a, b);
    let mut set = HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
    println!("{:?} vs {:?}", a, Id::<Order>::new(7));
    // let o: Id<Order> = a;            // ERROR: expected Id<Order>, found Id<User>
    // assert_eq!(a, Id::<Order>::new(7)); // ERROR: mismatched types
    // With #[derive(Clone, Copy)] on Id<T>, `let b = a;` above would MOVE, since User: !Clone.
    assert_eq!(size_of::<Id<User>>(), size_of::<u32>());

    let m = Length::<Meters>::new(2.0) + Length::new(3.0) * 2.0 - Length::new(1.0);
    let f: Length<Meters> = Length::<Feet>::new(10.0).into();
    assert_eq!(m.v, 7.0);
    assert!((f.v - 3.048).abs() < 1e-9);
    // let bad = m + Length::<Feet>::new(1.0); // ERROR: expected Length<Meters>, found Length<Feet>
    assert_eq!(size_of::<Length<Meters>>(), size_of::<f64>());

    let mut h = Handle::<ReadWrite> {
        data: vec![0; 4],
        _p: PhantomData,
    };
    h.write(1, 9);
    let ro = h.freeze();
    assert_eq!(ro.read(1), 9);
    // ro.write(0, 1); // ERROR: no method `write` for Handle<ReadOnly>
    assert_eq!(size_of::<Handle<ReadOnly>>(), size_of::<Vec<u8>>());

    assert_send_sync::<SendTag<*const u8>>(); // fn() -> T is always Send+Sync
    // assert_send_sync::<NotSendTag<u8>>(); // ERROR: `*const u8` cannot be shared between threads safely
    let _ = NotSendTag::<u8>(PhantomData);
    assert_eq!(size_of::<NotSendTag<u64>>(), 0);

    let s: Cov<&'static str> = Cov(PhantomData);
    let _short = shrink(s);
    let _inv: Inv<u8> = Inv(PhantomData);

    let bx = MyBox::new(String::from("owned"));
    assert_eq!(bx.get(), "owned");
    // Drop check: because MyBox<T> has PhantomData<T> and a Drop impl, T must strictly outlive it, so a
    // MyBox<&'a X> whose X dies first is rejected rather than dropping a dangling reference.
    println!("phantomdata ok");
}
