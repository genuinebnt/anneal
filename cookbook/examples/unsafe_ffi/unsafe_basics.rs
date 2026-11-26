//! RECIPE 99 (unsafe): unsafe basics: the five superpowers, safe wrappers, and how to test them.
//! LEVEL: intermediate
//! Use in: FFI shims, hot loops, building std-like containers, auditing `unsafe` in dependencies.   Run: cargo run --release --example unsafe_basics
#![allow(dead_code)]
//!
//! `unsafe` does NOT turn off the borrow checker. It unlocks exactly five things the compiler cannot verify:
//!   1. dereference a raw pointer (`*const T`, `*mut T`)      2. call an `unsafe fn` (or extern fn)
//!   3. access/modify a `static mut`                          4. implement an `unsafe trait` (Send, Sync, ...)
//!   5. read a field of a `union`.
//! The discipline: keep `unsafe` blocks tiny, put the proof in a `// SAFETY:` comment, document caller duties of
//! `unsafe fn` under `# Safety`, and wrap everything in a SAFE function whose assertions make misuse impossible
//! (or a panic). Soundness rule: safe code must never be able to trigger UB through your API.
//! `static mut` is discouraged: any access can race, and in edition 2024 taking `&`/`&mut` to it is a deny-by-default
//! lint. Prefer `AtomicUsize`, `OnceLock`, `Mutex`, or thread_local.
//! Testing unsafe: (a) boundary cases (0, 1, len), (b) drop counters to prove no leak / double drop,
//! (c) run under Miri, which detects UB (out-of-bounds, aliasing violations, uninit reads, leaks):
//!     rustup +nightly component add miri && cargo +nightly miri run --example unsafe_basics
//! (Miri is not installed in this environment; the command is documentation only.)
//! COMMON USES: slice/Vec internals in std (`split_at_mut`, `swap`), FFI wrappers around C libraries, SIMD and
//! zero-copy parsers, lock-free structures, custom allocators and arenas.
//! Read: https://doc.rust-lang.org/nomicon/ and https://doc.rust-lang.org/std/ptr/index.html
use std::cell::Cell;
use std::ptr;
use std::slice;

// ---- 1. raw pointers: creating them is safe, dereferencing is not ----
fn raw_pointer_demo() {
    let mut x = 10;
    let p = &raw mut x; // *mut i32, no reference created
    // SAFETY: `p` points to the live local `x`, is aligned, and nothing else accesses `x` during these uses.
    unsafe {
        *p += 5;
        assert_eq!(p.read(), 15);
    }
    let dangling: *const i32 = ptr::null();
    assert!(dangling.is_null()); // checking is safe; dereferencing would be UB
}

// ---- 2. unsafe fn: the caller must uphold a documented contract ----
/// Returns the element at `i` without bounds checking.
///
/// # Safety
/// `i` must be `< s.len()`. Violating this is undefined behaviour.
unsafe fn get_at<T>(s: &[T], i: usize) -> &T {
    debug_assert!(i < s.len());
    // SAFETY: the caller guarantees `i < s.len()`, so the offset stays inside the slice's allocation.
    unsafe { &*s.as_ptr().add(i) }
}

/// Safe wrapper: the assertion turns the contract into a panic.
fn checked_get<T>(s: &[T], i: usize) -> &T {
    assert!(i < s.len(), "index {i} out of range {}", s.len());
    // SAFETY: just asserted `i < s.len()`.
    unsafe { get_at(s, i) }
}

// ---- reimplement slice::split_at_mut: two disjoint &mut from one &mut ----
fn my_split_at_mut<T>(s: &mut [T], mid: usize) -> (&mut [T], &mut [T]) {
    let len = s.len();
    assert!(mid <= len, "mid {mid} > len {len}");
    let p = s.as_mut_ptr();
    // SAFETY: [0, mid) and [mid, len) do not overlap and both lie inside `s`, which we hold uniquely for the
    // returned lifetime. `p.add(mid)` is at most one-past-the-end, which is allowed (and the slice is then empty).
    unsafe {
        (
            slice::from_raw_parts_mut(p, mid),
            slice::from_raw_parts_mut(p.add(mid), len - mid),
        )
    }
}

