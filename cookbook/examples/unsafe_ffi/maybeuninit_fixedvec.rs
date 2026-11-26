//! RECIPE 100 (unsafe): MaybeUninit and ManuallyDrop: an inline, heap-free `FixedVec<T, N>`.
//! LEVEL: advanced
//! Use in: no-alloc / embedded code, small-vector optimisation, understanding how Vec and arrays are built.   Run: cargo run --release --example maybeuninit_fixedvec
#![allow(dead_code)]
//!
//! `[T; N]` must be fully initialised, so a growable inline buffer needs `[MaybeUninit<T>; N]`: memory that may hold
//! garbage and that the compiler will never drop or read for you. The container's INVARIANT is the whole proof:
//!     slots `0..len` are initialised, slots `len..N` are not.
//! Every unsafe block below just cites that invariant. `Drop` must drop exactly the `0..len` prefix.
//! `ManuallyDrop<T>` suppresses the automatic drop of a field, which lets a `Drop` impl (or a consuming method)
//! move a field out exactly once without a double drop. `mem::forget` is safe (leaking is not UB) and skips Drop.
//! `std::array::from_fn` is the SAFE way to build `[T; N]` from an index closure; MaybeUninit is the manual version.
//! Panic safety: if one element's `drop` panics, `drop_in_place` on a slice still drops the remaining elements.
//! COMMON USES: small-vector crates (smallvec/arrayvec style), ring buffers on embedded targets, arena slots,
//! the guts of `Vec`, `Option`-free object pools.
//! Read: https://doc.rust-lang.org/std/mem/union.MaybeUninit.html and https://doc.rust-lang.org/nomicon/
use std::mem::{self, ManuallyDrop, MaybeUninit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

struct FixedVec<T, const N: usize> {
    data: [MaybeUninit<T>; N],
    len: usize, // INVARIANT: data[..len] initialised, data[len..] uninitialised, len <= N
}

impl<T, const N: usize> FixedVec<T, N> {
    fn new() -> Self {
        Self {
            data: [const { MaybeUninit::uninit() }; N],
            len: 0,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    /// Err(value) gives the value back when full.
    fn push(&mut self, v: T) -> Result<(), T> {
        if self.len == N {
            return Err(v);
        }
        self.data[self.len].write(v); // safe: writing into MaybeUninit never drops the old (garbage) contents
        self.len += 1;
        Ok(())
    }
    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1; // shrink first: slot is no longer part of the initialised prefix
        // SAFETY: slot `len` was initialised (it was last in the prefix) and is now excluded, so reading moves
        // ownership out exactly once.
        Some(unsafe { self.data[self.len].assume_init_read() })
    }
    fn as_slice(&self) -> &[T] {
        // SAFETY: MaybeUninit<T> has T's layout; the first `len` slots are initialised per the invariant.
        unsafe { std::slice::from_raw_parts(self.data.as_ptr().cast::<T>(), self.len) }
    }
    fn get(&self, i: usize) -> Option<&T> {
        self.as_slice().get(i)
    }
    fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }
}

impl<T, const N: usize> Drop for FixedVec<T, N> {
    fn drop(&mut self) {
        let n = self.len;
        self.len = 0; // keep the invariant even if an element's drop panics
        // SAFETY: the first `n` slots are initialised and are dropped exactly once here; the array of
        // MaybeUninit fields that follows is a no-op to drop. A slice drop continues past a panicking element.
        unsafe {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(
                self.data.as_mut_ptr().cast::<T>(),
                n,
            ))
        }
    }
}

// ---- drop tracking ----
static CREATED: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicUsize = AtomicUsize::new(0);
struct Tracked {
    id: u64,
    panic_on_drop: bool,
}
impl Tracked {
    fn new(id: u64) -> Self {
        CREATED.fetch_add(1, SeqCst);
        Tracked {
            id,
            panic_on_drop: false,
        }
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, SeqCst);
        if self.panic_on_drop {
            panic!("drop of {} panicked", self.id);
        }
    }
}

