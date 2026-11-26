//! RECIPE 1047 (std-ptr): raw pointers, `NonNull`, slices from raw parts, pointer arithmetic, alignment and provenance, each unsafe step justified.
//! LEVEL: advanced
//! Use in: FFI, custom containers, intrusive structures, parsing over raw buffers, reading code like `Vec` or `VecDeque`.   Run: cargo run --release --example std_ptr_nonnull_slices_raw
//! COMMON USES: wrapping a C `(ptr, len)` pair; building a container over raw allocation; O(1) unlink in linked structures; splitting a buffer into disjoint mutable parts.
//! METHODS: ptr::read, ptr::write, ptr::read_unaligned, ptr::write_unaligned, ptr::replace, ptr::swap, ptr::copy, ptr::copy_nonoverlapping,
//! ptr::drop_in_place, ptr::null, ptr::null_mut, ptr::dangling, ptr::from_ref, ptr::from_mut, ptr::eq, ptr::addr_eq, ptr::slice_from_raw_parts,
//! ptr::slice_from_raw_parts_mut, slice::from_raw_parts, slice::from_raw_parts_mut, NonNull::new, NonNull::new_unchecked, NonNull::from, NonNull::dangling,
//! NonNull::as_ptr, NonNull::as_ref, NonNull::as_mut, NonNull::cast, pointer::{is_null, as_ref, add, sub, offset, wrapping_add, offset_from, byte_add,
//! cast, cast_const, cast_mut, is_aligned, align_offset, addr, with_addr, map_addr, expose_provenance}, ptr::with_exposed_provenance, &raw const,
//! &raw mut, std::alloc::{alloc, realloc, dealloc, handle_alloc_error, Layout}, Box::into_raw, Box::from_raw
//! DOCS: https://doc.rust-lang.org/std/ptr/index.html
//! DEFINED IN: core: `core::ptr` (including `NonNull`), the pointer primitive methods, `core::slice::from_raw_parts`, `&raw`, `core::alloc::Layout`.
//!   alloc: the allocation functions (`alloc::alloc`, re-exported as `std::alloc`) and `Box`.
//! NOT COVERED (and why): `NonNull<[T]>` helpers beyond use, `ptr::copy_from` method spellings, `swap_nonoverlapping`, `write_bytes`, volatile reads/writes
//! (memory-mapped I/O), `ptr::metadata` and custom DST construction (unstable), `Unique` (internal), `UnsafeCell` and aliasing models in depth.
//! VERIFICATION: assertions and differential tests against safe code check BEHAVIOUR. They cannot prove soundness. Miri checks the executed paths
//!   for undefined behaviour. Run on 2026-11-26 with rustc 1.100.0-nightly (5a2be9f5f 2026-09-06): no errors under the default (Stacked Borrows) and under
//!   `-Zmiri-tree-borrows -Zmiri-strict-provenance`. Loop counts shrink under `cfg(miri)`, and the exposed-provenance demo is skipped there.
//!
//! THE CHECKLIST for every dereference: non-null; aligned for T; points into ONE live allocation with enough room; the bytes are an initialised, valid T
//! (for reads); no `&mut` aliases it while you read, and nothing else aliases it while you write; you do not create a second owner of the same value.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   a pointer from a reference                    -> `ptr::from_ref(&x)` / `&raw const x`; for fields of packed or uninit structs ONLY `&raw`
//!   move a value out / in without dropping        -> `ptr::read` (now YOU must ensure one owner) / `ptr::write` (old value is NOT dropped)
//!   exchange through pointers                     -> `ptr::replace`, `ptr::swap` (swap allows overlap)
//!   memcpy / memmove                              -> `copy_nonoverlapping` (ranges disjoint) / `copy` (may overlap)
//!   an unaligned field in a byte buffer           -> `read_unaligned`, or better: `u32::from_le_bytes`
//!   (ptr, len) to a slice                         -> `slice::from_raw_parts` (non-null and aligned EVEN IF len == 0)
//!   non-null + covariant, in a struct             -> `NonNull<T>`; empty containers use `NonNull::dangling()`
//!   step through an array                         -> `add` / `sub` (must stay in bounds, one past the end allowed); else `wrapping_add`
//!   distance in elements                          -> `end.offset_from(start)` (same allocation only)
//!   tag bits in a pointer                         -> `map_addr` / `addr` (keeps provenance)
//!   address as an integer and back                -> `expose_provenance` + `with_exposed_provenance` (last resort)
//!
//! PITFALLS: `add` out of bounds is UB even without dereferencing; `ptr::read` of a `String` creates two owners (double free);
//! assigning through a raw pointer (`*p = v`) DROPS the old value, which is UB on uninitialised memory; comparing `dyn Trait` pointers is unreliable.
//! TRY NEXT:
//!   - [S] In `RawVec::push` replace `ptr::write` by `*slot = value` and explain, then confirm with Miri, what is now wrong.
//!   - [S] Why must `RawVec::pop` decrement `len` BEFORE or together with `ptr::read`? Construct the double drop.
//!   - [M] Add `RawVec::insert` and `remove` with `ptr::copy`, and extend the differential test.
//!   - [M] Add `IntoIterator` by value to `RawVec`; test early drop of a half-consumed iterator with the drop counter.
//!   - [A] PROJECT: combine with `std_vecdeque_linkedlist_ring_buffers`: an O(1) LRU from `HashMap<K, Handle>` + `List`, differential-tested and run under Miri.
//! Read: The Rustonomicon, implementing Vec https://doc.rust-lang.org/nomicon/vec/vec.html ; std::ptr safety and provenance https://doc.rust-lang.org/std/ptr/index.html ; Too Many Linked Lists https://rust-unofficial.github.io/too-many-lists/
#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::alloc::{self, Layout};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::slice;

