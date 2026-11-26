//! RECIPE 605 (interview): Unsafe Rust, memory layout, numeric edge cases, Drop/leaks, statics and FFI.
//! LEVEL: advanced
//! Use in: interview preparation, auditing `unsafe` blocks, reasoning about struct sizes, overflow and `as` casts.   Run: cargo run --release --example rust_unsafe_memory_layout_qa
//! COMMON USES: FFI bindings, performance-critical containers, memory-mapped data, embedded/no_std work, safe abstractions over raw pointers.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233474457/rust-interview-questions-2025-ownership-borrowing-lifetimes-traits-async-await-error-handling-smart-pointers-concurrency/
//! Read: https://fasterthanli.me/articles/pin-and-suffering and https://www.techinterview.org/post/3233460429/rust-interview-questions/
//! Read: https://github.com/devinterview-io/rust-interview-questions and https://interviewing.io/rust-interview-questions
//!
//! QUESTION BANK
//! Q1. What does `unsafe` actually permit, and what does it NOT change? [Medium] [asked: very common]
//! Q2. Safety contract vs soundness: what makes an unsafe abstraction sound? [Hard] [asked: common]
//! Q3. How do raw pointers and `MaybeUninit` work? [Hard] [asked: common]
//! Q4. What are the sizes and alignments of common types? What is a niche? [Medium] [asked: very common]
//! Q5. Does Rust reorder struct fields? `repr(C)`, `repr(transparent)`, `repr(packed)`, `repr(u8)`. [Hard] [asked: common]
//! Q6. Stack vs heap: where does each value live? [Easy] [asked: very common]
//! Q7. How does `Vec` grow, and why is `push` amortised O(1)? [Medium] [asked: very common]
//! Q8. Why is `String` UTF-8, and what are bytes vs chars vs graphemes? [Medium] [asked: very common]
//! Q9. What happens on integer overflow (debug vs release)? [Easy] [asked: very common]
//! Q10. What are the pitfalls of `as` casts? [Medium] [asked: common]
//! Q11. Why is `transmute` dangerous and what replaces it? [Hard] [asked: common]
//! Q12. What problem does `Pin` solve for self-referential structs? [Hard] [asked: common]
//! Q13. Are abstractions really zero-cost? Monomorphisation bloat and `#[inline]`. [Medium] [asked: common]
//! Q14. `Drop`, `mem::forget`, `ManuallyDrop` and `Box::leak`: is leaking safe? [Medium] [asked: common]
//! Q15. `static` vs `const`; `static mut`; thread-local storage. [Medium] [asked: common]
//! Q16. How does FFI work (`extern "C"`, `CString`, callbacks)? [Medium] [asked: common]
//! Q17. Data race vs race condition: what does Rust actually prevent? [Hard] [asked: very common]

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::hint::black_box;
use std::marker::PhantomPinned;
use std::mem::{self, ManuallyDrop, MaybeUninit, align_of, size_of};
use std::num::NonZeroU32;
use std::panic;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

fn quiet<R>(f: impl FnOnce() -> R) -> R {
    let old = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let r = f();
    panic::set_hook(old);
    r
}

// ---------------------------------------------------------------------------
// Q1. the five superpowers
// ---------------------------------------------------------------------------
/// ANSWER: `unsafe` unlocks exactly five operations the compiler cannot verify: (1) dereference a raw pointer,
/// (2) call an `unsafe fn` (including foreign functions), (3) access or modify a mutable `static`,
/// (4) implement an `unsafe trait` (Send, Sync, GlobalAlloc...), (5) access fields of a `union`. It does NOT turn off the
/// borrow checker, type checking or lifetime rules: references inside an `unsafe` block are still checked. It is a
/// promise by the programmer that the unchecked preconditions hold; violating them is undefined behaviour (UB).
/// Edition 2024 additionally requires `unsafe` for `extern` blocks and warns on unsafe ops inside `unsafe fn` bodies
/// without an inner block (`unsafe_op_in_unsafe_fn`).
///
/// REALLY TESTING: do you know the precise list and the "borrowck still applies" point.
/// WRONG: "unsafe disables all checks" / "unsafe code is inherently wrong".
/// FOLLOW-UP: name UB examples: data races, dangling/misaligned deref, invalid values (bool = 2), aliasing &mut.
static mut COUNTER: u32 = 0;
unsafe trait Zeroable {}
// SAFETY: u32 has no invalid bit patterns, all-zero is a valid value.
unsafe impl Zeroable for u32 {}
union IntOrFloat {
    i: u32,
    f: f32,
}
unsafe fn read_at(p: *const i32, i: usize) -> i32 {
    // SAFETY: the caller of `read_at` guarantees `p.add(i)` is in bounds and valid for reads.
    unsafe { *p.add(i) }
}
fn q1_five_superpowers() {
    let mut x = 5;
    let r1 = &raw const x; // creating a raw pointer is safe
    let r2 = &raw mut x;
    // (1) dereference raw pointers
    // SAFETY: both point to the live local `x`; no reference to `x` is used while we write through r2.
    unsafe {
        *r2 += 1;
        assert_eq!(*r1, 6);
    }
    // (2) call an unsafe fn
    let arr = [10, 20, 30];
    // SAFETY: index 2 < len 3 and `arr` outlives the call.
    assert_eq!(unsafe { read_at(arr.as_ptr(), 2) }, 30);
    // (3) mutable static
    // SAFETY: single-threaded access in this function; no references to the static are created.
    unsafe {
        COUNTER += 1;
        let seen = COUNTER; // copy out; never `&COUNTER` (denied in edition 2024)
        assert_eq!(seen, 1);
    }
    // (4) unsafe trait: implementing is the unsafe act (see `unsafe impl Zeroable` above)
    fn needs_zeroable<T: Zeroable>() {}
    needs_zeroable::<u32>();
    // (5) union field read
    let u = IntOrFloat { f: 1.0 };
    // SAFETY: every bit pattern of u32 is valid, so reinterpreting the f32 bits is fine.
    assert_eq!(unsafe { u.i }, 0x3f80_0000);

    // The borrow checker is STILL ON inside unsafe:
    //   let mut v = vec![1];
    //   unsafe { let a = &mut v; let b = &mut v; a.push(1); b.push(2); }   // error[E0499] even here
    // Only raw pointers escape the checker, which is exactly why they need `unsafe` to use.
}

