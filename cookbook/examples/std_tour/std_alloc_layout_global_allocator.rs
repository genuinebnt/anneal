//! RECIPE 1031 (std): the core / alloc / std layers, Layout arithmetic, the raw allocation API and a counting
//! #[global_allocator].
//! LEVEL: advanced
//! Use in: no_std libraries, allocation budgets in tests, memory metrics, arenas and custom containers.   Run: cargo run --release --example std_alloc_layout_global_allocator
//! COMMON USES: firmware and WASM crates that avoid std, "this hot path must not allocate" tests, per-process
//! allocation metrics, hand-rolled buffers with special alignment (SIMD, direct I/O).
//! METHODS: (everyday first) std::mem::size_of, std::mem::align_of, std::mem::offset_of, std::alloc::Layout::new, Layout::size, Layout::align,
//! METHODS: Layout::from_size_align, Layout::array, Layout::for_value, Layout::extend, Layout::pad_to_align, Layout::align_to, Layout::repeat,
//! METHODS: Layout::repeat_packed, Layout::extend_packed, std::alloc::alloc, std::alloc::alloc_zeroed, std::alloc::realloc,
//! METHODS: std::alloc::dealloc, std::alloc::handle_alloc_error, std::alloc::GlobalAlloc, std::alloc::System, std::alloc::LayoutError,
//! METHODS: std::ptr::NonNull::new, std::ptr::NonNull::dangling, std::hint::black_box, std::mem::size_of_val
//! DOCS: https://doc.rust-lang.org/std/alloc/index.html
//! DEFINED IN: core (`core::alloc`: Layout, LayoutError, GlobalAlloc; `core::mem`; `core::ptr`). alloc (`alloc::alloc`:
//! the free functions alloc/alloc_zeroed/realloc/dealloc/handle_alloc_error and every owning heap type). std: `System`.
//! `#[global_allocator]` is a language attribute.
//! NOT COVERED (and why): the Allocator trait, Global as a value, Box::new_in / Vec::new_in (allocator_api, unstable in
//! 1.98.1: with it a collection takes its allocator as a type parameter; today crates such as allocator-api2 or bumpalo
//! fill the gap), alloc_error_hook (unstable), unchecked Layout constructors, Layout::dangling_ptr.
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   size / alignment of a type              -> size_of::<T>() / align_of::<T>()
//!   where a field starts                    -> offset_of!(Type, field)
//!   layout of T / [T; n]                    -> Layout::new::<T>() / Layout::array::<T>(n)
//!   header followed by data                 -> let (l, off) = header.extend(data)?; l.pad_to_align()
//!   raw heap memory                         -> alloc(layout), check null, dealloc(ptr, SAME layout)
//!   zeroed memory                           -> alloc_zeroed
//!   grow or shrink                          -> realloc(ptr, old_layout, new_size)
//!   allocation failed                       -> handle_alloc_error(layout)
//!   count or limit allocations              -> impl GlobalAlloc wrapping System + #[global_allocator]
//!   a dangling pointer for zero capacity    -> NonNull::<T>::dangling()
//!
//! LAYERS. core: no heap, no OS (Option, Result, slices, str, fmt, atomics, Layout). alloc: adds a heap (Box, Vec, String,
//! Rc, Arc, BTreeMap, format!). std: re-exports both and adds the OS (files, sockets, threads, Mutex, HashMap, process).
//! A `#![no_std]` crate uses core, plus `extern crate alloc;` when an allocator exists.
//!
//! ALLOCATOR CONTRACT (core GlobalAlloc docs): alloc with a zero-sized layout is undefined behaviour; an allocator must not
//! unwind; and you must NOT rely on a source-level allocation actually happening, because the optimiser may remove it.
//! The counts below therefore pass values through black_box, are asserted as lower bounds or "none at all", and describe
//! this build rather than a language guarantee.
//!
//! Read: https://doc.rust-lang.org/std/alloc/index.html , https://doc.rust-lang.org/std/alloc/struct.Layout.html ,
//!       https://doc.rust-lang.org/reference/type-layout.html
//! TRY NEXT:
//!   - [S] Reorder the fields of `CRecord` and predict the new size before running.
//!   - [S] Remove `#[repr(C)]` and see which asserts are no longer guaranteed (even if they still pass).
//!   - [M] Add a byte LIMIT to the allocator that returns null, then see what `Vec::try_reserve` and `Vec::push` do.
//!   - [M] Extend `RawBuf` with `pop`, and run it under Miri if you have a nightly toolchain.
//!   - [A] PROJECT: an allocation-budget test helper using this counter, applied to the formatting paths in
//!         std_string_building_utf8 and std_fmt_display_debug_deep.
use std::alloc::{
    GlobalAlloc, Layout, LayoutError, System, alloc, alloc_zeroed, dealloc, handle_alloc_error,
    realloc,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::hint::black_box;
use std::mem::{align_of, offset_of, size_of, size_of_val};
use std::ptr::NonNull;

const COVERED: &[&str] = &[
    "size_of",
    "align_of",
    "offset_of",
    "new",
    "size",
    "align",
    "from_size_align",
    "array",
    "for_value",
    "extend",
    "pad_to_align",
    "align_to",
    "repeat",
    "repeat_packed",
    "extend_packed",
    "alloc",
    "alloc_zeroed",
    "realloc",
    "dealloc",
    "handle_alloc_error",
    "GlobalAlloc",
    "System",
    "LayoutError",
    "dangling",
    "black_box",
    "size_of_val",
];

// ---- The counting allocator ------------------------------------------------------------------------------------
const BUCKETS: usize = 16;
#[derive(Clone, Copy, Debug)]
struct Stats {
    allocs: u64,
    deallocs: u64,
    reallocs: u64,
    live_bytes: i64,
    /// histogram[i] counts requests with size in (2^(i-1), 2^i]; the last bucket takes everything larger.
    histogram: [u64; BUCKETS],
}
const EMPTY: Stats = Stats {
    allocs: 0,
    deallocs: 0,
    reallocs: 0,
    live_bytes: 0,
    histogram: [0; BUCKETS],
};

thread_local! {
    // Per thread, so other threads cannot disturb a measurement. `const` init means reading it never allocates,
    // which matters: an allocator that allocates while counting would recurse.
    static STATS: Cell<Stats> = const { Cell::new(EMPTY) };
}

fn bucket(size: usize) -> usize {
    (size.max(1).next_power_of_two().trailing_zeros() as usize).min(BUCKETS - 1)
}
fn record(f: impl FnOnce(&mut Stats)) {
    // try_with: the allocator can be called during thread teardown; it must never panic.
    let _ = STATS.try_with(|c| {
        let mut s = c.get();
        f(&mut s);
        c.set(s);
    });
}

struct Counting;
// SAFETY: every method forwards the unchanged arguments to `System`, which upholds the GlobalAlloc contract. The
// bookkeeping neither allocates nor unwinds (wrapping arithmetic, try_with, in-bounds bucket index).
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's obligations (non-zero size) are passed straight through.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            record(|s| {
                s.allocs = s.allocs.wrapping_add(1);
                s.live_bytes = s.live_bytes.wrapping_add(layout.size() as i64);
                s.histogram[bucket(layout.size())] += 1;
            });
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(|s| {
            s.deallocs = s.deallocs.wrapping_add(1);
            s.live_bytes = s.live_bytes.wrapping_sub(layout.size() as i64);
        });
        // SAFETY: ptr and layout come from our alloc/realloc, which came from System.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: same pointer, layout and size the caller promised to be valid.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            record(|s| {
                s.reallocs = s.reallocs.wrapping_add(1);
                s.live_bytes = s
                    .live_bytes
                    .wrapping_add(new_size as i64 - layout.size() as i64);
            });
        }
        p
    }
    // alloc_zeroed keeps its default (alloc + zero), so it is counted through `alloc`.
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Runs f and returns what this thread allocated during it (as differences).
fn measure<T>(f: impl FnOnce() -> T) -> (T, Stats) {
    let before = STATS.get();
    let out = f();
    let after = STATS.get();
    let diff = Stats {
        allocs: after.allocs - before.allocs,
        deallocs: after.deallocs - before.deallocs,
        reallocs: after.reallocs - before.reallocs,
        live_bytes: after.live_bytes - before.live_bytes,
        histogram: std::array::from_fn(|i| after.histogram[i] - before.histogram[i]),
    };
    (out, diff)
}