const CASES: usize = if cfg!(miri) { 40 } else { 3_000 };

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

// ------------------------------------------------------------ 1. Making and comparing pointers
fn s01_basics() {
    let mut x = 5;
    let p: *const i32 = ptr::from_ref(&x); // 1.76: states the intent better than `&x as *const _`
    // SAFETY: p comes from a live, aligned, initialised i32 and nothing mutates x while we read.
    assert_eq!(unsafe { *p }, 5);
    let q: *mut i32 = ptr::from_mut(&mut x);
    // SAFETY: q is derived from a unique borrow of x; no other reference to x is used while q is.
    unsafe { q.write(6) };
    assert_eq!(x, 6);

    let null: *const i32 = ptr::null();
    let null_mut: *mut i32 = ptr::null_mut();
    assert!(null.is_null() && null_mut.is_null());
    // SAFETY: as_ref requires "null, or valid for a shared reference". Both inputs satisfy that.
    unsafe {
        assert!(null.as_ref().is_none());
        assert_eq!(ptr::from_ref(&x).as_ref(), Some(&6));
    }
    // dangling: non-null and aligned, but pointing at nothing. Valid only for zero-sized accesses.
    let d: *const u64 = ptr::dangling(); // 1.84
    assert!(!d.is_null() && d.is_aligned());
    assert!(NonNull::<u64>::dangling().as_ptr().is_aligned());

    // equality: addresses, plus metadata for wide pointers
    let arr = [1, 2, 3, 4];
    let (two, three): (&[i32], &[i32]) = (&arr[..2], &arr[..3]);
    assert!(!ptr::eq(two, three), "same address, different length");
    assert!(ptr::addr_eq(two, three), "1.76: addresses only");
    let other = [1, 2];
    assert!(
        two == other && !ptr::eq(two, &other[..]),
        "== compares VALUES through references, ptr::eq compares places"
    );
    // (never compare *const dyn Trait pointers to decide identity: vtables can be duplicated or merged; use addr_eq.)

    let nn = NonNull::new(q).expect("not null");
    assert!(NonNull::new(null_mut).is_none());
    // SAFETY: q was just checked to be non-null.
    let nn2 = unsafe { NonNull::new_unchecked(q) };
    assert_eq!(nn, nn2);
    // the NonNull niche makes Option<NonNull<T>> pointer sized
    assert_eq!(size_of::<Option<NonNull<u8>>>(), size_of::<*const u8>());
    let mut y = 10u32;
    let mut ny = NonNull::from(&mut y);
    // SAFETY: ny points to `y`, which is alive and not otherwise borrowed while these references exist.
    unsafe {
        *ny.as_mut() += 1;
        assert_eq!(*ny.as_ref(), 11);
    }
    // SAFETY: same pointer, same allocation; u8 has weaker alignment and the first byte is initialised.
    let first_byte = unsafe { *ny.cast::<u8>().as_ptr() };
    assert_eq!(first_byte, y.to_ne_bytes()[0]);
    let cp: *const u32 = &raw const y; // 1.82
    let mp: *mut u32 = cp.cast_mut();
    assert_eq!(mp.cast_const(), cp); // casting back to *mut does not by itself grant permission to write
    println!("1. basics ok");
}