// ---------------------------------------------------------------------------
// Q2. soundness
// ---------------------------------------------------------------------------
/// ANSWER: An `unsafe fn` documents a SAFETY CONTRACT (`# Safety` section) the caller must uphold. A SAFE function that
/// uses unsafe internally is SOUND only if NO safe caller, with any arguments, can trigger UB, so it must itself check
/// (or make impossible) every precondition; otherwise it is unsound even if "nobody calls it badly yet". The unsafe
/// block's `// SAFETY:` comment states why the preconditions hold HERE. Keep unsafe blocks tiny, wrap them in a module
/// whose invariants (private fields) cannot be broken from outside, and test with Miri/sanitizers. `Vec`, `RefCell`,
/// `Mutex`, `split_at_mut` are all safe APIs over unsafe internals.
///
/// REALLY TESTING: the "safe abstraction" mindset and the soundness vocabulary.
/// WRONG: "if it works in tests it is sound" (UB can appear to work).
/// FOLLOW-UP: what tool finds UB in tests? Miri (`cargo +nightly miri test`).
/// # Safety (for the unsafe variant): `i < v.len()`.
unsafe fn get_unchecked_u32(v: &[u32], i: usize) -> u32 {
    // SAFETY: caller contract.
    unsafe { *v.get_unchecked(i) }
}
// SOUND: validates before using unsafe.
fn get_pair(v: &[u32], i: usize) -> Option<(u32, u32)> {
    if v.len() >= 2 && i < v.len() - 1 {
        // SAFETY: i + 1 < len was just checked, so both indices are in bounds.
        unsafe { Some((get_unchecked_u32(v, i), get_unchecked_u32(v, i + 1))) }
    } else {
        None
    }
}
// UNSOUND (do not write): a safe signature with an unchecked precondition.
//   pub fn bad_get(v: &[u32], i: usize) -> u32 { unsafe { *v.get_unchecked(i) } }   // bad_get(&[], 5) is UB from safe code
// A tiny sound container: private invariant `len <= cap` protects the unsafe read.
struct Buf {
    data: Vec<MaybeUninit<u32>>,
    len: usize, // invariant: the first `len` slots are initialised
}
impl Buf {
    fn new(cap: usize) -> Self {
        Buf {
            data: (0..cap).map(|_| MaybeUninit::uninit()).collect(),
            len: 0,
        }
    }
    fn push(&mut self, v: u32) -> Result<(), u32> {
        if self.len == self.data.len() {
            return Err(v);
        }
        self.data[self.len].write(v);
        self.len += 1;
        Ok(())
    }
    fn get(&self, i: usize) -> Option<u32> {
        if i < self.len {
            // SAFETY: slots [0, len) are initialised by `push` and `len` is private.
            Some(unsafe { self.data[i].assume_init() })
        } else {
            None
        }
    }
}
fn q2_soundness() {
    assert_eq!(get_pair(&[1, 2, 3], 1), Some((2, 3)));
    assert_eq!(get_pair(&[1, 2, 3], 2), None);
    assert_eq!(get_pair(&[], 0), None);
    let mut b = Buf::new(2);
    assert_eq!((b.push(7), b.push(8), b.push(9)), (Ok(()), Ok(()), Err(9)));
    assert_eq!((b.get(0), b.get(1), b.get(2)), (Some(7), Some(8), None));
}

// ---------------------------------------------------------------------------
// Q3. raw pointers and MaybeUninit
// ---------------------------------------------------------------------------
/// ANSWER: `*const T`/`*mut T` are like C pointers: may be null, dangling, unaligned, aliased; creating and comparing them is safe,
/// DEREFERENCING is unsafe. They carry no lifetime, are `!Send/!Sync`, and support `add/offset/read/write/copy`. A deref is
/// valid only if the pointer is non-null, aligned, points to a live allocation of enough size and an initialised `T`
/// (for reads). `MaybeUninit<T>` is a wrapper for memory that is not yet a valid `T`; reading uninitialised memory as `T`
/// is UB even for integers, so `mem::uninitialized()` is deprecated. Use `MaybeUninit::uninit()`, `.write(v)`,
/// `.assume_init()` after full initialisation. Prefer safe tools (`array::from_fn`, `Vec::with_capacity`) when possible.
///
/// REALLY TESTING: what makes a pointer valid; why uninitialised integers are UB.
/// WRONG: "raw pointers are just addresses; deref of any address is fine".
/// FOLLOW-UP: `NonNull<T>`: non-null, covariant raw pointer used by Vec/Box internals.
fn q3_raw_pointers_maybeuninit() {
    let mut v = vec![1, 2, 3, 4];
    let p = v.as_mut_ptr();
    // SAFETY: p points to 4 initialised elements of a live Vec; we stay within bounds and do not use `v` meanwhile.
    unsafe {
        *p.add(1) = 20;
        p.add(3).write(40);
        assert_eq!(p.add(2).read(), 3);
        std::ptr::swap(p, p.add(3));
    }
    assert_eq!(v, [40, 20, 3, 1]);
    let null: *const i32 = std::ptr::null();
    assert!(null.is_null());
    assert_eq!(size_of::<*const u8>(), size_of::<usize>());
    assert_eq!(
        size_of::<*const [u8]>(),
        2 * size_of::<usize>(),
        "raw slice pointers are fat too"
    );

    // MaybeUninit: build an array element by element.
    let mut arr: [MaybeUninit<String>; 3] = [const { MaybeUninit::uninit() }; 3];
    for (i, slot) in arr.iter_mut().enumerate() {
        slot.write(format!("s{i}"));
    }
    // SAFETY: all three elements were initialised above; MaybeUninit<T> has the same layout as T.
    let arr: [String; 3] = unsafe { mem::transmute::<[MaybeUninit<String>; 3], [String; 3]>(arr) };
    assert_eq!(arr, ["s0", "s1", "s2"]);
    let safe: [String; 3] = std::array::from_fn(|i| format!("s{i}")); // safe alternative
    assert_eq!(arr, safe);
    let mut one = MaybeUninit::<u64>::uninit();
    one.write(9);
    // SAFETY: written just above.
    assert_eq!(unsafe { one.assume_init() }, 9);
    // let x: u32 = unsafe { MaybeUninit::uninit().assume_init() };   // UB: reading uninitialised memory
}