// ---- 1. Everyday core: size_of, align_of, offset_of ------------------------------------------------------------
#[repr(C)]
struct CRecord {
    tag: u8,
    len: u64,
    flag: u16,
}
/// Default representation: the compiler may reorder fields; sizes are not guaranteed.
struct RustRecord {
    tag: u8,
    len: u64,
    flag: u16,
}

fn section_sizes() {
    assert_eq!(
        (
            size_of::<u8>(),
            size_of::<u32>(),
            size_of::<u64>(),
            size_of::<[u16; 5]>()
        ),
        (1, 4, 8, 10)
    );
    // repr(C): fields in order, each at the next multiple of its alignment; total rounded up to the max alignment.
    let a = align_of::<u64>(); // 8 on common 64-bit targets, may be 4 on some 32-bit ones
    assert_eq!(
        (
            offset_of!(CRecord, tag),
            offset_of!(CRecord, len),
            offset_of!(CRecord, flag)
        ),
        (0, a, a + 8)
    ); // offset_of! since 1.77
    assert_eq!(size_of::<CRecord>(), (a + 10).next_multiple_of(a));
    assert_eq!(align_of::<CRecord>(), a);
    println!(
        "   CRecord={}B RustRecord={}B (the second is this compiler's choice)",
        size_of::<CRecord>(),
        size_of::<RustRecord>()
    );
    // Fat pointers carry a length or a vtable.
    let word = size_of::<usize>();
    assert_eq!(
        (
            size_of::<&u8>(),
            size_of::<&[u8]>(),
            size_of::<&str>(),
            size_of::<&dyn Fn()>()
        ),
        (word, 2 * word, 2 * word, 2 * word)
    );
    let data = [0u32; 6];
    assert_eq!(size_of_val(&data[..]), 24); // size of an unsized value, known at run time
    println!("1. sizes and offsets ok");
}