// ------------------------------------------------------------ 2. read / write / replace / swap, and ownership
fn s02_read_write() {
    let token = Rc::new(());
    let mut a = Rc::clone(&token);
    // replace: moves the new value in and hands the old one back; the count is unchanged until `old` drops
    // SAFETY: &raw mut a is valid, aligned and initialised.
    let old = unsafe { ptr::replace(&raw mut a, Rc::clone(&token)) };
    assert_eq!(Rc::strong_count(&token), 3);
    drop(old);
    assert_eq!(Rc::strong_count(&token), 2);

    // read creates a bitwise copy: for a moment there are TWO values owning one reference count.
    // SAFETY: `a` is valid to read. To keep a single owner we immediately overwrite the original with `write`,
    // which does not drop it. Nothing between the two calls can panic.
    let moved = unsafe {
        let moved = ptr::read(&raw const a);
        ptr::write(&raw mut a, Rc::new(()));
        moved
    };
    assert_eq!(
        Rc::strong_count(&token),
        2,
        "ownership moved: no extra count, no lost count"
    );
    drop(moved);
    assert_eq!(Rc::strong_count(&token), 1);

    let (mut s, mut t) = (String::from("s"), String::from("t"));
    let (ps, pt) = (&raw mut s, &raw mut t);
    // SAFETY: both pointers are valid, aligned and initialised. swap also permits ps == pt.
    unsafe {
        ptr::swap(ps, pt);
        ptr::swap(ps, ps);
    }
    assert_eq!((s.as_str(), t.as_str()), ("t", "s"));

    // drop_in_place runs the destructor of a value that stays where it is
    let mut slot = std::mem::MaybeUninit::new(Rc::clone(&token));
    // SAFETY: the slot is initialised; after this call it is treated as uninitialised and never read again.
    unsafe { ptr::drop_in_place(slot.as_mut_ptr()) };
    assert_eq!(Rc::strong_count(&token), 1);

    // unaligned access into bytes
    let bytes = [0u8, 1, 0, 0, 0, 2, 0, 0, 0];
    // SAFETY: offsets 1..5 are in bounds and any 4 bytes form a valid u32; read_unaligned has no alignment requirement.
    let v = unsafe { ptr::read_unaligned(bytes.as_ptr().add(1).cast::<u32>()) };
    assert_eq!(v.to_ne_bytes(), [1, 0, 0, 0]);
    assert_eq!(
        v,
        u32::from_ne_bytes(bytes[1..5].try_into().unwrap()),
        "the safe spelling of the same thing"
    );
    let mut out = [0u8; 6];
    // SAFETY: bytes 1..5 of `out` are in bounds; the write needs no alignment.
    unsafe { ptr::write_unaligned(out.as_mut_ptr().add(1).cast::<u32>(), v) };
    assert_eq!(out, [0, 1, 0, 0, 0, 0]);

    // fields of a packed struct may be unaligned: a reference to one is rejected, a raw pointer is fine
    #[repr(C, packed)]
    struct Wire {
        tag: u8,
        len: u32,
    }
    let w = Wire {
        tag: 1,
        len: 0x0A0B_0C0D,
    };
    let len_ptr = &raw const w.len;
    // SAFETY: points to the initialised `len` field of a live struct; read_unaligned tolerates its alignment.
    assert_eq!(unsafe { len_ptr.read_unaligned() }, 0x0A0B_0C0D);
    println!("2. read / write / ownership ok");
}