// ---- ManuallyDrop: move a field out of a type that implements Drop ----
struct Handle {
    inner: ManuallyDrop<Tracked>,
}
impl Handle {
    /// Consuming method: `Handle: Drop` forbids `self.inner` moves, so we disarm Drop first.
    fn into_inner(self) -> Tracked {
        let mut this = ManuallyDrop::new(self); // Handle::drop will never run
        // SAFETY: `this` is never used again and never dropped, so `inner` is taken exactly once.
        unsafe { ManuallyDrop::take(&mut this.inner) }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: Drop runs once and `inner` is not touched afterwards (the field's own drop is suppressed).
        let t = unsafe { ManuallyDrop::take(&mut self.inner) };
        println!(
            "  Handle::drop took ownership of {} and drops it explicitly",
            t.id
        );
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn balanced() -> bool {
    CREATED.load(SeqCst) == DROPPED.load(SeqCst)
}

fn main() {
    // Safe array init vs manual MaybeUninit
    let safe: [String; 4] = std::array::from_fn(|i| format!("s{i}"));
    let mut raw: [MaybeUninit<String>; 4] = [const { MaybeUninit::uninit() }; 4];
    for (i, slot) in raw.iter_mut().enumerate() {
        slot.write(format!("s{i}"));
    }
    // SAFETY: all four slots were just initialised; [MaybeUninit<String>; 4] and [String; 4] share a layout.
    let manual: [String; 4] = unsafe { mem::transmute(raw) };
    assert_eq!(safe, manual);
    println!("from_fn and MaybeUninit agree: {manual:?}");

    // basics
    let mut fv: FixedVec<String, 3> = FixedVec::new();
    for w in ["a", "b", "c"] {
        fv.push(w.to_string()).unwrap();
    }
    assert_eq!(fv.push("d".into()), Err("d".to_string()));
    assert_eq!(fv.iter().cloned().collect::<String>(), "abc");
    assert_eq!(fv.pop().as_deref(), Some("c"));
    assert_eq!(fv.get(1).map(String::as_str), Some("b"));
    assert!(fv.get(2).is_none());
    drop(fv);

    // ManuallyDrop and forget
    let h = Handle {
        inner: ManuallyDrop::new(Tracked::new(1)),
    };
    let t = h.into_inner(); // no Handle::drop output
    drop(t);
    drop(Handle {
        inner: ManuallyDrop::new(Tracked::new(2)),
    });
    assert!(balanced());
    mem::forget(Tracked::new(3)); // leaked on purpose: created but never dropped
    assert_eq!(CREATED.load(SeqCst), DROPPED.load(SeqCst) + 1);
    DROPPED.fetch_add(1, SeqCst); // re-balance the books for the randomized test below

    // Randomised differential test against Vec with exact drop accounting
    let mut rng = Rng(0xDEADBEEFCAFEF00D);
    let mut next_id = 0;
    for _round in 0..300 {
        let mut fv: FixedVec<Tracked, 8> = FixedVec::new();
        let mut model: Vec<u64> = Vec::new();
        for _ in 0..60 {
            match rng.next() % 3 {
                0 | 1 => {
                    next_id += 1;
                    let r = fv.push(Tracked::new(next_id)); // a rejected value is returned and dropped here
                    if model.len() < 8 {
                        assert!(r.is_ok());
                        model.push(next_id);
                    } else {
                        assert!(r.is_err());
                    }
                }
                _ => assert_eq!(fv.pop().map(|t| t.id), model.pop()),
            }
            assert_eq!(fv.iter().map(|t| t.id).collect::<Vec<_>>(), model);
        }
        // fv dropped here with elements still inside
    }
    assert!(
        balanced(),
        "created {} vs dropped {}",
        CREATED.load(SeqCst),
        DROPPED.load(SeqCst)
    );

    // Panic safety: the middle element's drop panics; the others must still be dropped once.
    std::panic::set_hook(Box::new(|_| {})); // silence expected panic output
    let (c0, d0) = (CREATED.load(SeqCst), DROPPED.load(SeqCst));
    let res = catch_unwind(AssertUnwindSafe(|| {
        let mut fv: FixedVec<Tracked, 5> = FixedVec::new();
        for i in 0..5 {
            let mut t = Tracked::new(i);
            t.panic_on_drop = i == 2;
            fv.push(t).ok().unwrap();
        }
    }));
    let _ = std::panic::take_hook();
    assert!(res.is_err());
    assert_eq!(CREATED.load(SeqCst) - c0, 5);
    assert_eq!(
        DROPPED.load(SeqCst) - d0,
        5,
        "all five dropped despite the panic"
    );
    println!(
        "random push/pop/drop and panic-in-drop: created == dropped == {}",
        DROPPED.load(SeqCst)
    );
}