// ---------------------------------------------------------------------------
// Q4. sizes, alignment, niches
// ---------------------------------------------------------------------------
/// ANSWER: `size_of::<T>()` is a multiple of `align_of::<T>()`. On 64-bit: `u8/bool` 1, `u16` 2, `u32/char/f32` 4, `u64/f64/usize` 8,
/// `&T`/`Box<T>` 8, `&str`/`&[T]`/`Box<dyn Trait>`/`&dyn Trait` 16 (fat), `String`/`Vec<T>` 24 (ptr, cap, len), `()` and unit structs 0
/// (ZSTs). A NICHE is a bit pattern a type can never hold (null in references/Box, 0 in `NonZero*`, 2..255 in `bool`, invalid `char`s,
/// capacity limits in Vec); the compiler stores `None` in it, so `Option<Box<T>>`, `Option<&T>`, `Option<NonZeroU32>`,
/// `Option<bool>`, `Option<String>` cost no extra space, while `Option<u32>` needs a tag (8 bytes, alignment padding).
///
/// REALLY TESTING: mental model of memory cost; niche optimisation.
/// WRONG: "Option<T> always adds a byte".
/// FOLLOW-UP: what is `size_of::<Option<Option<bool>>>()`? 1: more niches remain (the compiler uses 2, 3...).
enum Three {
    A,
    B,
    C,
}
enum WithData {
    Small(u8),
    Big(u32),
}
fn q4_sizes_and_niches() {
    let w = size_of::<usize>();
    assert_eq!(w, 8, "this recipe assumes a 64-bit target");
    assert_eq!(
        (
            size_of::<u8>(),
            size_of::<u16>(),
            size_of::<u32>(),
            size_of::<u64>()
        ),
        (1, 2, 4, 8)
    );
    assert_eq!(
        (size_of::<bool>(), size_of::<char>(), size_of::<f64>()),
        (1, 4, 8)
    );
    assert_eq!(
        (
            size_of::<()>(),
            size_of::<[u8; 0]>(),
            size_of::<[(); 100]>()
        ),
        (0, 0, 0)
    );
    assert_eq!(
        (size_of::<&u8>(), size_of::<Box<u8>>(), size_of::<fn()>()),
        (8, 8, 8)
    );
    assert_eq!(
        (
            size_of::<&str>(),
            size_of::<&[u8]>(),
            size_of::<&dyn std::fmt::Debug>()
        ),
        (16, 16, 16)
    );
    assert_eq!(
        (
            size_of::<String>(),
            size_of::<Vec<u8>>(),
            size_of::<Vec<u64>>()
        ),
        (24, 24, 24)
    );
    // niches:
    assert_eq!(size_of::<Option<Box<u8>>>(), 8);
    assert_eq!(size_of::<Option<&u8>>(), 8);
    assert_eq!(size_of::<Option<NonZeroU32>>(), 4);
    assert_eq!(size_of::<Option<bool>>(), 1);
    assert_eq!(size_of::<Option<Option<bool>>>(), 1);
    assert_eq!(size_of::<Option<char>>(), 4);
    assert_eq!(size_of::<Option<String>>(), 24);
    assert_eq!(
        size_of::<Option<u32>>(),
        8,
        "u32 has no niche: needs a tag + padding"
    );
    assert_eq!(size_of::<Option<u8>>(), 2);
    // enums:
    assert_eq!(size_of::<Three>(), 1);
    assert_eq!(size_of::<WithData>(), 8); // tag + u32 payload, aligned to 4
    assert_eq!(size_of::<Option<Three>>(), 1);
    // alignment:
    assert_eq!(
        (
            align_of::<u8>(),
            align_of::<u32>(),
            align_of::<u64>(),
            align_of::<&u8>()
        ),
        (1, 4, 8, 8)
    );
    assert_eq!(align_of::<[u8; 7]>(), 1);
    assert_eq!(size_of::<(u8, u32)>() % align_of::<(u8, u32)>(), 0);
    assert_eq!(size_of::<[u32; 5]>(), 20);
    assert_eq!(mem::size_of_val("héllo"), 6);
}

// ---------------------------------------------------------------------------
// Q5. layout and repr
// ---------------------------------------------------------------------------
/// ANSWER: The default `repr(Rust)` layout is UNSPECIFIED: the compiler may reorder fields to minimise padding (largest-first
/// is typical) and may change between versions, so never assume offsets or transmute structs. `repr(C)` fixes C layout (declaration
/// order, C padding rules): required for FFI and when you need defined offsets. `repr(transparent)` (one non-ZST field) guarantees
/// the same layout and ABI as that field (newtypes for FFI). `repr(packed)` removes padding (alignment 1) but makes references to
/// fields potentially misaligned UB (taking `&packed.field` is error E0793; copy the field out). `repr(u8)` etc. on an
/// enum sets the discriminant type (and size); `repr(align(N))` raises alignment (cache-line padding).
///
/// REALLY TESTING: padding awareness and when layout is guaranteed.
/// WRONG: "fields are stored in the order written" (only with repr(C)).
/// FOLLOW-UP: how to reduce a struct's size? Order fields by alignment (the compiler already does) or use smaller types.
struct RustLayout {
    a: u8,
    b: u32,
    c: u8,
}
#[repr(C)]
struct CLayout {
    a: u8,
    b: u32,
    c: u8,
}
#[repr(C, packed)]
struct Packed {
    a: u8,
    b: u32,
    c: u8,
}
#[repr(transparent)]
struct Meters(f64);
#[repr(u8)]
enum Color {
    Red = 1,
    Green = 2,
    Blue = 4,
}
#[repr(align(64))]
struct CacheLine(u8);
fn q5_layout_and_repr() {
    assert_eq!(
        size_of::<RustLayout>(),
        8,
        "reordered: b, a, c + 2 padding bytes"
    );
    assert_eq!(size_of::<CLayout>(), 12, "a(1)+pad(3)+b(4)+c(1)+pad(3)");
    assert_eq!(align_of::<CLayout>(), 4);
    assert_eq!(mem::offset_of!(CLayout, a), 0);
    assert_eq!(mem::offset_of!(CLayout, b), 4);
    assert_eq!(mem::offset_of!(CLayout, c), 8);
    assert_eq!(size_of::<Packed>(), 6);
    assert_eq!(align_of::<Packed>(), 1);
    let p = Packed {
        a: 1,
        b: 0xdead_beef,
        c: 3,
    };
    // let r = &p.b;                    // error[E0793]: reference to packed field is unaligned
    let copy = p.b; // copying the field out is fine
    assert_eq!((copy, p.a, p.c), (0xdead_beef, 1, 3));
    assert_eq!(size_of::<Meters>(), size_of::<f64>());
    assert_eq!(align_of::<Meters>(), align_of::<f64>());
    assert_eq!(size_of::<Color>(), 1);
    assert_eq!(
        (Color::Red as u8, Color::Green as u8, Color::Blue as u8),
        (1, 2, 4)
    );
    assert_eq!((size_of::<CacheLine>(), align_of::<CacheLine>()), (64, 64));
    let _ = (
        RustLayout { a: 1, b: 2, c: 3 }.a,
        CLayout { a: 0, b: 0, c: 0 }.c,
    );
}