// ------------------------------------------------------------ 3. Arithmetic and slices from raw parts
/// The usual shape of a safe wrapper over a C style (pointer, length) pair.
///
/// # Safety
/// If `ptr` is non-null it must be aligned and point to `len` initialised `u32`s in one allocation, which stay alive
/// and unmodified for `'a`.
unsafe fn slice_from_c<'a>(ptr: *const u32, len: usize) -> &'a [u32] {
    if ptr.is_null() {
        &[] // C often passes (NULL, 0); from_raw_parts would be UB for that even though the length is zero
    } else {
        // SAFETY: forwarded from this function's contract.
        unsafe { slice::from_raw_parts(ptr, len) }
    }
}

fn s03_arithmetic() {
    let a: [u32; 6] = [10, 11, 12, 13, 14, 15];
    let base = a.as_ptr();
    // SAFETY: every offset below stays within `a` or one past its end, and only in-bounds elements are read.
    unsafe {
        let end = base.add(a.len()); // one past the end: allowed to compute, not to read
        assert_eq!(*base.add(2), 12);
        assert_eq!(*end.sub(1), 15);
        assert_eq!(*base.add(3).offset(-2), 11);
        assert_eq!(end.offset_from(base), 6); // in ELEMENTS
        assert_eq!(base.offset_from(end), -6);
        assert_eq!(*base.byte_add(8), 12); // 1.75: counts BYTES, still typed
        let mut p = base;
        let mut sum = 0;
        while p != end {
            sum += *p;
            p = p.add(1);
        }
        assert_eq!(sum, a.iter().sum::<u32>());
    }
    // wrapping_add may leave the allocation without UB, but the result must not be dereferenced out there
    let far = base.wrapping_add(1_000).wrapping_sub(997);
    // SAFETY: far has the same provenance as base and is back at element 3.
    assert_eq!(unsafe { *far }, 13);

    // SAFETY: a null pointer is explicitly handled; the second call passes a real array and its length.
    unsafe {
        assert_eq!(slice_from_c(ptr::null(), 0), &[] as &[u32]);
        assert_eq!(slice_from_c(a.as_ptr(), 3), [10, 11, 12]);
    }
    // two disjoint mutable slices from one pointer: what split_at_mut does inside
    let mut v = [1, 2, 3, 4, 5];
    let len = v.len();
    let p = v.as_mut_ptr(); // derive BOTH halves from this one pointer
    // SAFETY: [0, 2) and [2, len) are in bounds and do not overlap, and `v` is not used while the slices live.
    let (l, r) = unsafe {
        (
            slice::from_raw_parts_mut(p, 2),
            slice::from_raw_parts_mut(p.add(2), len - 2),
        )
    };
    l[0] = 100;
    r[0] = 300;
    assert_eq!(v, [100, 2, 300, 4, 5]);

    // slice_from_raw_parts builds a raw slice POINTER: safe to create, even with nonsense inside, until dereferenced
    let raw: *const [u32] = ptr::slice_from_raw_parts(a.as_ptr(), 4);
    assert_eq!(raw.len(), 4); // reading the length touches no memory
    // SAFETY: describes the first four elements of the live array `a`.
    assert_eq!(unsafe { &*raw }, [10, 11, 12, 13]);
    let raw_mut: *mut [u32] = ptr::slice_from_raw_parts_mut(ptr::null_mut(), 8);
    assert_eq!(raw_mut.len(), 8, "a raw slice pointer may even be null");
    println!("3. arithmetic / raw slices ok");
}