// ---- 2. Layout arithmetic --------------------------------------------------------------------------------------
/// Computes a repr(C) struct layout from its fields, exactly as in the Layout::extend docs.
fn repr_c(fields: &[Layout]) -> Result<(Layout, Vec<usize>), LayoutError> {
    let mut layout = Layout::from_size_align(0, 1)?;
    let mut offsets = Vec::with_capacity(fields.len());
    for &f in fields {
        let (next, off) = layout.extend(f)?; // inserts padding BEFORE the field
        layout = next;
        offsets.push(off);
    }
    Ok((layout.pad_to_align(), offsets)) // trailing padding is a separate step: easy to forget
}

fn section_layout() {
    let l = Layout::new::<CRecord>();
    assert_eq!(
        (l.size(), l.align()),
        (size_of::<CRecord>(), align_of::<CRecord>())
    );
    let fields = [
        Layout::new::<u8>(),
        Layout::new::<u64>(),
        Layout::new::<u16>(),
    ];
    let (computed, offs) = repr_c(&fields).unwrap();
    assert_eq!(computed, l);
    assert_eq!(
        offs,
        [
            offset_of!(CRecord, tag),
            offset_of!(CRecord, len),
            offset_of!(CRecord, flag)
        ]
    );

    assert_eq!(
        Layout::array::<u32>(10).unwrap(),
        Layout::new::<[u32; 10]>()
    );
    assert!(Layout::array::<u64>(usize::MAX).is_err()); // overflow is an Err, not a wrap
    // from_size_align: align must be a non-zero power of two, and size rounded up must fit in isize.
    assert!(Layout::from_size_align(8, 8).is_ok());
    assert!(Layout::from_size_align(8, 0).is_err() && Layout::from_size_align(8, 3).is_err());
    assert!(Layout::from_size_align(isize::MAX as usize, 2).is_err());
    let value = String::from("x");
    assert_eq!(Layout::for_value(&value), Layout::new::<String>()); // the String header, not its heap text
    assert_eq!(Layout::for_value("abc").size(), 3);
    // align_to raises the alignment (for example to a cache line); the size is unchanged until pad_to_align.
    let line = Layout::new::<[u8; 10]>().align_to(64).unwrap();
    assert_eq!(
        (line.size(), line.align(), line.pad_to_align().size()),
        (10, 64, 64)
    );
    // repeat (since 1.95): n padded copies plus the stride between them. repeat_packed: no padding at all.
    let item = Layout::from_size_align(6, 4).unwrap();
    let (three, stride) = item.repeat(3).unwrap();
    assert_eq!((stride, three.align()), (8, 4));
    assert!(three.size() >= 2 * stride + item.size() && three.size() <= 3 * stride);
    assert_eq!(item.repeat_packed(3).unwrap().size(), 18);
    // extend_packed (since 1.95): append with no padding, keeping self's alignment.
    let packed = Layout::new::<u8>()
        .extend_packed(Layout::new::<u64>())
        .unwrap();
    assert_eq!((packed.size(), packed.align()), (9, 1));
    println!("2. Layout arithmetic ok");
}