// ---------------------------------------------------------------------------
// Q6. stack vs heap
// ---------------------------------------------------------------------------
/// ANSWER: Locals (and fixed-size values: ints, arrays, structs, the 24-byte `Vec`/`String` HEADERS) live in the stack frame, allocated
/// and freed by moving the stack pointer: very fast, size fixed at compile time, limited (typically 8 MiB main thread, 2 MiB spawned).
/// Heap memory comes from the allocator (`Box`, `Vec`/`String` buffers, `Rc`/`Arc` contents): dynamic size and lifetime, slower
/// (alloc/free, cache misses), freed by `Drop`. A `Vec<u8>` of one million bytes is a 24-byte header on the stack pointing to 1 MB on the
/// heap. `static`s and string literals live in the binary's data segment. Moving a Vec copies 24 bytes, not the elements.
///
/// REALLY TESTING: where memory comes from and why `Box::new([0; BIG])` can still overflow the stack (array built on the stack first).
/// WRONG: "everything in Rust is on the stack unless you use Box" (Vec/String/HashMap data is heap) or "Box is faster".
/// FOLLOW-UP: how to allocate a huge array directly on the heap? `vec![0; N].into_boxed_slice()`.
static GLOBAL: u64 = 42;
fn q6_stack_vs_heap() {
    let local = 1u64;
    let boxed = Box::new(2u64);
    let v = vec![0u8; 1_000_000];
    assert_eq!(
        size_of_val(&v),
        24,
        "stack header size is independent of len"
    );
    assert_eq!(
        v.len() * size_of::<u8>(),
        1_000_000,
        "the elements are on the heap"
    );
    let moved = v; // moves 24 bytes
    assert_eq!(moved.as_ptr() as usize % align_of::<u8>(), 0);
    let stack_addr = &local as *const u64 as usize;
    let heap_addr = &*boxed as *const u64 as usize;
    let static_addr = &GLOBAL as *const u64 as usize;
    assert_ne!(stack_addr, heap_addr);
    assert_ne!(heap_addr, static_addr);
    assert_ne!(stack_addr, static_addr);
    // The box pointer itself is a stack value; the heap address does not change when the Box is moved.
    let before = &*boxed as *const u64;
    let b2 = boxed;
    assert_eq!(before, &*b2 as *const u64);
    // Heap-direct big allocation:
    let big = vec![0u8; 64 * 1024 * 1024].into_boxed_slice();
    assert_eq!(big.len(), 64 * 1024 * 1024);
    // let overflow = [0u8; 64 * 1024 * 1024];   // would overflow an 8 MiB stack at runtime
}

// ---------------------------------------------------------------------------
// Q7. Vec growth
// ---------------------------------------------------------------------------
/// ANSWER: A `Vec` is (ptr, capacity, len). When `len == capacity` a push reallocates to a larger buffer, currently max(2 x capacity,
/// needed, small minimum), and moves the elements; the exact factor is an implementation detail, but geometric growth is the
/// reason `push` is amortised O(1): n pushes cost O(n) total since copy work forms a geometric series (n/2 + n/4 + ... < n), though
/// one individual push can be O(n). `Vec::with_capacity(n)` allocates once; `reserve` hints; `shrink_to_fit` releases slack
/// (best effort); `clear` keeps capacity. Reallocation invalidates pointers/references to elements (hence the borrow rule).
///
/// REALLY TESTING: amortised analysis and capacity vs length.
/// WRONG: "push is O(1) worst-case" or "capacity == length".
/// FOLLOW-UP: why `Vec::with_capacity` in hot loops? Avoids log n reallocations and copies.
fn q7_vec_growth() {
    let mut v: Vec<u32> = Vec::new();
    assert_eq!(
        (v.len(), v.capacity()),
        (0, 0),
        "no allocation until first push"
    );
    let mut caps = vec![v.capacity()];
    for i in 0..1000 {
        v.push(i);
        if v.capacity() != *caps.last().unwrap() {
            caps.push(v.capacity());
        }
    }
    // every growth from a non-empty buffer at least doubles (current implementation: exactly doubles)
    for w in caps.windows(2).skip(1) {
        assert!(w[1] >= 2 * w[0], "geometric growth: {} -> {}", w[0], w[1]);
    }
    assert!(
        caps.len() <= 12,
        "1000 pushes needed only {} allocations",
        caps.len() - 1
    );
    assert!(v.capacity() >= 1000);

    let mut w: Vec<u32> = Vec::with_capacity(1000);
    let ptr = w.as_ptr();
    for i in 0..1000 {
        w.push(i);
    }
    assert_eq!(w.as_ptr(), ptr, "no reallocation: the buffer never moved");
    let before = v.capacity();
    v.truncate(10);
    assert_eq!(v.capacity(), before, "truncate keeps the capacity");
    v.shrink_to_fit();
    assert!(v.capacity() < before && v.capacity() >= 10);
    // Amortised cost: count element copies during growth for n pushes (copies ~ capacity at each reallocation).
    let n = 1 << 16;
    let mut copies = 0usize;
    let mut vv: Vec<u8> = Vec::new();
    for i in 0..n {
        let cap = vv.capacity();
        vv.push(i as u8);
        if vv.capacity() != cap {
            copies += vv.len() - 1; // elements moved to the new buffer
        }
    }
    assert!(
        copies < 2 * n,
        "total copies {copies} < 2n for n = {n}: amortised O(1) per push"
    );
    // Zero-sized elements never allocate:
    let mut z: Vec<()> = Vec::new();
    z.push(());
    assert_eq!(z.capacity(), usize::MAX);
}

// ---------------------------------------------------------------------------
// Q8. UTF-8
// ---------------------------------------------------------------------------
/// ANSWER: `String`/`str` are guaranteed valid UTF-8, a variable-width encoding (1-4 bytes per `char`, a Unicode scalar value).
/// `len()` is BYTES, `chars()` iterates scalar values (O(n)), and what users see as one character may be several
/// `char`s (a GRAPHEME CLUSTER: "e" + combining accent, emoji with modifiers; needs the `unicode-segmentation` crate). Because
/// of this there is no `s[i]` indexing (E0277: `String` cannot be indexed by integer) and byte-range slicing `&s[a..b]` PANICS if a
/// boundary is not on a char boundary (`is_char_boundary`). Benefits: ASCII-compatible, no endianness, self-synchronising, compact.
///
/// REALLY TESTING: bytes vs chars vs graphemes.
/// WRONG: "len() is the number of characters" / "chars are 1 byte".
/// FOLLOW-UP: how to get the nth char? `s.chars().nth(n)` (O(n)); `char_indices` for byte offsets.
fn q8_string_utf8() {
    let s = String::from("héllo");
    assert_eq!((s.len(), s.chars().count()), (6, 5));
    assert_eq!("é".len(), 2);
    assert_eq!("日".len(), 3);
    assert_eq!("🦀".len(), 4);
    assert_eq!('🦀'.len_utf8(), 4);
    assert_eq!("é".chars().next().unwrap() as u32, 0xE9);
    assert_eq!(&s[0..1], "h");
    assert_eq!(&s[1..3], "é");
    assert!(!s.is_char_boundary(2));
    let r = quiet(|| panic::catch_unwind(|| s[..black_box(2)].len()));
    assert!(r.is_err(), "slicing in the middle of `é` panics at runtime");
    assert!(
        s.get(0..2).is_none(),
        "`get` returns None instead of panicking"
    );
    assert_eq!(
        s.char_indices().map(|(i, _)| i).collect::<Vec<_>>(),
        [0, 1, 3, 4, 5]
    );
    // grapheme cluster: 1 visible character, 2 chars, 3 bytes
    let decomposed = "e\u{301}";
    assert_eq!((decomposed.chars().count(), decomposed.len()), (2, 3));
    assert_ne!(
        decomposed, "\u{e9}",
        "visually identical, different code points (needs normalisation)"
    );
    // from_utf8 validates; bytes are not text until validated
    assert!(String::from_utf8(vec![0xff, 0xfe]).is_err());
    assert_eq!(String::from_utf8_lossy(&[b'a', 0xff]), "a\u{fffd}");
    // FAILING (E0277): let c = s[0];   // `String` cannot be indexed by `{integer}`
    assert_eq!(s.chars().rev().collect::<String>(), "olléh");
}