// ---- swap via ptr::read/write: bitwise moves, no Drop runs, no clones ----
fn my_swap<T>(s: &mut [T], i: usize, j: usize) {
    assert!(i < s.len() && j < s.len(), "swap index out of range");
    if i == j {
        return; // read/write of the same slot would be fine here, but there is nothing to do
    }
    let p = s.as_mut_ptr();
    // SAFETY: i != j and both in bounds, so the two slots are distinct, valid and aligned. `read` makes a bitwise
    // copy, then `write` overwrites without dropping the old value; each value is therefore owned exactly once.
    unsafe {
        let a = ptr::read(p.add(i));
        ptr::copy_nonoverlapping(p.add(j), p.add(i), 1); // slots differ, so no overlap
        ptr::write(p.add(j), a);
    }
}

// ---- 3. static mut: shown only to explain why not ----
static mut COUNTER: u32 = 0;
fn bump_static() -> u32 {
    // SAFETY: this demo is single-threaded, and we read/write by value (no reference to the static is formed).
    // With threads this would be a data race = UB; use an atomic instead.
    unsafe {
        COUNTER += 1;
        COUNTER
    }
}

// ---- 4. unsafe impl Send: YOU promise the type is thread-safe ----
/// Owning pointer to a heap `T`, like a tiny `Box`.
struct Owned<T>(*mut T);
impl<T> Owned<T> {
    fn new(v: T) -> Self {
        Owned(Box::into_raw(Box::new(v)))
    }
    fn get(&self) -> &T {
        // SAFETY: the pointer came from Box::into_raw and is freed only in Drop, so it is valid for `&self`.
        unsafe { &*self.0 }
    }
}
impl<T> Drop for Owned<T> {
    fn drop(&mut self) {
        // SAFETY: unique owner, pointer from Box::into_raw, dropped exactly once.
        drop(unsafe { Box::from_raw(self.0) });
    }
}
// SAFETY: `Owned<T>` uniquely owns its T like Box<T>, so moving it to another thread is fine iff T: Send.
unsafe impl<T: Send> Send for Owned<T> {}
// SAFETY: only `&T` is reachable through `&Owned<T>`, so sharing is fine iff T: Sync.
unsafe impl<T: Sync> Sync for Owned<T> {}

// ---- 5. union ----
union IntOrFloat {
    i: u32,
    f: f32,
}

// ---- drop counter for tests ----
struct Counted<'a>(&'a Cell<usize>, u32);
impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
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

fn main() {
    raw_pointer_demo();
    let v = [1, 2, 3];
    assert_eq!(*checked_get(&v, 2), 3);
    assert!(std::panic::catch_unwind(|| checked_get(&[1, 2, 3], 3)).is_err());

    // split_at_mut at every boundary, including 0 and len
    let mut a = [1, 2, 3, 4, 5];
    for mid in 0..=a.len() {
        let (l, r) = my_split_at_mut(&mut a, mid);
        assert_eq!((l.len(), r.len()), (mid, 5 - mid));
        if let Some(x) = l.first_mut() {
            *x += 100;
        }
        if let Some(y) = r.last_mut() {
            *y += 100;
        }
    }
    println!("after split_at_mut sweep: {a:?}");
    assert!(std::panic::catch_unwind(|| my_split_at_mut(&mut [1, 2], 3).0.len()).is_err());

    // differential test: my_swap vs slice::swap, with drop counting
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for _ in 0..200 {
        let n = 1 + (rng.next() % 8) as usize;
        let drops = Cell::new(0);
        {
            let mut mine: Vec<Counted> = (0..n as u32).map(|k| Counted(&drops, k)).collect();
            let mut std_v: Vec<u32> = (0..n as u32).collect();
            for _ in 0..20 {
                let (i, j) = (
                    (rng.next() % n as u64) as usize,
                    (rng.next() % n as u64) as usize,
                );
                my_swap(&mut mine, i, j);
                std_v.swap(i, j);
            }
            let ids: Vec<u32> = mine.iter().map(|c| c.1).collect();
            assert_eq!(ids, std_v);
            assert_eq!(drops.get(), 0, "swap must not drop anything");
        }
        assert_eq!(drops.get(), n, "every element dropped exactly once");
    }

    println!(
        "static mut counter: {} then {}",
        bump_static(),
        bump_static()
    );
    let o = Owned::new(String::from("sent across threads"));
    let h = std::thread::spawn(move || o.get().len());
    assert_eq!(h.join().unwrap(), 19);

    let u = IntOrFloat { f: 1.0 };
    // SAFETY: every bit pattern is a valid u32, and the union was initialised via its f32 field (same size).
    assert_eq!(unsafe { u.i }, 1.0f32.to_bits());
    println!("all unsafe-basics checks passed");
}