// ------------------------------------------------------------ 4. Alignment and casting
/// Sums bytes using whole aligned u64 words in the middle. A safe version exists: `slice::align_to` (itself unsafe) or `as_chunks`.
fn sum_bytes_wordwise(data: &[u8]) -> u64 {
    let p = data.as_ptr();
    let head = p.align_offset(align_of::<u64>()).min(data.len()); // may legitimately be usize::MAX: clamp
    let mut sum: u64 = data[..head].iter().map(|&b| u64::from(b)).sum();
    let mut i = head;
    while data.len() - i >= 8 {
        // SAFETY: i + 8 <= len, so all 8 bytes are in bounds. Alignment: if the loop runs after a real align_offset,
        // p + i is aligned; read_unaligned is used so that soundness does not DEPEND on align_offset succeeding.
        let w = unsafe { p.add(i).cast::<u64>().read_unaligned() };
        sum += w.to_ne_bytes().iter().map(|&b| u64::from(b)).sum::<u64>();
        i += 8;
    }
    sum + data[i..].iter().map(|&b| u64::from(b)).sum::<u64>()
}

fn s04_alignment() {
    #[repr(align(8))]
    struct Aligned([u8; 24]);
    let buf = Aligned([7; 24]);
    let p = buf.0.as_ptr();
    assert!(p.cast::<u64>().is_aligned()); // 1.79
    assert!(!p.wrapping_add(1).cast::<u64>().is_aligned());
    assert!(
        p.wrapping_add(1).cast::<u8>().is_aligned(),
        "u8 is always aligned"
    );
    // align_offset is allowed to answer usize::MAX ("cannot"), so assert only what is guaranteed
    let off = p.wrapping_add(3).align_offset(8);
    assert!(off == usize::MAX || p.wrapping_add(3).wrapping_add(off).addr() % 8 == 0);

    let mut rng = Rng(11);
    for case in 0..CASES {
        let data: Vec<u8> = (0..rng.below(100)).map(|_| rng.below(256) as u8).collect();
        let start = rng.below(data.len() as u64 + 1) as usize; // every starting alignment
        let want: u64 = data[start..].iter().map(|&b| u64::from(b)).sum();
        assert_eq!(sum_bytes_wordwise(&data[start..]), want, "case {case}");
    }
    println!("4. alignment ok");
}

// ------------------------------------------------------------ 5. copy and copy_nonoverlapping; reverse in place
/// memmove for a `Copy` type: move `len` elements from index `src` to index `dst` of one buffer.
fn copy_within_raw<T: Copy>(v: &mut [T], src: usize, dst: usize, len: usize) {
    // check with checked arithmetic BEFORE the unsafe block: a wrapped sum would pass a naive bounds check
    assert!(
        src.checked_add(len).is_some_and(|e| e <= v.len())
            && dst.checked_add(len).is_some_and(|e| e <= v.len())
    );
    let p = v.as_mut_ptr();
    // SAFETY: both ranges are in bounds (checked above). They may overlap, so `copy`, not `copy_nonoverlapping`.
    // T: Copy, so duplicating bytes cannot create a second owner.
    unsafe { ptr::copy(p.add(src), p.add(dst), len) }
}

fn memcpy<T: Copy>(dst: &mut [T], src: &[T]) {
    assert_eq!(dst.len(), src.len());
    // SAFETY: equal lengths checked; `&mut` and `&` borrows cannot overlap, which is exactly the non-overlap requirement.
    unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), src.len()) }
}

/// Works for any T, including types with destructors: swapping moves values and never duplicates one.
fn reverse_raw<T>(v: &mut [T]) {
    let len = v.len();
    let p = v.as_mut_ptr();
    for i in 0..len / 2 {
        // SAFETY: i < len / 2, so i and len - 1 - i are distinct in-bounds indices.
        unsafe { ptr::swap(p.add(i), p.add(len - 1 - i)) };
    }
}

