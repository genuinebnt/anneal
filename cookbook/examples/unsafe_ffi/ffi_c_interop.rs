//! RECIPE 233 (ffi): C interop both ways with only `libc`: extern "C" calls, callbacks, opaque handles, CString ownership, panic safety.
//! LEVEL: advanced
//! Use in: wrapping C libraries, exposing a Rust library to C/Python/Swift, plugin ABIs, anything crossing a language boundary.   Run: cargo run --release --example ffi_c_interop
#![allow(dead_code)]
//!
//! WHAT: the C ABI is the lingua franca between languages. Two directions:
//!   (1) Rust calls C: declare the function in an `unsafe extern "C" { ... }` block (mandatory `unsafe` in edition 2024), then every call
//!       is `unsafe` because the compiler cannot check C's promises. We call `strlen`, `getenv`, `qsort`, `snprintf` (variadic) and
//!       `gettimeofday` (fills a `#[repr(C)]` struct, `libc::timeval`) straight from the platform libc, no C compiler needed.
//!   (2) C calls Rust: `#[unsafe(no_mangle)] pub extern "C" fn` gives an unmangled symbol with the C calling convention. Here the "C
//!       caller" is simulated by a table of plain function pointers (`CApi`), which is exactly what a C program sees via dlsym/headers.
//!
//! RULES THAT PREVENT UB:
//!   - Ownership: memory is freed by the allocator that made it. `CString::into_raw` hands Rust-allocated memory to C; it must come back
//!     through `CString::from_raw` (our `ff_string_free`), NEVER libc `free`. Likewise memory from `malloc` must not go to `Box`/`drop`.
//!   - Opaque handle: Rust type boxed, exposed as `*mut Counter` (C sees an incomplete type). `_new` = `Box::into_raw`, `_free` =
//!     `Box::from_raw`. A safe Rust wrapper (`SafeCounter`) owns the pointer and calls `_free` in `Drop`, so leaks and double frees are impossible.
//!   - Panics must not unwind across `extern "C"` (since Rust 1.81 that aborts the process). Wrap callback bodies in `catch_unwind` and
//!     translate to an error code.
//!   - Enums: a Rust enum with an invalid discriminant is UB. Accept the raw integer at the boundary and validate with `TryFrom`.
//!   - Slices are `(ptr, len)`; check null, then `slice::from_raw_parts` (ptr must be non-null and aligned even when len is 0).
//!   - Out-parameters: `*mut T` the callee writes through; validate non-null, return a status code. `Option<extern "C" fn>` is the
//!     nullable function pointer (same ABI as a C pointer, `None` is NULL). Errors from C: read `errno` immediately via `io::Error::last_os_error()`.
//!   - qsort has no user-context argument (`qsort_r`'s signature differs between glibc and BSD), so the comparator uses a global counter;
//!     our own `ff_counter_visit` shows the portable `(callback, void* ctx)` pattern.
//!
//! AUTOMATION: writing these declarations by hand does not scale. `bindgen` reads C headers and generates the Rust `extern` blocks and
//! `#[repr(C)]` structs; `cbindgen` reads your Rust `extern "C"` items and generates the C header. Use them in `build.rs` / CI.
//! COMMON USES: sys crates (`libz-sys`, `openssl-sys` style) wrapped by safe crates, Rust cores embedded in Python/Node/Swift apps via a C ABI,
//!   plugin systems with C callbacks and `void*` user data, and Rust components dropped into existing C codebases.
//! Read: https://doc.rust-lang.org/nomicon/ffi.html , https://doc.rust-lang.org/std/panic/fn.catch_unwind.html , https://github.com/rust-lang/rust-bindgen , https://github.com/mozilla/cbindgen
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::io;
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------- (1) Rust calls C