// ---------------------------------------------------------------------------
// Q9. integer overflow
// ---------------------------------------------------------------------------
/// ANSWER: Arithmetic overflow is NOT undefined behaviour in Rust. In DEBUG builds (`overflow-checks` on) `+ - *` panic ("attempt
/// to add with overflow"); in RELEASE builds the default is two's-complement WRAPPING (configurable via
/// `[profile.release] overflow-checks = true`). Be explicit instead: `wrapping_*` (always wrap), `checked_*` (returns `Option`),
/// `saturating_*` (clamp to MIN/MAX), `overflowing_*` (value + overflow flag), `Wrapping<T>`. Division by zero and `i32::MIN / -1`
/// always panic. Constant expressions that overflow are a compile error (`deny(arithmetic_overflow)`). Array indexing is always bounds checked.
///
/// REALLY TESTING: knowing that behaviour differs by profile and which explicit methods to choose (hashing => wrapping, input => checked).
/// WRONG: "overflow is UB like in C" (signed overflow in C is UB; in Rust it is defined).
/// FOLLOW-UP: `usize` subtraction `a - b` with b > a: the classic underflow bug; use `checked_sub`/`abs_diff`.
fn q9_integer_overflow() {
    let max = black_box(255u8);
    assert_eq!(max.wrapping_add(1), 0);
    assert_eq!(max.checked_add(1), None);
    assert_eq!(max.saturating_add(1), 255);
    assert_eq!(max.overflowing_add(1), (0, true));
    assert_eq!(i32::MIN.wrapping_abs(), i32::MIN, "abs(MIN) overflows");
    assert_eq!(i32::MIN.checked_abs(), None);
    assert_eq!(5u32.checked_sub(6), None);
    assert_eq!(5u32.abs_diff(6), 1);
    assert_eq!(
        std::num::Wrapping(250u8) + std::num::Wrapping(10),
        std::num::Wrapping(4)
    );
    assert_eq!((-128i8).wrapping_neg(), -128);
    assert_eq!(200u8.saturating_mul(2), 255);
    assert_eq!(i64::MAX.wrapping_add(1), i64::MIN);

    // The plain operator: behaviour depends on the build profile.
    let r = quiet(|| panic::catch_unwind(|| black_box(255u8) + black_box(1u8)));
    if cfg!(debug_assertions) {
        assert!(r.is_err(), "debug: panics on overflow");
    } else {
        assert_eq!(r.unwrap(), 0, "release: wraps silently");
    }
    // Division by zero and MIN / -1 panic in every profile:
    assert!(quiet(|| panic::catch_unwind(|| black_box(1) / black_box(0))).is_err());
    assert!(quiet(|| panic::catch_unwind(|| black_box(i32::MIN) / black_box(-1))).is_err());
    // let x: u8 = 255 + 1;   // error: this arithmetic operation will overflow (deny-by-default lint)
}

// ---------------------------------------------------------------------------
// Q10. `as` casts
// ---------------------------------------------------------------------------
/// ANSWER: `as` is a primitive conversion that never fails and often loses information: integer narrowing TRUNCATES to the low bits;
/// signed<->unsigned reinterprets bits (`-1i32 as u32 == u32::MAX`); widening sign-extends signed and zero-extends unsigned; float -> int SATURATES
/// and maps NaN to 0 (defined since 1.45; earlier UB); int -> float rounds; `u8 as char` is allowed but `u32 as char` is not (use
/// `char::from_u32`). Prefer `From`/`Into` for lossless, `TryFrom` for checked, and `as` only when truncation/reinterpretation is the
/// intent. Casting pointers to integers and back loses provenance.
///
/// REALLY TESTING: silent data loss awareness.
/// WRONG: "`as` panics on overflow" or "`as` is the same as `into`".
/// FOLLOW-UP: clippy lints `cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap`.
fn q10_as_casts() {
    assert_eq!(300i32 as u8, 44);
    assert_eq!(-1i32 as u32, 4_294_967_295);
    assert_eq!(-1i8 as u8, 255);
    assert_eq!(255u8 as i8, -1);
    assert_eq!(-1i8 as i32, -1, "sign extension");
    assert_eq!(255u8 as i32, 255, "zero extension");
    assert_eq!(-1i64 as usize, usize::MAX);
    assert_eq!(3.99f64 as i32, 3, "truncates toward zero");
    assert_eq!(-3.99f64 as i32, -3);
    assert_eq!(f64::NAN as i32, 0);
    assert_eq!(1e20f64 as i32, i32::MAX, "saturates");
    assert_eq!(-1e20f64 as i32, i32::MIN);
    assert_eq!(-5.0f64 as u8, 0);
    assert_eq!(
        16_777_217i32 as f32, 16_777_216.0,
        "f32 cannot represent 2^24 + 1"
    );
    assert_eq!(true as i32 + false as i32, 1);
    assert_eq!(97u8 as char, 'a');
    assert_eq!('€' as u32, 0x20AC);
    assert_eq!('€' as u8, 0xAC, "char -> u8 truncates");
    assert_eq!(char::from_u32(0xD800), None, "surrogates are not chars");
    // Checked alternatives:
    assert!(u8::try_from(300i32).is_err());
    assert_eq!(u8::try_from(200i32), Ok(200));
    assert_eq!(i64::from(7u32), 7);
    // let c = 65u32 as char;   // error[E0604]: only `u8` can be cast as `char`, not `u32`
}