fn s05_copy() {
    let mut rng = Rng(22);
    for case in 0..CASES {
        let n = rng.below(40) as usize;
        let base: Vec<u16> = (0..n).map(|_| rng.below(1000) as u16).collect();
        let len = rng.below(n as u64 + 1) as usize;
        let src = rng.below((n - len) as u64 + 1) as usize;
        let dst = rng.below((n - len) as u64 + 1) as usize;
        let (mut a, mut b) = (base.clone(), base.clone());
        copy_within_raw(&mut a, src, dst, len);
        b.copy_within(src..src + len, dst);
        assert_eq!(a, b, "case {case}: n={n} src={src} dst={dst} len={len}");

        let mut d = vec![0u16; n];
        memcpy(&mut d, &base);
        assert_eq!(d, base);

        let mut x: Vec<String> = base.iter().map(ToString::to_string).collect();
        let mut y = x.clone();
        reverse_raw(&mut x);
        y.reverse();
        assert_eq!(x, y, "case {case}");
    }
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    assert!(
        std::panic::catch_unwind(|| copy_within_raw(&mut [0u8; 4], usize::MAX, 0, 2)).is_err(),
        "overflow is rejected"
    );
    std::panic::set_hook(old);
    println!("5. copy / memmove / reverse ok");
}

// ------------------------------------------------------------ 6. A tiny Vec over a raw allocation
/// Invariants: `cap == 0` means `ptr` is dangling and nothing is allocated; otherwise `ptr` is an allocation with
/// layout `array::<T>(cap)`. The first `len <= cap` slots are initialised and owned by this value.
struct RawVec<T> {
    ptr: NonNull<T>,
    len: usize,
    cap: usize,
    _owns: PhantomData<T>, // states ownership of T; NonNull alone also leaves the type !Send and !Sync, which is the safe default
}
impl<T> RawVec<T> {
    fn new() -> Self {
        const {
            assert!(
                size_of::<T>() != 0,
                "this teaching type does not support zero-sized types"
            )
        };
        RawVec {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
            _owns: PhantomData,
        }
    }
    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            4
        } else {
            self.cap.checked_mul(2).expect("capacity overflow")
        };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow"); // rejects sizes above isize::MAX
        let raw = if self.cap == 0 {
            // SAFETY: the layout has non-zero size (T is not zero-sized and new_cap > 0).
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old = Layout::array::<T>(self.cap).expect("was valid when allocated");
            // SAFETY: ptr was allocated by this allocator with layout `old`; the new size is non-zero and, being the size
            // of a valid Layout with the same alignment, within the allowed range.
            unsafe { alloc::realloc(self.ptr.as_ptr().cast(), old, new_layout.size()) }
        };
        // on failure realloc leaves the old block valid, so the invariants still hold when we diverge
        self.ptr =
            NonNull::new(raw.cast::<T>()).unwrap_or_else(|| alloc::handle_alloc_error(new_layout));
        self.cap = new_cap;
    }
    fn push(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: len < cap, so the slot is inside the allocation. It is uninitialised: `write` does not drop it.
        unsafe { self.ptr.as_ptr().add(self.len).write(value) };
        self.len += 1;
    }
    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1; // first: from here on the container no longer owns that slot
        // SAFETY: the slot at the new len was initialised, and is now outside the owned prefix, so it is read exactly once.
        Some(unsafe { self.ptr.as_ptr().add(self.len).read() })
    }
}
impl<T> Deref for RawVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: ptr is non-null and aligned (also when dangling and len == 0); the first len slots are initialised;
        // the returned lifetime is tied to &self, so no mutation can happen meanwhile.
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}
impl<T> DerefMut for RawVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: as in deref, and &mut self makes this the only access.
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}
impl<T> Drop for RawVec<T> {
    fn drop(&mut self) {
        // SAFETY: exactly the first len slots hold live values; they are dropped once, here.
        unsafe { ptr::drop_in_place(ptr::slice_from_raw_parts_mut(self.ptr.as_ptr(), self.len)) };
        // (If an element's destructor panics, drop_in_place still drops the rest, but the buffer below is leaked: safe.)
        if self.cap != 0 {
            let layout = Layout::array::<T>(self.cap).expect("was valid when allocated");
            // SAFETY: allocated by this allocator with this layout, and not used afterwards.
            unsafe { alloc::dealloc(self.ptr.as_ptr().cast(), layout) };
        }
    }
}