unsafe extern "C" {
    fn strlen(s: *const c_char) -> usize;
    fn getenv(name: *const c_char) -> *const c_char;
    fn qsort(
        base: *mut c_void,
        n: usize,
        size: usize,
        cmp: Option<extern "C" fn(*const c_void, *const c_void) -> c_int>,
    );
    fn snprintf(buf: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
}

fn c_strlen(s: &CStr) -> usize {
    // SAFETY: `CStr` guarantees a valid NUL-terminated string that outlives this call.
    unsafe { strlen(s.as_ptr()) }
}

fn c_getenv(name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    // SAFETY: `name` is a valid C string. getenv returns NULL or a pointer into the environment block.
    let p = unsafe { getenv(name.as_ptr()) };
    if p.is_null() {
        return None;
    }
    // SAFETY: non-null and NUL-terminated per getenv's contract; we copy it out immediately (no other thread mutates env here).
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

static COMPARISONS: AtomicUsize = AtomicUsize::new(0);

/// qsort comparator: a Rust fn with the C ABI. Must never panic (would abort), hence only trivial code.
extern "C" fn cmp_i32(a: *const c_void, b: *const c_void) -> c_int {
    COMPARISONS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: qsort passes pointers to two elements of the i32 array we gave it; they are valid and aligned for i32.
    let (a, b) = unsafe { (*(a as *const i32), *(b as *const i32)) };
    (a > b) as c_int - (a < b) as c_int
}

fn c_qsort(v: &mut [i32]) {
    // SAFETY: pointer/len/size describe exactly the live, exclusively borrowed slice; cmp_i32 reads only i32 elements.
    unsafe {
        qsort(
            v.as_mut_ptr().cast(),
            v.len(),
            size_of::<i32>(),
            Some(cmp_i32),
        )
    };
}

fn c_format(n: i32, word: &CStr, x: f64) -> String {
    let mut buf = [0u8; 64];
    let fmt = c"%d-%s-%.2f";
    // SAFETY: buf is writable for 64 bytes and we pass its length (snprintf never writes more, always NUL-terminates);
    // the format consumes int, const char*, double, which match the variadic arguments exactly.
    let written = unsafe {
        snprintf(
            buf.as_mut_ptr().cast(),
            buf.len(),
            fmt.as_ptr(),
            n,
            word.as_ptr(),
            x,
        )
    };
    assert!(
        written >= 0 && (written as usize) < buf.len(),
        "truncated or failed"
    );
    CStr::from_bytes_until_nul(&buf)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn now_micros() -> i64 {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    // SAFETY: tv is a valid repr(C) timeval to write into; the timezone argument may be NULL.
    let rc = unsafe { libc::gettimeofday(&mut tv, ptr::null_mut()) };
    assert_eq!(rc, 0);
    tv.tv_sec as i64 * 1_000_000 + tv.tv_usec as i64
}

fn open_missing() -> io::Error {
    let path = c"/definitely/not/here/ff_233";
    // SAFETY: valid C string path; flags are plain O_RDONLY.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY) };
    // errno must be read IMMEDIATELY, before any other call can overwrite it.
    let err = io::Error::last_os_error();
    assert_eq!(fd, -1);
    err
}

// ---------------------------------------------------------------- (2) C calls Rust

/// Opaque to C: only ever seen as `struct Counter*`. No `repr(C)` needed because C never looks inside.
pub struct Counter {
    items: Vec<i64>,
}

pub const FF_OK: c_int = 0;
pub const FF_NULL: c_int = -2;
pub const FF_CALLBACK_PANIC: c_int = -1;
pub const FF_BAD_ARG: c_int = -3;

pub type VisitCb = extern "C" fn(ctx: *mut c_void, value: i64) -> c_int;

#[unsafe(no_mangle)]
pub extern "C" fn ff_counter_new() -> *mut Counter {
    Box::into_raw(Box::new(Counter { items: Vec::new() }))
}

#[unsafe(no_mangle)]
pub extern "C" fn ff_counter_push(h: *mut Counter, v: i64) -> c_int {
    // SAFETY: caller contract: `h` is NULL or a live pointer from ff_counter_new, not used concurrently.
    match unsafe { h.as_mut() } {
        Some(c) => {
            c.items.push(v);
            FF_OK
        }
        None => FF_NULL,
    }
}

/// Calls `cb(ctx, item)` per item; stops at the first nonzero return and passes it back. `cb` is nullable.
#[unsafe(no_mangle)]
pub extern "C" fn ff_counter_visit(
    h: *const Counter,
    cb: Option<VisitCb>,
    ctx: *mut c_void,
) -> c_int {
    // SAFETY: caller contract as above (shared access only here).
    let Some(c) = (unsafe { h.as_ref() }) else {
        return FF_NULL;
    };
    let Some(cb) = cb else { return FF_NULL };
    for &v in &c.items {
        let rc = cb(ctx, v);
        if rc != 0 {
            return rc;
        }
    }
    FF_OK
}

/// Slice as (ptr, len) plus an out-parameter. Returns a status; writes `*out` only on success.
#[unsafe(no_mangle)]
pub extern "C" fn ff_sum_i32(ptr: *const i32, len: usize, out: *mut i64) -> c_int {
    if out.is_null() || (ptr.is_null() && len != 0) {
        return FF_NULL;
    }
    let s: &[i32] = if len == 0 {
        &[]
    } else {
        // SAFETY: non-null checked; caller guarantees `len` initialized, aligned i32s that stay valid for this call.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    // SAFETY: `out` is non-null and caller guarantees it is valid for a write of i64.
    unsafe { out.write(s.iter().map(|&x| x as i64).sum()) };
    FF_OK
}

/// Rust-allocated string handed to C. C must give it back to `ff_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn ff_greeting(name: *const c_char) -> *mut c_char {
    if name.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: caller guarantees a valid NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    CString::new(format!("hello, {name}!"))
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn ff_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: `s` came from CString::into_raw in ff_greeting and is freed exactly once (caller contract).
        drop(unsafe { CString::from_raw(s) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ff_counter_free(h: *mut Counter) {
    if !h.is_null() {
        // SAFETY: `h` came from Box::into_raw in ff_counter_new and is freed exactly once (caller contract).
        drop(unsafe { Box::from_raw(h) });
    }
}

/// `repr(C)` enum: layout matches a C enum, but ONLY these values are valid Rust values.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    Red = 0,
    Green = 1,
    Blue = 2,
}
impl TryFrom<c_int> for Color {
    type Error = c_int;
    fn try_from(v: c_int) -> Result<Self, c_int> {
        match v {
            0 => Ok(Color::Red),
            1 => Ok(Color::Green),
            2 => Ok(Color::Blue),
            other => Err(other),
        }
    }
}

/// Boundary function takes the RAW int (a `Color` parameter fed 7 from C would already be UB).
#[unsafe(no_mangle)]
pub extern "C" fn ff_color_name(raw: c_int) -> *const c_char {
    match Color::try_from(raw) {
        Ok(Color::Red) => c"red".as_ptr(),
        Ok(Color::Green) => c"green".as_ptr(),
        Ok(Color::Blue) => c"blue".as_ptr(),
        Err(_) => ptr::null(),
    }
}

/// What a C program holds after dlsym/header inclusion: nothing but function pointers.
struct CApi {
    new: extern "C" fn() -> *mut Counter,
    push: extern "C" fn(*mut Counter, i64) -> c_int,
    visit: extern "C" fn(*const Counter, Option<VisitCb>, *mut c_void) -> c_int,
    free: extern "C" fn(*mut Counter),
    sum: extern "C" fn(*const i32, usize, *mut i64) -> c_int,
    greeting: extern "C" fn(*const c_char) -> *mut c_char,
    string_free: extern "C" fn(*mut c_char),
    color_name: extern "C" fn(c_int) -> *const c_char,
}
const API: CApi = CApi {
    new: ff_counter_new,
    push: ff_counter_push,
    visit: ff_counter_visit,
    free: ff_counter_free,
    sum: ff_sum_i32,
    greeting: ff_greeting,
    string_free: ff_string_free,
    color_name: ff_color_name,
};

/// Callback implemented in Rust but with the C ABI. Panics are caught: unwinding out of `extern "C"` would abort.
extern "C" fn accumulate_cb(ctx: *mut c_void, v: i64) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the caller of visit passes `&mut i64` as ctx and keeps it alive for the whole visit call.
        let acc = unsafe { &mut *(ctx as *mut i64) };
        if v == 13 {
            panic!("unlucky number");
        }
        *acc += v;
        FF_OK
    }))
    .unwrap_or(FF_CALLBACK_PANIC)
}

/// Safe wrapper: owns the handle, exposes a safe API, frees in Drop. All unsafe is confined here.
struct SafeCounter(NonNull<Counter>);
impl SafeCounter {
    fn new() -> Self {
        NonNull::new((API.new)())
            .map(SafeCounter)
            .expect("allocation failed")
    }
    fn push(&mut self, v: i64) {
        assert_eq!((API.push)(self.0.as_ptr(), v), FF_OK);
    }
    fn sum_via_callback(&self) -> Result<i64, c_int> {
        let mut acc: i64 = 0;
        let rc = (API.visit)(
            self.0.as_ptr(),
            Some(accumulate_cb),
            (&mut acc as *mut i64).cast(),
        );
        if rc == FF_OK { Ok(acc) } else { Err(rc) }
    }
}
impl Drop for SafeCounter {
    fn drop(&mut self) {
        (API.free)(self.0.as_ptr());
    }
}

fn main() {
    // --- Rust -> C
    let cs = CString::new("flowforge").unwrap();
    assert_eq!(c_strlen(&cs), 9);
    assert!(
        CString::new("a\0b").is_err(),
        "interior NUL is rejected, not truncated"
    );
    assert!(c_getenv("FF_233_SURELY_UNSET").is_none());
    assert!(c_getenv("PATH").is_some());

    let mut v = [5, -3, 9, 0, 2, 2, -8, 7];
    c_qsort(&mut v);
    assert_eq!(v, [-8, -3, 0, 2, 2, 5, 7, 9]);
    assert!(COMPARISONS.load(Ordering::Relaxed) >= v.len() - 1);
    println!(
        "qsort ok after {} comparator calls: {v:?}",
        COMPARISONS.load(Ordering::Relaxed)
    );

    let s = c_format(42, c"answer", 3.14159);
    assert_eq!(s, "42-answer-3.14");
    println!("snprintf -> {s}");

    let (t1, t2) = (now_micros(), now_micros());
    assert!(t1 > 1_600_000_000_000_000 && t2 >= t1);

    let e = open_missing();
    assert_eq!(e.raw_os_error(), Some(libc::ENOENT));
    assert_eq!(e.kind(), io::ErrorKind::NotFound);
    println!("errno via last_os_error: {e}");

    // --- C -> Rust through function pointers
    let mut c = SafeCounter::new();
    for i in 1..=5 {
        c.push(i);
    }
    assert_eq!(c.sum_via_callback(), Ok(15));
    // null callback / null handle are reported, not crashes
    assert_eq!((API.visit)(c.0.as_ptr(), None, ptr::null_mut()), FF_NULL);
    assert_eq!((API.push)(ptr::null_mut(), 1), FF_NULL);

    // panic inside the callback: contained, reported as an error code, process survives
    c.push(13);
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {})); // keep the demo output quiet
    let r = c.sum_via_callback();
    panic::set_hook(old_hook);
    assert_eq!(r, Err(FF_CALLBACK_PANIC));
    println!("panic in callback contained -> code {}", r.unwrap_err());
    drop(c); // _free runs here

    // slices + out-params
    let data = [10i32, 20, 30, -5];
    let mut out = 0i64;
    assert_eq!((API.sum)(data.as_ptr(), data.len(), &mut out), FF_OK);
    assert_eq!(out, 55);
    assert_eq!((API.sum)(ptr::null(), 0, &mut out), FF_OK);
    assert_eq!(out, 0);
    assert_eq!((API.sum)(ptr::null(), 3, &mut out), FF_NULL);
    assert_eq!((API.sum)(data.as_ptr(), 1, ptr::null_mut()), FF_NULL);

    // ownership round trip: Rust allocates, C uses (strlen), Rust frees
    let g = (API.greeting)(c"world".as_ptr());
    assert!(!g.is_null());
    // SAFETY: g is a valid NUL-terminated string from ff_greeting, not yet freed.
    let (len, text) = unsafe { (strlen(g), CStr::from_ptr(g).to_str().unwrap().to_owned()) };
    assert_eq!((len, text.as_str()), (13, "hello, world!"));
    (API.string_free)(g); // never libc::free(g): different allocator contract
    (API.string_free)(ptr::null_mut()); // freeing NULL is a no-op
    assert!((API.greeting)(ptr::null()).is_null());

    // enum validation
    for (raw, want) in [(0, Some("red")), (2, Some("blue")), (7, None), (-1, None)] {
        let p = (API.color_name)(raw);
        let got = if p.is_null() {
            None
        } else {
            // SAFETY: non-null results are 'static C string literals.
            Some(unsafe { CStr::from_ptr(p) }.to_str().unwrap())
        };
        assert_eq!(got, want, "raw={raw}");
    }
    assert_eq!(Color::try_from(7), Err(7));
    println!("r233 ok: C interop, opaque handle, panic containment, enum validation");
}