// ---------------------------------------------------------------------------
// Q11. transmute
// ---------------------------------------------------------------------------
/// ANSWER: `mem::transmute::<A, B>(a)` reinterprets the BITS of `a` as `B` (sizes must match at compile time: E0512 otherwise). It is
/// the most dangerous unsafe function: it ignores validity invariants (`transmute::<u8, bool>(2)` is UB, as is making a
/// reference with a longer lifetime or transmuting `Vec<T>` to `Vec<U>` whose layout is unspecified), alignment of references, and
/// `repr(Rust)` struct layout. Use safe tools: `to_bits/from_bits`, `to_ne_bytes/from_ne_bytes`, `as` casts, `bytemuck`/`zerocopy` for
/// plain-old-data, `char::from_u32`, `slice::align_to`, `ptr::cast`. Legitimate uses: function pointers, `MaybeUninit` arrays,
/// FFI with repr(C)/repr(transparent) types.
///
/// REALLY TESTING: whether you reach for transmute first (bad) or last (good).
/// WRONG: "transmute is a faster cast".
/// FOLLOW-UP: `transmute_copy`, why `ptr::read_unaligned`?
fn q11_transmute() {
    // SAFETY: [u8; 4] and u32 have identical size and every bit pattern is a valid u32.
    #[allow(unnecessary_transmutes)] // the lint is the point: a safe alternative exists
    let via_transmute: u32 = unsafe { mem::transmute::<[u8; 4], u32>([1, 2, 3, 4]) };
    let via_safe = u32::from_ne_bytes([1, 2, 3, 4]);
    assert_eq!(via_transmute, via_safe);
    assert_eq!(1.0f32.to_bits(), 0x3f80_0000);
    assert_eq!(f32::from_bits(0x4000_0000), 2.0);
    // FAILING (E0512): transmute::<u32, u64>(1)   // cannot transmute between types of different sizes
    // UB (do not run): let b: bool = unsafe { mem::transmute::<u8, bool>(2) };   // invalid value for bool
    // UB: transmute::<&'a T, &'static T>(r)   // fabricates a lifetime; use Box::leak or restructure
    // Safe validated alternative for the bool case:
    assert_eq!(2u8 != 0, true);
    // fn pointers are a valid use: SAFETY: same ABI and size.
    fn double(x: i32) -> i32 {
        x * 2
    }
    let addr = double as fn(i32) -> i32 as usize;
    // SAFETY: `addr` is the address of a function with exactly this signature obtained just above.
    let f: fn(i32) -> i32 = unsafe { mem::transmute::<usize, fn(i32) -> i32>(addr) };
    assert_eq!(f(4), 8);
}

// ---------------------------------------------------------------------------
// Q12. Pin and self-referential structs
// ---------------------------------------------------------------------------
/// ANSWER: A self-referential struct stores a pointer to its own field. Rust moves are `memcpy`s that do not update pointers, so
/// after a move the pointer would dangle. `Pin<P>` forbids getting `&mut T` (and therefore moving or `mem::swap`ping the pointee)
/// when `T: !Unpin`; `PhantomPinned` makes a type `!Unpin`. Heap-pin with `Box::pin` (the Box can be moved around, the heap value
/// never moves) or stack-pin with `pin!`. Creating the internal pointer needs `unsafe` (`get_unchecked_mut`) with the promise not to
/// move the value afterwards. This is exactly what async state machines rely on (see the async QA file).
///
/// REALLY TESTING: the reason Pin exists, with a concrete example.
/// WRONG: "Pin makes memory immovable at the OS level" / "Pin is only for async".
/// FOLLOW-UP: why is every ordinary struct fine to move? No internal pointers into itself.
struct P<T>(std::marker::PhantomData<T>);
trait NotUnpin {
    const UNPIN: bool = false;
}
impl<T> NotUnpin for P<T> {}
impl<T: Unpin> P<T> {
    const UNPIN: bool = true;
}
struct SelfRef {
    data: String,
    ptr: *const String, // points at `data` once pinned
    _pin: PhantomPinned,
}
impl SelfRef {
    fn new(s: &str) -> Pin<Box<SelfRef>> {
        let mut b = Box::pin(SelfRef {
            data: s.to_string(),
            ptr: std::ptr::null(),
            _pin: PhantomPinned,
        });
        let p: *const String = &b.data;
        // SAFETY: we only write the `ptr` field, and we never move the value out of the pinned box.
        unsafe { b.as_mut().get_unchecked_mut().ptr = p };
        b
    }
    fn data_via_ptr(&self) -> &str {
        // SAFETY: `ptr` was set to the address of `self.data` after pinning; the value cannot move while pinned.
        unsafe { &*self.ptr }
    }
}
fn q12_pin_self_referential() {
    let a = SelfRef::new("hello");
    assert_eq!(a.data_via_ptr(), "hello");
    // Moving the *Pin<Box>* (a pointer) around is fine: the pointee stays put.
    let holder = vec![a];
    let a = holder.into_iter().next().unwrap();
    assert_eq!(a.data_via_ptr(), "hello");
    assert!(
        std::ptr::eq(a.ptr, &a.data),
        "self-pointer still points at the field"
    );
    // FAILING (E0133/E0596 style): you cannot get `&mut SelfRef` from `Pin<Box<SelfRef>>` safely
    //   let r: &mut SelfRef = a.as_mut().get_mut();   // error[E0277]: `PhantomPinned` cannot be unpinned
    //   mem::swap(&mut *a1, &mut *a2);                // would move the data and leave both pointers dangling
    assert!(!P::<SelfRef>::UNPIN && P::<String>::UNPIN);
}
// ---------------------------------------------------------------------------
// Q13. zero-cost abstractions, monomorphisation, inline
// ---------------------------------------------------------------------------
/// ANSWER: "Zero-cost" means high-level constructs (iterators, closures, generics, newtypes, Option) compile to the code you would
/// have written by hand; you pay only for what you use. The price shows up elsewhere: MONOMORPHISATION generates one copy of a generic
/// function per concrete type, so code size and compile time grow (`Vec<u8>`, `Vec<String>`, `Vec<Foo>` are three copies of push/grow...);
/// mitigations: a thin generic wrapper over a non-generic inner function, `dyn Trait` at cold boundaries, `-C opt-level=s`, LTO.
/// `#[inline]` is a HINT enabling cross-crate inlining of small functions (generic functions are already available to inline);
/// `#[inline(always)]` forces it, `#[inline(never)]` forbids it (benchmarks, keeping stack frames). Inlining is not free: it can bloat.
///
/// REALLY TESTING: balanced view (zero runtime cost, nonzero compile-time/binary-size cost).
/// WRONG: "zero-cost means free" / "inline always makes code faster".
/// FOLLOW-UP: how to see the bloat? `cargo bloat`, `cargo llvm-lines`.
fn generic_sum<T: Copy + std::ops::Add<Output = T> + Default>(xs: &[T]) -> T {
    let mut acc = T::default();
    for &x in xs {
        acc = acc + x;
    }
    acc
}
fn name_of<T>(_: &T) -> &'static str {
    std::any::type_name::<T>()
}
#[inline(never)]
fn not_inlined(x: u32) -> u32 {
    x.wrapping_mul(3)
}
#[inline(always)]
fn always_inlined(x: u32) -> u32 {
    x.wrapping_mul(3)
}
struct UserId(u32); // newtype: no runtime cost
fn q13_zero_cost() {
    assert_eq!(generic_sum(&[1u32, 2, 3]), 6);
    assert_eq!(generic_sum(&[0.5f64, 0.25]), 0.75);
    // Distinct instantiations exist as distinct functions:
    let a = generic_sum::<u32> as fn(&[u32]) -> u32 as usize;
    let b = generic_sum::<u64> as fn(&[u64]) -> u64 as usize;
    assert_ne!(a, b);
    assert_eq!(name_of(&1u8), "u8");
    assert_eq!(name_of(&"x"), "&str");
    // Newtypes and Option<&T> are free:
    assert_eq!(size_of::<UserId>(), size_of::<u32>());
    assert_eq!(size_of::<Option<&u32>>(), size_of::<&u32>());
    assert_eq!(not_inlined(5), always_inlined(5));
    // A closure is a zero-sized type when it captures nothing, its call is static:
    let f = |x: u32| x + 1;
    assert_eq!(size_of_val(&f), 0);
    let k = 7;
    let g = move |x: u32| x + k;
    assert_eq!(size_of_val(&g), 4, "captured state is just a field");
    // Thin generic wrapper pattern to limit bloat: one non-generic body shared by all T: AsRef<[u8]>.
    fn count_zeros<T: AsRef<[u8]>>(t: T) -> usize {
        fn inner(b: &[u8]) -> usize {
            b.iter().filter(|x| **x == 0).count()
        }
        inner(t.as_ref())
    }
    assert_eq!(count_zeros(vec![0u8, 1, 0]) + count_zeros([0u8; 2]), 4);
}