fn s06_raw_vec() {
    let token = Rc::new(());
    let mut rng = Rng(33);
    for case in 0..CASES / 10 {
        let mut mine: RawVec<(u64, Rc<()>)> = RawVec::new();
        let mut oracle: Vec<u64> = Vec::new();
        for step in 0..rng.below(100) {
            match rng.below(5) {
                0..=2 => {
                    mine.push((step, Rc::clone(&token)));
                    oracle.push(step);
                }
                3 => assert_eq!(mine.pop().map(|e| e.0), oracle.pop()),
                _ => {
                    mine.reverse(); // a slice method, through DerefMut
                    oracle.reverse();
                }
            }
            assert!(
                mine.iter().map(|e| e.0).eq(oracle.iter().copied()),
                "case {case} step {step}"
            );
            assert_eq!(
                Rc::strong_count(&token),
                1 + oracle.len(),
                "every element has exactly one owner"
            );
        }
        drop(mine);
        assert_eq!(
            Rc::strong_count(&token),
            1,
            "case {case}: Drop released all of them"
        );
    }
    let empty: RawVec<String> = RawVec::new();
    assert!(empty.is_empty());
    println!("6. tiny vec matches Vec ok");
}

// ------------------------------------------------------------ 7. Doubly linked list with O(1) removal by handle
struct Node<T> {
    value: T,
    prev: Option<NonNull<Node<T>>>,
    next: Option<NonNull<Node<T>>>,
}
/// An opaque reference to one element. Copyable, so it can go stale: that is why `remove` is unsafe.
struct Handle<T>(NonNull<Node<T>>);
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}

/// Nodes are Box allocations that never move. The list owns them; all links and handles are raw copies of
/// the pointer returned by Box::into_raw. (A safe alternative: store nodes in a Vec and link by index with generations.)
struct List<T> {
    head: Option<NonNull<Node<T>>>,
    tail: Option<NonNull<Node<T>>>,
    len: usize,
}
impl<T> List<T> {
    fn new() -> Self {
        List {
            head: None,
            tail: None,
            len: 0,
        }
    }
    fn push_back(&mut self, value: T) -> Handle<T> {
        let boxed = Box::new(Node {
            value,
            prev: self.tail,
            next: None,
        });
        // SAFETY: Box::into_raw never returns null.
        let node = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
        match self.tail {
            // SAFETY: tail is a live node of this list; we hold &mut self, so no reference into it exists.
            Some(t) => unsafe { (*t.as_ptr()).next = Some(node) },
            None => self.head = Some(node),
        }
        self.tail = Some(node);
        self.len += 1;
        Handle(node)
    }
    /// # Safety
    /// `h` must have been returned by `push_back` on THIS list, and its element must not have been removed yet.
    unsafe fn remove(&mut self, h: Handle<T>) -> T {
        // SAFETY: by the contract the node is live and owned by this list; taking the Box back transfers ownership to us.
        let node = unsafe { Box::from_raw(h.0.as_ptr()) };
        match node.prev {
            // SAFETY: a neighbour link always points to another live node of this list.
            Some(p) => unsafe { (*p.as_ptr()).next = node.next },
            None => self.head = node.next,
        }
        match node.next {
            // SAFETY: as above.
            Some(n) => unsafe { (*n.as_ptr()).prev = node.prev },
            None => self.tail = node.prev,
        }
        self.len -= 1;
        node.value
    }
    fn pop_front(&mut self) -> Option<T> {
        let head = self.head?;
        // SAFETY: head is by invariant a live node of this list.
        Some(unsafe { self.remove(Handle(head)) })
    }
    fn walk(&self, backwards: bool) -> Vec<&T> {
        let mut out = Vec::new();
        let mut cur = if backwards { self.tail } else { self.head };
        while let Some(p) = cur {
            // SAFETY: every linked node is live; the references are bounded by &self, which blocks removal.
            let n = unsafe { &*p.as_ptr() };
            out.push(&n.value);
            cur = if backwards { n.prev } else { n.next };
        }
        out
    }
}
impl<T> Drop for List<T> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
    }
}

