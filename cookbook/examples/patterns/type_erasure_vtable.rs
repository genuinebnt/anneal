//! RECIPE 109 (patterns): type erasure side by side: `dyn Trait`, `dyn Any`, `Box<dyn Fn>`, `clone_box`, and a hand-written vtable.
//! LEVEL: advanced
//! Use in: heterogeneous collections, task queues, plugin systems, FFI callbacks.   Run: cargo run --release --example type_erasure_vtable
//! (unsafe part: also try `cargo +nightly miri run --example type_erasure_vtable`)
#![allow(dead_code)]
//!
//! Type erasure = forget the concrete type `T`, keep only what you need to DO with it. Rust offers several levels:
//!   1. `Box<dyn Trait>`: compiler-made vtable (drop, size, align, trait methods). The default choice.
//!   2. `Box<dyn Any>`: erase everything, recover by `downcast` using `TypeId` (see any_event_bus).
//!   3. `Box<dyn Fn..>`: a trait object for one callable method; the closure's captures are the erased data.
//!   4. `clone_box`: `dyn Trait` cannot require `Clone` (it returns `Self`, so it is not object safe), so add a method
//!      `fn clone_box(&self) -> Box<dyn Trait>` and `impl Clone for Box<dyn Trait>`. The crate https://github.com/dtolnay/dyn-clone
//!      automates this.
//!   5. A MANUAL vtable: `ErasedTask { data: *mut (), run, drop }`: two plain function pointers monomorphized per closure type.
//!      This is exactly what `RawWakerVTable` does for `Waker` (https://doc.rust-lang.org/std/task/struct.RawWakerVTable.html):
//!      a data pointer plus a table of functions that know the real type. Use it when you need a layout you control (FFI, no
//!      fat pointer, custom allocation), otherwise prefer `Box<dyn FnOnce()>`.
//! OBJECT SAFETY: a trait is `dyn`-compatible if methods have a receiver, do not return `Self`, and have no generic type
//! parameters. Escape hatch: add `where Self: Sized` to a method (generic or returning Self) and it is simply not callable on `dyn`.
//! COMMON USES: thread-pool job queues (`Box<dyn FnOnce() + Send>`), executors' `RawWaker`s, plugin registries,
//! `Box<dyn Error>`, GUI callback lists, cloneable strategy objects in configs.
//! Read: https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility , https://doc.rust-lang.org/std/any/trait.Any.html
use std::any::Any;
use std::cell::Cell;
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::rc::Rc;

// ---- 1 + object safety + 4 clone_box ----
trait Shape {
    fn area(&self) -> f64;
    fn clone_box(&self) -> Box<dyn Shape>;
    // Generic method: not object safe by itself, so opt out of the vtable.
    fn map_area<R>(&self, f: impl FnOnce(f64) -> R) -> R
    where
        Self: Sized,
    {
        f(self.area())
    }
    fn as_any(&self) -> &dyn Any;
}
impl Clone for Box<dyn Shape> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
#[derive(Clone)]
struct Square(f64);
#[derive(Clone)]
struct Circle(f64);
impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn clone_box(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.0 * self.0
    }
    fn clone_box(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---- 5 manual vtable ----
struct ErasedTask {
    data: *mut (),
    run: unsafe fn(*mut ()),
    drop: unsafe fn(*mut ()),
}

impl ErasedTask {
    fn new<F: FnOnce() + 'static>(f: F) -> Self {
        // One pair of these functions exists per concrete F (monomorphization): that is the "vtable".
        unsafe fn run_impl<F: FnOnce()>(p: *mut ()) {
            // SAFETY: `p` came from `Box::into_raw(Box<F>)` in `new` and is consumed exactly once
            // (ErasedTask::run / Drop never both call into it).
            let boxed = unsafe { Box::from_raw(p as *mut F) };
            (*boxed)();
        }
        unsafe fn drop_impl<F>(p: *mut ()) {
            // SAFETY: same provenance and single-use argument as `run_impl`.
            drop(unsafe { Box::from_raw(p as *mut F) });
        }
        ErasedTask {
            data: Box::into_raw(Box::new(f)) as *mut (),
            run: run_impl::<F>,
            drop: drop_impl::<F>,
        }
    }

    fn run(self) {
        // ManuallyDrop: our Drop must NOT also free the data after `run` consumed it.
        let this = ManuallyDrop::new(self);
        // SAFETY: `data` is a live Box<F> matching `run`; `this` is never dropped, so it is used once.
        unsafe { (this.run)(this.data) }
    }
}

impl Drop for ErasedTask {
    fn drop(&mut self) {
        // SAFETY: reached only if `run` was never called (run suppresses Drop), so `data` is still live and owned by us.
        unsafe { (self.drop)(self.data) }
    }
}

struct Counter(Rc<Cell<usize>>);
impl Drop for Counter {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn main() {
    // 1. dyn Trait + clone_box
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(2.0)), Box::new(Circle(1.0))];
    let copy = shapes.clone();
    let total: f64 = copy.iter().map(|s| s.area()).sum();
    assert_eq!(total, 7.0);
    assert_eq!(Square(3.0).map_area(|a| a as i32), 9); // generic method works on concrete types only
    // 2. dyn Any recovers the concrete type
    assert!(shapes[0].as_any().downcast_ref::<Square>().is_some());
    assert!(shapes[0].as_any().downcast_ref::<Circle>().is_none());
    // 3. Box<dyn Fn>
    let k = 10;
    let fs: Vec<Box<dyn Fn(i32) -> i32>> = vec![Box::new(|x| x + 1), Box::new(move |x| x * k)];
    assert_eq!(fs.iter().map(|f| f(3)).collect::<Vec<_>>(), vec![4, 30]);

    // 5. manual vtable: run path
    let ran = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    {
        let (r, guard) = (ran.clone(), Counter(drops.clone()));
        let t = ErasedTask::new(move || {
            r.set(r.get() + 1);
            let _g = &guard;
        });
        t.run();
    }
    assert_eq!((ran.get(), drops.get()), (1, 1)); // ran once, captures dropped once (no double free)

    // drop path: never run, still freed
    {
        let guard = Counter(drops.clone());
        let r = ran.clone();
        let _t = ErasedTask::new(move || {
            r.set(99);
            let _g = &guard;
        });
    }
    assert_eq!((ran.get(), drops.get()), (1, 2));

    // zero-sized closure
    ErasedTask::new(|| {}).run();

    // queue of 1000 tasks: run the even ones, drop the rest unrun
    let drops = Rc::new(Cell::new(0));
    let ran = Rc::new(Cell::new(0));
    let mut q: VecDeque<ErasedTask> = VecDeque::new();
    for _ in 0..1000 {
        let (g, r) = (Counter(drops.clone()), ran.clone());
        q.push_back(ErasedTask::new(move || {
            r.set(r.get() + 1);
            let _g = &g;
        }));
    }
    let mut i = 0;
    while let Some(t) = q.pop_front() {
        if i % 2 == 0 {
            t.run();
        } else {
            drop(t);
        }
        i += 1;
    }
    assert_eq!((ran.get(), drops.get()), (500, 1000));
    println!("type erasure OK: 1000 tasks, 500 run, all 1000 captures dropped exactly once");
}