// ---------------------------------------------------------------------------
// Q14. Drop, forget, leak
// ---------------------------------------------------------------------------
/// ANSWER: `Drop::drop` runs when an owned value goes out of scope (RAII). Leaking is SAFE: `mem::forget(x)` takes ownership and never
/// runs the destructor; `Rc` cycles, `Box::leak`, `ManuallyDrop`, thread panics in detached threads can all skip Drop. Rust only guarantees
/// memory safety, not that destructors run, so unsafe code must not rely on Drop for SAFETY (the "leakpocalypse" resolved `thread::scoped`
/// by removing it). `ManuallyDrop<T>` suppresses automatic drop and you call `ManuallyDrop::drop` yourself. `Box::leak` gives
/// `&'static mut T` (useful for process-lifetime config). You cannot call `x.drop()` explicitly (E0040): use `drop(x)`.
///
/// REALLY TESTING: "memory safety != no leaks" and RAII limits.
/// WRONG: "forget is unsafe" (it is a safe function) or "Drop always runs".
/// FOLLOW-UP: what is skipped on `process::exit` or `panic = "abort"`? All destructors.
struct Noisy<'a>(&'a Cell<u32>);
impl Drop for Noisy<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn q14_drop_forget_leak() {
    let drops = Cell::new(0);
    {
        let _n = Noisy(&drops);
    }
    assert_eq!(drops.get(), 1, "dropped at scope end");
    mem::forget(Noisy(&drops));
    assert_eq!(
        drops.get(),
        1,
        "forget skipped the destructor (and is a safe function)"
    );
    drop(Noisy(&drops));
    assert_eq!(drops.get(), 2);
    let md = ManuallyDrop::new(Noisy(&drops));
    assert_eq!(drops.get(), 2, "ManuallyDrop does not drop automatically");
    let mut md = md;
    // SAFETY: `md` is dropped exactly once here and never used afterwards.
    unsafe { ManuallyDrop::drop(&mut md) };
    assert_eq!(drops.get(), 3);
    // FAILING (E0040): let n = Noisy(&drops); n.drop();   // explicit destructor calls not allowed

    let leaked: &'static mut String = Box::leak(Box::new(String::from("lives until exit")));
    leaked.push('!');
    assert_eq!(leaked, "lives until exit!");
    // into_raw/from_raw round trip: ownership leaves the Box and must come back exactly once.
    let raw = Box::into_raw(Box::new(5u32));
    // SAFETY: `raw` came from Box::into_raw just above and is reclaimed only once.
    let back = unsafe { Box::from_raw(raw) };
    assert_eq!(*back, 5);
    // Moving out of a Drop type's field is forbidden (E0509): struct has Drop => fields cannot be moved out individually.
}

// ---------------------------------------------------------------------------
// Q15. static vs const, thread_local
// ---------------------------------------------------------------------------
/// ANSWER: `const` is a compile-time VALUE that is inlined (copied) at every use: it has no fixed address, each use creates a fresh
/// instance. `static` is ONE memory location with a single address for the whole program, `'static` lifetime; its initialiser must be
/// const-evaluable and the type must be `Sync` (shared across threads). Mutation requires interior mutability (`AtomicUsize`,
/// `Mutex` via `LazyLock`/`OnceLock`) or `static mut`, which is `unsafe`, easy to get UB with (data races; references to it are denied in
/// edition 2024) and best avoided. `thread_local!` gives each thread its own lazily-initialised copy accessed with `.with(..)`
/// (no `Sync` requirement). Use `const` for constants and array sizes, `static` for global state/large read-only tables.
///
/// REALLY TESTING: address identity and the interior-mutability trap with `const`.
/// WRONG: "const and static are the same, one is just immutable".
/// FOLLOW-UP: why does `const COUNTER: AtomicUsize` silently misbehave? Each use is a NEW atomic (clippy: declare_interior_mutable_const).
const LIMIT: u32 = 10;
static NAME: &str = "global";
static HITS: AtomicUsize = AtomicUsize::new(0);
const BAD_COUNTER: AtomicUsize = AtomicUsize::new(0);
static CONFIG: std::sync::OnceLock<String> = std::sync::OnceLock::new();
thread_local! {
    static LOCAL: Cell<u32> = const { Cell::new(0) };
}
#[allow(const_item_interior_mutations)] // rustc warns about exactly the trap we demonstrate
fn q15_static_const_tls() {
    assert_eq!(LIMIT + 1, 11);
    // static: one address
    let a1 = &HITS as *const AtomicUsize;
    let a2 = &HITS as *const AtomicUsize;
    assert_eq!(a1, a2);
    HITS.fetch_add(1, Ordering::SeqCst);
    HITS.fetch_add(1, Ordering::SeqCst);
    assert_eq!(HITS.load(Ordering::SeqCst), 2);
    // const with interior mutability: every use is a fresh value => the "counter" never counts
    assert_eq!(BAD_COUNTER.fetch_add(1, Ordering::SeqCst), 0);
    assert_eq!(
        BAD_COUNTER.fetch_add(1, Ordering::SeqCst),
        0,
        "a brand-new atomic each time"
    );
    assert_eq!(NAME, "global");
    // static initialised once at runtime:
    assert_eq!(CONFIG.get_or_init(|| "cfg".to_string()), "cfg");
    assert_eq!(CONFIG.get_or_init(|| "other".to_string()), "cfg");
    // FAILING (E0277): static S: std::cell::Cell<i32> = std::cell::Cell::new(0);   // `Cell<i32>` cannot be shared between threads safely
    // thread_local: each thread has its own copy
    LOCAL.with(|c| c.set(5));
    let other = thread::spawn(|| {
        LOCAL.with(|c| {
            assert_eq!(c.get(), 0, "fresh copy in the new thread");
            c.set(99);
        });
        LOCAL.with(|c| c.get())
    })
    .join()
    .unwrap();
    assert_eq!(other, 99);
    assert_eq!(LOCAL.with(|c| c.get()), 5, "our copy is untouched");
}