fn s07_list() {
    let token = Rc::new(());
    let mut rng = Rng(44);
    for case in 0..CASES / 20 {
        let mut list = List::new();
        let mut model: Vec<(u64, Handle<(u64, Rc<()>)>)> = Vec::new(); // exactly the live handles, in list order
        for step in 0..rng.below(80) {
            match rng.below(4) {
                0 | 1 => model.push((step, list.push_back((step, Rc::clone(&token))))),
                2 if !model.is_empty() => {
                    let (want, h) = model.remove(rng.below(model.len() as u64) as usize);
                    // SAFETY: `model` holds only handles of this list whose elements are still linked, and this one
                    // has just been removed from `model`, so it cannot be used twice.
                    assert_eq!(unsafe { list.remove(h) }.0, want);
                }
                _ => {
                    let want = (!model.is_empty()).then(|| model.remove(0).0);
                    assert_eq!(list.pop_front().map(|e| e.0), want);
                }
            }
            let want: Vec<u64> = model.iter().map(|m| m.0).collect();
            assert_eq!(
                list.walk(false).iter().map(|e| e.0).collect::<Vec<_>>(),
                want,
                "case {case} step {step}"
            );
            let mut back: Vec<u64> = list.walk(true).iter().map(|e| e.0).collect();
            back.reverse();
            assert_eq!(back, want, "prev links agree");
            assert_eq!(
                (list.len, Rc::strong_count(&token)),
                (want.len(), 1 + want.len())
            );
        }
        drop(list);
        assert_eq!(Rc::strong_count(&token), 1);
    }
    println!("7. intrusive-style list ok");
}

// ------------------------------------------------------------ 8. Provenance
fn s08_provenance() {
    // A pointer is an address PLUS the right to access a particular allocation (its provenance). An integer carries no
    // provenance. Strict provenance APIs (1.84) keep the two together.
    #[repr(align(4))]
    struct Slot(u32);
    let slot = Slot(77);
    let p: *const Slot = &raw const slot;
    // pointer tagging: the two low bits of an aligned address are free
    let tagged = p.map_addr(|a| a | 0b1);
    assert_eq!(tagged.addr() & 0b11, 0b1);
    assert!(!tagged.is_aligned());
    let clean = tagged.map_addr(|a| a & !0b11);
    assert_eq!(clean, p);
    // SAFETY: `clean` has p's address and provenance, and `slot` is alive.
    assert_eq!(unsafe { (*clean).0 }, 77);
    // with_addr: take the address from an integer and the provenance from an existing pointer
    let arr = [1u8, 2, 3, 4];
    let base = arr.as_ptr();
    let third = base.with_addr(base.addr() + 2);
    // SAFETY: the new address is inside `arr`, which `base` is allowed to access.
    assert_eq!(unsafe { *third }, 3);
    // Exposed provenance is the escape hatch when the integer must travel on its own (e.g. through a C API's
    // "user data" integer). It is weaker: tools such as Miri cannot check it precisely and warn about it.
    #[cfg(not(miri))]
    {
        let as_int: usize = base.expose_provenance();
        let back: *const u8 = ptr::with_exposed_provenance(as_int + 3);
        // SAFETY: the provenance of `arr` was exposed above, the address is in bounds, and `arr` is alive.
        assert_eq!(unsafe { *back }, 4);
    }
    println!("8. provenance ok");
}

fn main() {
    s01_basics();
    s02_read_write();
    s03_arithmetic();
    s04_alignment();
    s05_copy();
    s06_raw_vec();
    s07_list();
    s08_provenance();
    println!("all pointer sections passed");
}