// ---- 3. Raw allocation: a minimal growable buffer --------------------------------------------------------------
/// A push-only Vec<u64>, written to show every obligation of the raw API. Use Vec in real code.
struct RawBuf {
    ptr: NonNull<u64>,
    cap: usize,
    len: usize,
}
impl RawBuf {
    fn new() -> Self {
        RawBuf {
            ptr: NonNull::dangling(),
            cap: 0,
            len: 0,
        } // no allocation for capacity 0
    }
    fn layout(cap: usize) -> Layout {
        Layout::array::<u64>(cap).expect("capacity overflow")
    }
    fn push(&mut self, x: u64) {
        if self.len == self.cap {
            let new_cap = self.cap.checked_mul(2).expect("capacity overflow").max(4);
            let new_layout = Self::layout(new_cap);
            let raw = if self.cap == 0 {
                // SAFETY: new_layout has non-zero size because new_cap >= 4 and u64 is not zero-sized.
                unsafe { alloc(new_layout) }
            } else {
                // SAFETY: ptr was allocated by this allocator with layout(self.cap); the new size is non-zero and,
                // having come from a valid Layout, does not overflow isize when rounded to the alignment.
                unsafe {
                    realloc(
                        self.ptr.as_ptr().cast(),
                        Self::layout(self.cap),
                        new_layout.size(),
                    )
                }
            };
            // Null means failure. On a failed realloc the OLD block is still valid.
            let Some(ptr) = NonNull::new(raw.cast::<u64>()) else {
                handle_alloc_error(new_layout)
            };
            self.ptr = ptr;
            self.cap = new_cap;
        }
        // SAFETY: len < cap, so the slot is inside the allocation; u64 needs no drop of the old (uninitialised) value.
        unsafe { self.ptr.as_ptr().add(self.len).write(x) };
        self.len += 1;
    }
    fn as_slice(&self) -> &[u64] {
        // SAFETY: the first len elements are initialised; for len == 0 a dangling, aligned, non-null pointer is valid.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}
impl Drop for RawBuf {
    fn drop(&mut self) {
        if self.cap != 0 {
            // SAFETY: allocated by this allocator with exactly this layout, and not freed before.
            unsafe { dealloc(self.ptr.as_ptr().cast(), Self::layout(self.cap)) }
        }
    }
}

fn section_raw() {
    let ((), s) = measure(|| {
        let mut b = RawBuf::new();
        assert!(b.as_slice().is_empty());
        for i in 0..1000 {
            b.push(black_box(i));
        }
        assert!(b.as_slice().iter().copied().eq(0..1000));
        black_box(&b);
    });
    assert_eq!(s.live_bytes, 0, "everything was freed: {s:?}");
    assert!(
        s.allocs >= 1 && s.reallocs >= 1 && s.allocs == s.deallocs,
        "{s:?}"
    );

    let layout = Layout::array::<u8>(256).unwrap();
    // SAFETY: the layout has non-zero size.
    let p = unsafe { alloc_zeroed(layout) };
    if p.is_null() {
        handle_alloc_error(layout);
    }
    // SAFETY: p points to 256 bytes that alloc_zeroed initialised.
    let bytes = unsafe { std::slice::from_raw_parts(black_box(p), 256) };
    assert!(bytes.iter().all(|&b| b == 0));
    // SAFETY: p came from alloc_zeroed with this layout; `bytes` is not used after this line.
    unsafe { dealloc(p, layout) };
    println!("3. raw alloc/realloc/dealloc ok");
}

// ---- 4. Zero-sized and over-aligned allocations ----------------------------------------------------------------
struct Marker;
#[repr(align(4096))]
struct Page([u8; 100]);

fn section_zst_and_align() {
    // Zero-sized values never reach the allocator (calling alloc with size 0 yourself would be undefined behaviour).
    let (kept, s) = measure(|| {
        let b = black_box(Box::new(Marker));
        let mut v: Vec<()> = Vec::new();
        (0..10_000).for_each(|_| v.push(()));
        let empty_vec: Vec<u64> = Vec::new();
        let empty_string = String::new();
        black_box((b, v, empty_vec, empty_string))
    });
    assert_eq!(
        (s.allocs, s.reallocs, kept.1.len()),
        (0, 0, 10_000),
        "{s:?}"
    );
    assert_eq!(size_of::<Box<Marker>>(), size_of::<usize>());

    // Over-aligned types: the size is rounded up to a multiple of the alignment, and Box honours both.
    assert_eq!((align_of::<Page>(), size_of::<Page>()), (4096, 4096));
    let pages: Vec<Box<Page>> = (0..4).map(|i| Box::new(Page([i; 100]))).collect();
    for p in &pages {
        assert_eq!(std::ptr::from_ref::<Page>(p) as usize % 4096, 0);
    }
    let v: Vec<Page> = vec![Page([0; 100]), Page([1; 100])];
    assert_eq!(v.as_ptr() as usize % 4096, 0);
    println!("4. zero-sized and over-aligned ok");
}

// ---- 5. Measuring what std types allocate ----------------------------------------------------------------------
fn section_measure() {
    let (v, grown) = measure(|| {
        let mut v = Vec::new();
        for i in 0..10_000u64 {
            v.push(black_box(i));
        }
        v
    });
    let (w, sized) = measure(|| {
        let mut w = Vec::with_capacity(10_000);
        for i in 0..10_000u64 {
            w.push(black_box(i));
        }
        w
    });
    assert_eq!(v, w);
    assert_eq!((sized.allocs, sized.reallocs), (1, 0), "{sized:?}");
    assert!(
        grown.reallocs >= 5,
        "growth reallocates repeatedly: {grown:?}"
    );
    assert!(sized.live_bytes >= 80_000);
    assert_eq!(sized.histogram[bucket(80_000)], 1);

    // A hot path that must not allocate: parse and sum borrowing from the input.
    let line = black_box(String::from("1,2,3,4"));
    let (sum, s) = measure(|| {
        line.split(',')
            .filter_map(|x| x.parse::<u32>().ok())
            .sum::<u32>()
    });
    assert_eq!((sum, s.allocs), (10, 0));
    let (parts, s) = measure(|| black_box(line.split(',').map(String::from).collect::<Vec<_>>()));
    assert!(parts.len() == 4 && s.allocs >= 5, "{s:?}"); // four Strings and the Vec
    println!("   histogram of the pre-sized run: {:?}", sized.histogram);
    println!("5. allocation measurement ok");
}

// ---- 6. Writing code that only needs core ----------------------------------------------------------------------
/// This module names only `core::` paths, so it can be moved into a `#![no_std]` crate unchanged. It is tested here,
/// under std. (Compiling inside a std program does not by itself prove that; a no_std build in CI does.)
mod only_core {
    use core::fmt::{self, Write};

    /// fmt::Write over a caller-provided buffer: formatting with no heap.
    pub struct SliceWriter<'a> {
        buf: &'a mut [u8],
        len: usize,
    }
    impl<'a> SliceWriter<'a> {
        pub fn new(buf: &'a mut [u8]) -> Self {
            SliceWriter { buf, len: 0 }
        }
        pub fn as_str(&self) -> &str {
            // Only whole &str values are copied in, so the prefix is always valid UTF-8.
            core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
        }
    }
    impl Write for SliceWriter<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let end = self.len.checked_add(s.len()).ok_or(fmt::Error)?;
            let dst = self.buf.get_mut(self.len..end).ok_or(fmt::Error)?;
            dst.copy_from_slice(s.as_bytes());
            self.len = end;
            Ok(())
        }
    }
    pub fn render_reading(out: &mut SliceWriter<'_>, sensor: u8, milli: i32) -> fmt::Result {
        let sign = if milli < 0 { "-" } else { "" };
        let m = milli.unsigned_abs();
        write!(out, "s{sensor}={sign}{}.{:03}", m / 1000, m % 1000)
    }
}

