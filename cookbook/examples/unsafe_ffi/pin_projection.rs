//! RECIPE 103 (unsafe): Pin, Unpin, PhantomPinned and structural pin projection (a hand-made `Future::map`).
//! LEVEL: advanced
//! Use in: writing future combinators, self-referential state machines, understanding what `async` blocks compile to.   Run: cargo run --release --example pin_projection
#![allow(dead_code)]
//!
//! Moving a value is a bitwise copy. If a value holds a pointer into ITSELF, a move leaves the pointer aiming at the
//! old location: dangling (UB on use). `Pin<P>` is a wrapper whose API never gives out `&mut T` to the pointee unless
//! `T: Unpin`, so a pinned `!Unpin` value cannot be moved (`mem::swap`, `mem::replace`, `Option::take` all need `&mut T`).
//! `Unpin` is an auto trait meaning "moving is harmless" (almost every type); `PhantomPinned` opts a struct out.
//! `async` blocks are `!Unpin` because their locals may borrow each other across `.await`.
//! Rules: once `Pin<&mut T>` of a `!Unpin` T exists, you may not move or invalidate that memory until T is dropped.
//! `get_unchecked_mut` is the escape hatch (safe only if you modify fields WITHOUT moving the value).
//! STRUCTURAL PINNING (projection): `Pin<&mut Wrapper>` -> `Pin<&mut Field>` is sound only if (1) the wrapper never
//! moves that field out, (2) the wrapper's Drop does not move it, (3) the wrapper is `Unpin` only if the field is,
//! (4) the layout is not `repr(packed)`. The `pin-project` crate generates this code and checks the rules for you.
//! Compile errors avoided here (shown as comments):
//!   let mut a = SelfRef::new("a"); let mut b = SelfRef::new("b"); std::mem::swap(&mut *a, &mut *b);
//!     -> error[E0277]: `PhantomPinned` cannot be unpinned (DerefMut needs Target: Unpin)
//!   let s: Pin<&mut Timer> = ...; let r: &mut Timer = s.get_mut();   -> same E0277; `Pin::new(&mut timer)` too.
//!   fut.poll(cx) on an unpinned `&mut F`                             -> error[E0599]: poll needs `Pin<&mut Self>`
//! COMMON USES: tokio/futures combinators (`Map`, `Then`, `Select`), intrusive linked lists whose nodes are pinned
//! (e.g. tokio's timer/notify waiter lists), generators/async state machines, FFI objects registered by address.
//! Read: https://doc.rust-lang.org/std/pin/index.html and https://doc.rust-lang.org/std/marker/struct.PhantomPinned.html
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::{Pin, pin};
use std::ptr::NonNull;
use std::task::{Context, Poll, Waker};

// ---- a self-referential struct, made safe by pinning ----
struct SelfRef {
    data: String,
    ptr: Option<NonNull<String>>, // points at `self.data`
    _pin: PhantomPinned,          // makes SelfRef !Unpin
}
impl SelfRef {
    fn new(s: &str) -> Pin<Box<SelfRef>> {
        // The pointer can only be set AFTER the value has reached its final heap address and is pinned.
        let mut boxed = Box::pin(SelfRef {
            data: s.to_string(),
            ptr: None,
            _pin: PhantomPinned,
        });
        let p = NonNull::from(&boxed.data);
        // SAFETY: we only assign a field; the SelfRef itself is not moved out of its Box.
        unsafe { boxed.as_mut().get_unchecked_mut().ptr = Some(p) };
        boxed
    }
    fn data(self: Pin<&Self>) -> &str {
        &self.get_ref().data
    }
    fn via_ptr(self: Pin<&Self>) -> &str {
        // SAFETY: `ptr` was set in `new` to this value's own `data` field. Pin guarantees the SelfRef has not moved
        // since, so the pointer is still valid, and `data` is never mutated after construction.
        unsafe { self.ptr.expect("set in new").as_ref() }
    }
}

// ---- a hand-written !Unpin future that checks it is never moved between polls ----
struct Timer {
    left: u32,
    first_addr: usize, // address observed on first poll; 0 = not polled yet
    polls: u32,
    _pin: PhantomPinned,
}
impl Timer {
    fn new(left: u32) -> Self {
        Timer {
            left,
            first_addr: 0,
            polls: 0,
            _pin: PhantomPinned,
        }
    }
}
impl Future for Timer {
    type Output = u32; // number of polls it took
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        // SAFETY: we only mutate plain fields and never move `*this` out of the pin.
        let this = unsafe { self.get_unchecked_mut() };
        let addr = this as *mut Timer as usize;
        if this.first_addr == 0 {
            this.first_addr = addr;
        }
        assert_eq!(this.first_addr, addr, "future moved after being polled!");
        this.polls += 1;
        if this.left == 0 {
            return Poll::Ready(this.polls);
        }
        this.left -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

// ---- MapFuture: `fut` is structurally pinned, `f` is not ----
struct MapFuture<F, G> {
    fut: F,
    f: Option<G>,
}
// No custom Drop, no manual Unpin impl: MapFuture<F, G> is Unpin exactly when F and G are (rules 2 and 3).
impl<F: Future, G: FnOnce(F::Output) -> T, T> Future for MapFuture<F, G> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        // SAFETY: projection of the pinned field. `fut` is never moved out of `self` and is dropped in place
        // (rule 1); the closure returns a `&mut` to the field only for the duration of this call.
        let fut = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.fut) };
        match fut.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(v) => {
                // SAFETY: `f` is NOT structurally pinned; we only take it out and never touch `fut` here.
                let f = unsafe { self.get_unchecked_mut() }
                    .f
                    .take()
                    .expect("polled after completion");
                Poll::Ready(f(v))
            }
        }
    }
}
trait FutureExt2: Future + Sized {
    fn map_with<G: FnOnce(Self::Output) -> T, T>(self, g: G) -> MapFuture<Self, G> {
        MapFuture {
            fut: self,
            f: Some(g),
        }
    }
}
impl<F: Future> FutureExt2 for F {}

fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut); // stack pinning: the future cannot be moved again
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

fn main() {
    let a = SelfRef::new("hello");
    let a_addr = &a.data as *const String as usize;
    assert_eq!(a.as_ref().via_ptr(), "hello");
    // Moving the Pin<Box> moves only the box pointer; the pointee (and thus the self-pointer) stays put.
    let holder = vec![a];
    assert_eq!(&holder[0].data as *const String as usize, a_addr);
    assert_eq!(holder[0].as_ref().via_ptr(), holder[0].as_ref().data());
    println!(
        "self-referential data stays valid: {}",
        holder[0].as_ref().via_ptr()
    );

    // Hand-written !Unpin future
    assert_eq!(block_on(Timer::new(3)), 4);
    // Map adapter over it; the closure runs once, on completion
    let mapped = Timer::new(2).map_with(|polls| format!("done after {polls} polls"));
    assert_eq!(block_on(mapped), "done after 3 polls");
    // Over an async block (also !Unpin), nested twice
    let nested = async { Timer::new(1).await + 10 }
        .map_with(|x| x * 2)
        .map_with(|x| x + 1);
    assert_eq!(block_on(nested), 25); // (2 + 10) * 2 + 1
    // Immediately-ready input
    assert_eq!(block_on(std::future::ready(5).map_with(|x| x + 1)), 6);

    // Boxing keeps the combinator pinned on the heap
    let boxed: Pin<Box<dyn Future<Output = String>>> =
        Box::pin(Timer::new(0).map_with(|n| n.to_string()));
    assert_eq!(block_on(boxed), "1");
    println!("MapFuture + Timer + async blocks all pinned correctly");
}