// ---------------------------------------------------------------------------
// Q16. FFI
// ---------------------------------------------------------------------------
unsafe extern "C" {
    safe fn abs(x: c_int) -> c_int;
    unsafe fn strlen(s: *const c_char) -> usize;
    unsafe fn qsort(
        base: *mut c_void,
        n: usize,
        size: usize,
        cmp: extern "C" fn(*const c_void, *const c_void) -> c_int,
    );
}
extern "C" fn cmp_i32(a: *const c_void, b: *const c_void) -> c_int {
    // SAFETY: qsort passes pointers to elements of the i32 array we handed it; they are valid and aligned.
    let (a, b) = unsafe { (*(a as *const i32), *(b as *const i32)) };
    a.cmp(&b) as c_int
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}
/// ANSWER: FFI uses the C ABI. Declare foreign functions in an `unsafe extern "C" { ... }` block (edition 2024; items may be marked
/// `safe fn` or `unsafe fn`); calling an `unsafe` one needs `unsafe` because the compiler cannot check the C side. Types must be
/// `#[repr(C)]`/primitives. Strings: Rust `&str` is not NUL-terminated, so build a `CString` (fails on interior NUL) and pass
/// `.as_ptr()`; read C strings with `CStr::from_ptr` (unsafe, must be NUL-terminated and valid). Export Rust to C with
/// `#[unsafe(no_mangle)] pub extern "C" fn`. Never let a Rust panic unwind across the boundary (UB; use `catch_unwind` or
/// `extern "C-unwind"`). Ownership must be explicit: whoever allocates frees (`CString::into_raw` / `from_raw`).
///
/// REALLY TESTING: ABI, string handling, ownership across the boundary.
/// WRONG: "just pass `String::as_ptr()` to C" (no NUL terminator).
/// FOLLOW-UP: tools: bindgen (C -> Rust), cbindgen (Rust -> C), `cc` crate for building C in build.rs.
fn q16_ffi() {
    assert_eq!(abs(-5), 5); // declared `safe`: no unsafe needed
    let c = CString::new("hello").unwrap();
    // SAFETY: `c` is a valid NUL-terminated string that outlives the call.
    assert_eq!(unsafe { strlen(c.as_ptr()) }, 5);
    assert!(CString::new("a\0b").is_err(), "interior NUL is rejected");
    let owned = CStr::from_bytes_with_nul(b"abc\0").unwrap();
    assert_eq!(owned.to_str().unwrap(), "abc");
    assert_eq!(c.as_bytes_with_nul().len(), 6);
    let mut v = [5i32, 1, 4, 2, 3];
    // SAFETY: base/size/n describe exactly `v`; the callback only reads two i32 elements.
    unsafe {
        qsort(
            v.as_mut_ptr() as *mut c_void,
            v.len(),
            size_of::<i32>(),
            cmp_i32,
        )
    };
    assert_eq!(v, [1, 2, 3, 4, 5]);
    assert_eq!(rust_add(i32::MAX, 1), i32::MIN);
    // Ownership across the boundary: into_raw gives C a pointer; from_raw takes it back to free it.
    let raw = CString::new("handoff").unwrap().into_raw();
    // SAFETY: `raw` came from CString::into_raw and has not been freed or modified by C.
    let back = unsafe { CString::from_raw(raw) };
    assert_eq!(back.to_str().unwrap(), "handoff");
}

// ---------------------------------------------------------------------------
// Q17. data race vs race condition
// ---------------------------------------------------------------------------
/// ANSWER: A DATA RACE is two threads accessing the same memory concurrently, at least one writing, with no synchronisation: UB in
/// Rust/C++. Safe Rust makes data races IMPOSSIBLE (via `Send`/`Sync` plus the aliasing rules). A RACE CONDITION is a logic bug where the result
/// depends on timing/interleaving of operations; Rust does NOT prevent it. Example: `let v = x.load(); x.store(v + 1)` with atomics has
/// no data race (every access is atomic) but loses updates because the read-modify-write is not one atomic step. Fixes: `fetch_add`,
/// `compare_exchange` loops, or a Mutex covering the whole check-then-act. Other logical races: TOCTOU on files, deadlocks, lock-ordering.
///
/// REALLY TESTING: precise vocabulary, and the limits of Rust's guarantees.
/// WRONG: "Rust prevents all race conditions" / "atomics are always correct".
/// FOLLOW-UP: what does `Ordering::Relaxed` guarantee? Atomicity only, no ordering with other memory ops.
fn q17_data_race_vs_race_condition() {
    // Deterministic lost update: both threads read before either writes (the barrier forces the interleaving).
    let x = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(2));
    let hs: Vec<_> = (0..2)
        .map(|_| {
            let (x, barrier) = (x.clone(), barrier.clone());
            thread::spawn(move || {
                let v = x.load(Ordering::SeqCst); // atomic read
                barrier.wait(); // both threads have now read 0
                x.store(v + 1, Ordering::SeqCst); // atomic write of a stale value
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(
        x.load(Ordering::SeqCst),
        1,
        "two increments, one lost: a race CONDITION without any data race"
    );

    // Fix: one atomic read-modify-write.
    let y = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..2)
        .map(|_| {
            let y = y.clone();
            thread::spawn(move || {
                y.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(y.load(Ordering::SeqCst), 2);

    // The data race itself is rejected at compile time:
    //   let mut n = 0;
    //   thread::scope(|s| { s.spawn(|| n += 1); s.spawn(|| n += 1); });
    //   error[E0499]: cannot borrow `n` as mutable more than once at a time
    // and `Rc`/`Cell` across threads fail with E0277 (not Send / not Sync).
}

fn main() {
    q1_five_superpowers();
    println!("Q1: ok");
    q2_soundness();
    println!("Q2: ok");
    q3_raw_pointers_maybeuninit();
    println!("Q3: ok");
    q4_sizes_and_niches();
    println!("Q4: ok");
    q5_layout_and_repr();
    println!("Q5: ok");
    q6_stack_vs_heap();
    println!("Q6: ok");
    q7_vec_growth();
    println!("Q7: ok");
    q8_string_utf8();
    println!("Q8: ok");
    q9_integer_overflow();
    println!("Q9: ok");
    q10_as_casts();
    println!("Q10: ok");
    q11_transmute();
    println!("Q11: ok");
    q12_pin_self_referential();
    println!("Q12: ok");
    q13_zero_cost();
    println!("Q13: ok");
    q14_drop_forget_leak();
    println!("Q14: ok");
    q15_static_const_tls();
    println!("Q15: ok");
    q16_ffi();
    println!("Q16: ok");
    q17_data_race_vs_race_condition();
    println!("Q17: ok");
}