fn section_core_only() {
    let mut buf = [0u8; 16];
    let (text_ok, s) = measure(|| {
        let mut w = only_core::SliceWriter::new(&mut buf);
        only_core::render_reading(&mut w, black_box(3), black_box(-1005)).is_ok()
            && w.as_str() == "s3=-1.005"
    });
    assert!(text_ok);
    assert_eq!(s.allocs, 0, "no heap use");
    let mut tiny = [0u8; 4];
    let mut w = only_core::SliceWriter::new(&mut tiny);
    assert!(only_core::render_reading(&mut w, 3, 1).is_err()); // full buffer is an Err, never a panic
    assert!(w.as_str().len() <= 4);
    println!("6. core-only code ok");
}

// ---- 7. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Do not call the raw API when Vec, Box<[T]> or Box<MaybeUninit<T>> will do.
    // 2. Free with the SAME layout you allocated with; store the capacity, not a guess.
    // 3. Check every alloc result for null, then call handle_alloc_error.
    // 4. Compute layouts with checked Layout methods, never `n * size_of::<T>()` (overflow gives a too-small block).
    assert!(Layout::array::<[u8; 1024]>(usize::MAX / 2).is_err());
    // 5. Zero size: branch before allocating (RawBuf::new, section 3).
    // 6. A global allocator is a per-binary decision: libraries must not declare one.
    // 7. Counting allocators with shared atomics cost throughput; keep them to tests or sample them.
    // 8. Every unsafe block carries a SAFETY comment (clippy::undocumented_unsafe_blocks, restriction group).
    // 9. Run unsafe containers under Miri and sanitizers in CI.
    // 10. A no_std claim needs a no_std build in CI (a target without std), not just a std test run.
    let (b, s) = measure(|| black_box(Box::new(black_box([0u8; 64]))));
    assert_eq!((s.allocs, b.len()), (1, 64));
    println!("7. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_sizes();
    section_layout();
    section_raw();
    section_zst_and_align();
    section_measure();
    section_core_only();
    section_production_tips();
    println!(
        "covered {} alloc/layout names; all asserts passed",
        COVERED.len()
    );
}
