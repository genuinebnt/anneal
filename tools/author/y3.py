from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
# 1. C strings
CS_STARTER = """
use std::ffi::{c_char, CStr, CString, NulError};

extern "C" {
    fn strlen(s: *const c_char) -> usize;
}

/// The length in **bytes** of `s` as C's `strlen` sees it. Fails if `s` contains an interior NUL byte.
pub fn c_strlen(s: &str) -> Result<usize, NulError> {
    todo!()
}

/// Reads a C string owned by someone else. `None` for a null pointer; invalid UTF-8 is replaced with U+FFFD.
///
/// # Safety
/// `p` is null or points to a NUL-terminated string that stays valid for the call.
pub unsafe fn from_c(p: *const c_char) -> Option<String> {
    todo!()
}

/// Builds `"hello, {name}"` as a C string and hands ownership to the caller (who must give it back to `free_greeting`).
pub fn make_greeting(name: &str) -> Result<*mut c_char, NulError> {
    todo!()
}

/// Takes back a pointer from `make_greeting` and frees it. Null is allowed and does nothing.
///
/// # Safety
/// `p` is null or came from `make_greeting` and has not been freed.
pub unsafe fn free_greeting(p: *mut c_char) {
    todo!()
}

/// The length of the C string at the start of `buf` (up to its first NUL), or `None` if `buf` has no NUL.
pub fn c_prefix_len(buf: &[u8]) -> Option<usize> {
    todo!()
}
"""

CS = """
use std::ffi::{c_char, CStr, CString, NulError};

extern "C" {
    fn strlen(s: *const c_char) -> usize;
}

pub fn c_strlen(s: &str) -> Result<usize, NulError> {
    let c = CString::new(s)?; // adds the terminator, refuses an interior NUL
    Ok(unsafe { strlen(c.as_ptr()) })
}

pub unsafe fn from_c(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    Some(CStr::from_ptr(p).to_string_lossy().into_owned())
}

pub fn make_greeting(name: &str) -> Result<*mut c_char, NulError> {
    Ok(CString::new(format!("hello, {name}"))?.into_raw())
}

pub unsafe fn free_greeting(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p)); // the allocator that made it is the one that frees it
    }
}

pub fn c_prefix_len(buf: &[u8]) -> Option<usize> {
    CStr::from_bytes_until_nul(buf).ok().map(|c| c.to_bytes().len())
}
"""

P.append(dict(
    slug="cstring-boundary", title="C strings across the boundary: who owns what", level="easy", stage="use",
    tags=["FFI", "CString", "CStr", "NUL terminator", "into_raw / from_raw", "ownership", "extern \"C\""],
    teaches=["A C string is a pointer to bytes ending in NUL. Rust's `&str` has a length and may contain NULs, so `CString::new` can fail, and `CStr` is the borrowed view of someone else's string.",
             "`CString::into_raw` gives ownership to C; it must come back through `CString::from_raw`, never `free`: memory goes back to the allocator that made it.",
             "`strlen` counts bytes, not characters, and stops at the first NUL: the length of `\"h\\u{e9}llo\"` is 6."],
    statement="""
        Write the thin safe layer you need around a C library's string functions. Rust links against the C library already, so
        `extern "C" { fn strlen(...) }` just works.

        - `c_strlen`: build a `CString`, call C's `strlen` on it. An interior NUL byte in the Rust string is an error (`NulError`).
        - `from_c`: read a C string that someone else owns into a `String`; a null pointer is `None`; invalid UTF-8 becomes
          U+FFFD instead of failing.
        - `make_greeting` / `free_greeting`: produce `"hello, {name}"` for C to hold on to, and take it back. The pointer must be freed
          by Rust's `CString::from_raw`, not by `free`.
        - `c_prefix_len`: the length of the C string at the start of a byte buffer, or `None` if there is no NUL terminator.
    """,
    starter=CS_STARTER,
    solution=CS,
    visible=[
        T("strlen_counts_bytes", "e-acute is two bytes", "(c_strlen(\"hello\"), c_strlen(\"h\\u{e9}llo\"), c_strlen(\"\"))", "(Ok(5), Ok(6), Ok(0))"),
        T("interior_nul_is_refused", "NulError carries the position", "c_strlen(\"a\\0b\").map_err(|e| e.nul_position())", "Err(1)"),
        T("null_pointer_is_none", "no string at all", "unsafe { from_c(std::ptr::null()) }", "None"),
        T("read_a_borrowed_c_string", "bytes with a terminator", "unsafe { from_c(b\"caf\\xc3\\xa9\\0\".as_ptr() as *const std::ffi::c_char) }", "Some(\"caf\\u{e9}\".to_string())"),
        T("greeting_round_trip", "make, read, free", "{ let p = make_greeting(\"Ferris\").unwrap(); let s = unsafe { from_c(p) }; unsafe { free_greeting(p) }; s }", "Some(\"hello, Ferris\".to_string())"),
        T("prefix_of_a_buffer", "stops at the first NUL", "(c_prefix_len(b\"abc\\0def\"), c_prefix_len(b\"abc\"), c_prefix_len(b\"\\0\"), c_prefix_len(b\"\"))", "(Some(3), None, Some(0), None)"),
    ],
    hidden=[
        T("invalid_utf8_is_replaced", "0xFF is not UTF-8", "unsafe { from_c(b\"a\\xffb\\0\".as_ptr() as *const std::ffi::c_char) }", "Some(\"a\\u{fffd}b\".to_string())"),
        T("greeting_rejects_a_nul_in_the_name", "cannot be a C string", "make_greeting(\"a\\0b\").is_err()", "true"),
        T("free_null_is_a_no_op", "must not crash", "{ unsafe { free_greeting(std::ptr::null_mut()) }; true }", "true"),
        T("greetings_are_independent", "two live at once", "{ let a = make_greeting(\"A\").unwrap(); let b = make_greeting(\"BB\").unwrap(); let r = unsafe { (from_c(a), from_c(b)) }; unsafe { free_greeting(a); free_greeting(b); } r }", "(Some(\"hello, A\".to_string()), Some(\"hello, BB\".to_string()))"),
        T("strlen_long_string", "a megabyte", "c_strlen(&\"x\".repeat(1_000_000))", "Ok(1_000_000)"),
        T("strlen_stops_nowhere_early", "all multi-byte", "c_strlen(\"\\u{1F980}\\u{1F980}\")", "Ok(8)"),
        T("prefix_ignores_what_follows_the_first_nul", "two strings in one buffer", "(c_prefix_len(b\"ab\\0cd\\0\"), c_prefix_len(b\"\\0\\0\"))", "(Some(2), Some(0))"),
        T("prefix_of_a_buffer_without_terminator_but_with_nul_later_is_not_found", "no NUL at all", "c_prefix_len(&[1u8, 2, 3, 4])", "None"),
        T("greeting_with_unicode", "UTF-8 passes through", "{ let p = make_greeting(\"\\u{e9}\").unwrap(); let s = unsafe { from_c(p) }; unsafe { free_greeting(p) }; s }", "Some(\"hello, \\u{e9}\".to_string())"),
    ],
    wrong=dict(
        strlen_counts_chars=sub(CS, "    let c = CString::new(s)?; // adds the terminator, refuses an interior NUL\n    Ok(unsafe { strlen(c.as_ptr()) })", "    let c = CString::new(s)?;\n    let _ = unsafe { strlen(c.as_ptr()) };\n    Ok(s.chars().count())"),
        strict_utf8=sub(CS, "Some(CStr::from_ptr(p).to_string_lossy().into_owned())", "Some(CStr::from_ptr(p).to_str().unwrap().to_string())"),
        no_terminator_means_whole_buffer=sub(CS, "CStr::from_bytes_until_nul(buf).ok().map(|c| c.to_bytes().len())", "Some(buf.iter().position(|&b| b == 0).unwrap_or(buf.len()))"),
        greeting_checks_nothing_about_nul=sub(CS, "    Ok(CString::new(format!(\"hello, {name}\"))?.into_raw())", "    Ok(CString::new(format!(\"hello, {}\", name.replace('\\0', \"\"))).unwrap().into_raw())"),
        prefix_is_the_buffer_length=sub(CS, "CStr::from_bytes_until_nul(buf).ok().map(|c| c.to_bytes().len())", "CStr::from_bytes_until_nul(buf).ok().map(|_| buf.len())"),
    ),
    hints=[("approach", "`CString::new(s)` allocates the NUL-terminated copy and fails on an interior NUL. Its `.as_ptr()` is what C sees; the `CString` must outlive the call. For borrowed strings the other way, `CStr::from_ptr(p)` wraps the pointer without copying, and `to_string_lossy` makes an owned `String`."),
           ("rust", "`into_raw` leaks the allocation on purpose and returns `*mut c_char`; `CString::from_raw` is its only correct partner. `CStr::from_bytes_until_nul(buf)` finds the terminator in a byte slice and returns an error if there is none."),
           ("edge", "Never call C's `free` on a pointer from `CString::into_raw`: Rust may use a different allocator than C's `malloc`. Never keep `as_ptr()` of a temporary `CString` (`CString::new(s).unwrap().as_ptr()` dangles the moment the statement ends).")],
    notes=("The `CString`/`CStr` pair is the whole FFI story for text: owned vs borrowed, like `String`/`&str`. Every `bindgen`-generated wrapper and every `pyo3`/`napi` string conversion is built on these. The classic bug (the dangling `as_ptr()` of a temporary) compiles cleanly and works until the allocator reuses the memory.", "O(n) for strlen and copies", "O(n)"),
    follow_up="1) Write `fn with_c_str<R>(s: &str, f: impl FnOnce(*const c_char) -> R) -> Result<R, NulError>` and explain why a closure API is safer than returning the pointer. 2) Call `getenv` from libc and return `Option<String>`: who owns the result? 3) Write a function taking `*const *const c_char` (an `argv`-style array ending in null) and returning a `Vec<String>`. 4) What changes for Windows wide strings (`*const u16`)? 5) Fuzz `from_c` with random byte buffers that always end in NUL.",
    related=["Y2", "S2", "Y3"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. repr(C)
RC_TYPES = """
use std::mem::{align_of, offset_of, size_of};

/// The C struct `struct header { uint8_t tag; uint32_t len; uint16_t flags; };`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub tag: u8,
    pub len: u32,
    pub flags: u16,
}
"""

RC_STARTER = RC_TYPES + """
/// `(size, alignment, [offset of tag, offset of len, offset of flags])` of `Header` under the C layout rules.
pub fn layout() -> (usize, usize, [usize; 3]) {
    todo!()
}

/// The 12 bytes C would have in memory for `h` (native endianness). Padding bytes are zero, never uninitialised.
pub fn to_bytes(h: &Header) -> [u8; 12] {
    todo!()
}

/// Reads a `Header` from bytes in the C layout. `None` if fewer than 12 bytes are given. Works for any alignment.
pub fn from_bytes(b: &[u8]) -> Option<Header> {
    todo!()
}

/// Reinterprets the bytes in place as a `&Header` without copying. `None` unless there are at least 12 bytes
/// **and** the address is correctly aligned for `Header`.
pub fn view(b: &[u8]) -> Option<&Header> {
    todo!()
}
"""

RC = RC_TYPES + """
pub fn layout() -> (usize, usize, [usize; 3]) {
    (size_of::<Header>(), align_of::<Header>(), [offset_of!(Header, tag), offset_of!(Header, len), offset_of!(Header, flags)])
}

pub fn to_bytes(h: &Header) -> [u8; 12] {
    let mut out = [0u8; 12]; // zeroed first: the padding bytes stay 0
    out[offset_of!(Header, tag)] = h.tag;
    let l = offset_of!(Header, len);
    out[l..l + 4].copy_from_slice(&h.len.to_ne_bytes());
    let f = offset_of!(Header, flags);
    out[f..f + 2].copy_from_slice(&h.flags.to_ne_bytes());
    out
}

pub fn from_bytes(b: &[u8]) -> Option<Header> {
    if b.len() < size_of::<Header>() {
        return None;
    }
    // every bit pattern is a valid u8/u32/u16, so a byte-wise read is sound whatever the alignment
    Some(unsafe { std::ptr::read_unaligned(b.as_ptr() as *const Header) })
}

pub fn view(b: &[u8]) -> Option<&Header> {
    if b.len() < size_of::<Header>() || (b.as_ptr() as usize) % align_of::<Header>() != 0 {
        return None;
    }
    Some(unsafe { &*(b.as_ptr() as *const Header) })
}
"""

RC_HELPERS = """
        #[repr(align(4))]
        struct Aligned([u8; 20]);

        fn bytes(tag: u8, len: u32, flags: u16) -> [u8; 12] {
            let mut b = [0u8; 12];
            b[0] = tag;
            b[4..8].copy_from_slice(&len.to_ne_bytes());
            b[8..10].copy_from_slice(&flags.to_ne_bytes());
            b
        }
"""

P.append(dict(
    slug="repr-c-header", title="A #[repr(C)] header: layout, bytes and aliasing", level="medium", stage="use",
    tags=["FFI", "repr(C)", "layout", "padding", "alignment", "offset_of", "read_unaligned", "zero-copy"],
    teaches=["`#[repr(C)]` fixes field order and padding to the C rules: `{u8, u32, u16}` is 12 bytes with align 4, not the 7 you get by adding sizes. Plain Rust structs may be reordered; only `repr(C)` is a stable ABI.",
             "Padding bytes are not part of the value: building the byte image from a zeroed buffer (not by transmuting the struct) avoids leaking uninitialised memory.",
             "Reading a struct from a byte slice is two different operations: `read_unaligned` copies from any address; a `&Header` pointing into the slice is only legal when the address is aligned."],
    statement="""
        A C library gives you a struct and you must exchange it as raw bytes. Given
        `struct header { uint8_t tag; uint32_t len; uint16_t flags; }` as the `#[repr(C)] Header` below:

        - `layout()` returns `(size, align, offsets)` as the compiler computes them (use `size_of`, `align_of`, `offset_of!`).
        - `to_bytes` writes the 12 bytes of its C image in native byte order, with the padding bytes **zero**.
        - `from_bytes` reads a `Header` back from at least 12 bytes at *any* address (a copy).
        - `view` returns a `&Header` pointing straight into the slice (no copy) but only if it is long enough **and** the slice's
          address is a multiple of `Header`'s alignment; otherwise `None`.
    """,
    starter=RC_STARTER,
    solution=RC,
    visible=[
        T("layout_follows_c_rules", "12 bytes, align 4, padded", "layout()", "(12, 4, [0, 4, 8])"),
        T("to_bytes_has_zero_padding", "tag, 3 pad, len, flags, 2 pad", "to_bytes(&Header { tag: 7, len: 0x01020304, flags: 0x0506 }) == { let mut b = [0u8; 12]; b[0] = 7; b[4..8].copy_from_slice(&0x01020304u32.to_ne_bytes()); b[8..10].copy_from_slice(&0x0506u16.to_ne_bytes()); b }", "true"),
        T("from_bytes_round_trip", "write then read", "{ let h = Header { tag: 255, len: u32::MAX, flags: 1 }; from_bytes(&to_bytes(&h)) }", "Some(Header { tag: 255, len: u32::MAX, flags: 1 })"),
        T("short_input_is_none", "11 bytes", "(from_bytes(&[0u8; 11]), view(&[0u8; 11]).is_none())", "(None, true)"),
        T("from_bytes_ignores_padding_contents", "garbage in the padding", "{ let mut b = to_bytes(&Header { tag: 1, len: 2, flags: 3 }); b[1] = 0xAA; b[11] = 0xBB; from_bytes(&b) }", "Some(Header { tag: 1, len: 2, flags: 3 })"),
    ],
    hidden=[
        RC_HELPERS + """
        #[test]
        fn view_needs_alignment() {
            let buf = Aligned([0u8; 20]);
            check!("aligned address", view(&buf.0[..12]).is_some(), true);
            check!("offset by one", view(&buf.0[1..13]).is_none(), true);
            check!("offset by two", view(&buf.0[2..14]).is_none(), true);
            check!("offset by four is aligned again", view(&buf.0[4..16]).is_some(), true);
        }

        #[test]
        fn view_reads_in_place() {
            let mut buf = Aligned([0u8; 20]);
            buf.0[..12].copy_from_slice(&bytes(9, 1000, 77));
            let h = view(&buf.0[..12]).unwrap();
            check!("fields", (h.tag, h.len, h.flags), (9, 1000, 77));
            check!("it is the same memory, not a copy", h as *const Header as usize, buf.0.as_ptr() as usize);
        }

        #[test]
        fn from_bytes_works_at_every_alignment() {
            let buf = Aligned([0u8; 20]);
            let mut buf = buf;
            buf.0[1..13].copy_from_slice(&bytes(5, 123456, 321));
            check!("misaligned source", from_bytes(&buf.0[1..13]), Some(Header { tag: 5, len: 123456, flags: 321 }));
            buf.0[3..15].copy_from_slice(&bytes(6, 7, 8));
            check!("offset 3", from_bytes(&buf.0[3..15]), Some(Header { tag: 6, len: 7, flags: 8 }));
        }

        #[test]
        fn extra_trailing_bytes_are_ignored() {
            let mut v = bytes(1, 2, 3).to_vec();
            v.extend_from_slice(&[9, 9, 9, 9]);
            check!("16 bytes given", from_bytes(&v), Some(Header { tag: 1, len: 2, flags: 3 }));
            check!("exactly 12 is enough", from_bytes(&v[..12]).is_some(), true);
            check!("11 is not", from_bytes(&v[..11]), None);
        }

        #[test]
        fn to_bytes_is_deterministic_and_matches_the_layout() {
            let h = Header { tag: 0xFF, len: 0, flags: 0xFFFF };
            let b = to_bytes(&h);
            check!("tag at offset 0", b[0], 0xFF);
            check!("padding after the tag", &b[1..4], &[0u8, 0, 0][..]);
            check!("flags at offset 8", &b[8..10], &0xFFFFu16.to_ne_bytes()[..]);
            check!("padding at the end", &b[10..12], &[0u8, 0][..]);
            check!("same input, same bytes", to_bytes(&h), b);
        }
        """,
        T("layout_matches_size_of", "the numbers are the compiler's", "(layout().0 == std::mem::size_of::<Header>(), layout().1 == std::mem::align_of::<Header>())", "(true, true)"),
        T("size_is_a_multiple_of_alignment", "arrays of Header stay aligned", "layout().0 % layout().1", "0"),
        T("sum_of_fields_is_not_the_size", "1 + 4 + 2 = 7 is wrong", "layout().0 > 1 + 4 + 2", "true"),
    ],
    wrong=dict(
        layout_is_the_sum_of_fields=sub(RC, "    (size_of::<Header>(), align_of::<Header>(), [offset_of!(Header, tag), offset_of!(Header, len), offset_of!(Header, flags)])", "    (7, 1, [0, 1, 5])"),
        view_ignores_alignment=sub(RC, "    if b.len() < size_of::<Header>() || (b.as_ptr() as usize) % align_of::<Header>() != 0 {", "    if b.len() < size_of::<Header>() {"),
        view_copies_instead_of_aliasing=sub(RC, "    Some(unsafe { &*(b.as_ptr() as *const Header) })", "    Some(Box::leak(Box::new(from_bytes(b)?)))"),
        length_check_off_by_one=sub(RC, "    if b.len() < size_of::<Header>() {\n        return None;\n    }\n    // every", "    if b.len() < size_of::<Header>() - 1 {\n        return None;\n    }\n    // every"),
        padding_is_not_zeroed=sub(RC, "    let mut out = [0u8; 12]; // zeroed first: the padding bytes stay 0", "    let mut out = [0xA5u8; 12];"),
        flags_in_the_wrong_place=sub(RC, "    let f = offset_of!(Header, flags);\n    out[f..f + 2]", "    let f = 6;\n    out[f..f + 2]"),
    ),
    hints=[("approach", "The layout rules: each field sits at the next multiple of its own alignment, and the struct's size is rounded up to a multiple of its largest field alignment. `offset_of!(Header, len)` and `size_of` give you the compiler's answer, so you never hard-code a number."),
           ("rust", "`to_bytes`: start from `[0u8; 12]` and copy each field's `to_ne_bytes()` at its `offset_of!`. `from_bytes`: `std::ptr::read_unaligned(b.as_ptr() as *const Header)`. `view`: check `(b.as_ptr() as usize) % align_of::<Header>() == 0` before `&*(ptr as *const Header)`."),
           ("edge", "`&*(ptr as *const Header)` on a misaligned address is **undefined behaviour** even on hardware that tolerates unaligned loads: the compiler may assume alignment and miscompile. For enums, `bool` or references inside the struct you would also need to validate the bit pattern; here every pattern is valid.")],
    notes=("This is the core of zero-copy parsing (`zerocopy`, `bytemuck`) and of every C ABI binding. `repr(C)` is what makes a struct a stable interface; the padding rule is why serialising a struct with `transmute` can leak secrets (uninitialised padding), the classic information-disclosure bug in C network code.", "O(1)", "O(1)"),
    follow_up="1) Add `#[repr(C, packed)]` and see why taking `&h.len` is an error (an unaligned reference). 2) Add a `Header2` with a `u64` field and compute its layout by hand before asking the compiler. 3) Write `view_slice(b: &[u8]) -> Option<&[Header]>` for an array of headers and decide what to do with trailing bytes. 4) Make the byte order explicit (big-endian, as on the wire) with `u32::from_be_bytes` and explain why `repr(C)` alone is not a wire format. 5) Add a `#[repr(C)] enum` field and show why reading arbitrary bytes into it is unsound.",
    related=["F2", "Y2", "Y3"],
))

# ---------------------------------------------------------------------------------------------------------------
# 3. qsort_r with a closure
QS_STARTER = """
use std::cmp::Ordering;
use std::ffi::{c_int, c_void};

extern "C" {
    /// glibc's re-entrant `qsort`: sorts `nmemb` elements of `size` bytes, calling `compar(a, b, arg)`.
    fn qsort_r(base: *mut c_void, nmemb: usize, size: usize, compar: extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int, arg: *mut c_void);
}

/// Sorts `v` with C's `qsort_r`, ordering elements with the closure `cmp`.
///
/// If `cmp` panics, the panic must not unwind through C (that is undefined behaviour): catch it, stop calling `cmp`
/// for the rest of the sort, let `qsort_r` finish, and then re-raise the same panic to the caller.
pub fn sort_by_c<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], cmp: F) {
    todo!()
}
"""

QS = """
use std::any::Any;
use std::cmp::Ordering;
use std::ffi::{c_int, c_void};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

extern "C" {
    fn qsort_r(base: *mut c_void, nmemb: usize, size: usize, compar: extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int, arg: *mut c_void);
}

struct Ctx<'a, T> {
    cmp: &'a mut dyn FnMut(&T, &T) -> Ordering,
    panic: Option<Box<dyn Any + Send>>,
}

/// The `extern "C"` function C calls back. It must never unwind: a panic is caught and parked in the context.
extern "C" fn trampoline<T>(a: *const c_void, b: *const c_void, arg: *mut c_void) -> c_int {
    let ctx = unsafe { &mut *(arg as *mut Ctx<T>) };
    if ctx.panic.is_some() {
        return 0; // already failed: let C finish quickly without running user code again
    }
    let (a, b) = unsafe { (&*(a as *const T), &*(b as *const T)) };
    match catch_unwind(AssertUnwindSafe(|| (ctx.cmp)(a, b))) {
        Ok(order) => order as c_int, // Less = -1, Equal = 0, Greater = 1
        Err(payload) => {
            ctx.panic = Some(payload);
            0
        }
    }
}

pub fn sort_by_c<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut cmp: F) {
    let mut ctx = Ctx::<T> { cmp: &mut cmp, panic: None };
    unsafe {
        qsort_r(v.as_mut_ptr() as *mut c_void, v.len(), std::mem::size_of::<T>(), trampoline::<T>, &mut ctx as *mut Ctx<T> as *mut c_void);
    }
    if let Some(payload) = ctx.panic {
        resume_unwind(payload); // back on the Rust side of the boundary
    }
}
"""

P.append(dict(
    slug="qsort-callback", title="Sort with C's qsort_r and a Rust closure", level="medium", stage="understand",
    tags=["FFI", "callbacks", "extern \"C\" fn", "trampoline", "context pointer", "catch_unwind", "panic safety", "qsort_r"],
    teaches=["C callbacks cannot capture: the usual trick is a `void *context` argument, and a generic `extern \"C\"` trampoline that casts it back to your closure.",
             "A Rust panic must never unwind through C frames. Catch it in the trampoline, remember it, and re-raise with `resume_unwind` once control is back in Rust.",
             "`qsort_r` moves elements as raw bytes, so `T: Copy` and `size_of::<T>()` are what make it sound."],
    statement="""
        Use libc's `qsort_r` (declared for you) to implement `sort_by_c(v, cmp)`: sort any slice of `Copy` values with a Rust
        closure as the comparison, like `slice::sort_by` but powered by C.

        `qsort_r` takes a function pointer `extern "C" fn(*const c_void, *const c_void, *mut c_void) -> c_int` and a `void *`
        context. Write a generic trampoline that recovers the closure from the context, calls it on the two elements and returns
        `-1 / 0 / 1`.

        If the closure **panics**, the panic must not cross the C boundary. Catch it, do not call the closure again for the rest of
        the sort, let `qsort_r` return, and then re-raise the same panic payload in the caller.
    """,
    starter=QS_STARTER,
    solution=QS,
    visible=[
        T("sorts_integers", "ascending", "{ let mut v = vec![5, 1, 4, 2, 3]; sort_by_c(&mut v, |a, b| a.cmp(b)); v }", "vec![1, 2, 3, 4, 5]"),
        T("sorts_descending_with_a_closure", "reverse order", "{ let mut v = vec![5, 1, 4, 2, 3]; sort_by_c(&mut v, |a, b| b.cmp(a)); v }", "vec![5, 4, 3, 2, 1]"),
        T("sorts_by_a_key", "tuples by the second field", "{ let mut v = vec![(1, 'c'), (2, 'a'), (3, 'b')]; sort_by_c(&mut v, |a, b| a.1.cmp(&b.1)); v }", "vec![(2, 'a'), (3, 'b'), (1, 'c')]"),
        T("empty_and_single", "nothing to do", "{ let mut e: Vec<i32> = vec![]; let mut one = vec![9]; sort_by_c(&mut e, |a, b| a.cmp(b)); sort_by_c(&mut one, |a, b| a.cmp(b)); (e, one) }", "(vec![], vec![9])"),
        T("the_closure_may_capture_and_mutate", "counts its own calls", "{ let mut calls = 0; let mut v = vec![3, 2, 1]; sort_by_c(&mut v, |a, b| { calls += 1; a.cmp(b) }); (v, calls > 0) }", "(vec![1, 2, 3], true)"),
    ],
    hidden=[
        T("strings_and_floats", "non-integer Copy types", "{ let mut s = vec![\"pear\", \"apple\", \"fig\"]; sort_by_c(&mut s, |a, b| a.cmp(b)); let mut f = vec![2.5f64, -1.0, 0.0]; sort_by_c(&mut f, |a, b| a.partial_cmp(b).unwrap()); (s, f) }", "(vec![\"apple\", \"fig\", \"pear\"], vec![-1.0, 0.0, 2.5])"),
        T("large_structs_use_the_right_element_size", "a 40-byte element", "{ #[derive(Clone, Copy, Debug, PartialEq)] struct Big { key: u32, pad: [u64; 4] } let mut v: Vec<Big> = (0..50u32).rev().map(|k| Big { key: k, pad: [k as u64; 4] }).collect(); sort_by_c(&mut v, |a, b| a.key.cmp(&b.key)); v.iter().enumerate().all(|(i, b)| b.key == i as u32 && b.pad == [i as u64; 4]) }", "true"),
        T("already_sorted_and_reversed", "common inputs", "{ let mut a: Vec<u32> = (0..500).collect(); let mut b: Vec<u32> = (0..500).rev().collect(); sort_by_c(&mut a, |x, y| x.cmp(y)); sort_by_c(&mut b, |x, y| x.cmp(y)); (a == (0..500).collect::<Vec<u32>>(), b == (0..500).collect::<Vec<u32>>()) }", "(true, true)"),
        T("all_equal", "no order to impose", "{ let mut v = vec![7u8; 100]; sort_by_c(&mut v, |a, b| a.cmp(b)); v == vec![7u8; 100] }", "true"),
        T("zero_sized_elements", "size 0", "{ let mut v = vec![(); 10]; sort_by_c(&mut v, |_, _| std::cmp::Ordering::Equal); v.len() }", "10"),
        """
        #[test]
        fn matches_the_standard_sort_on_random_data() {
            let mut rng = anneal_prelude::Rng::new(31337);
            for round in 0..200 {
                let n = rng.below(300);
                let v: Vec<u32> = (0..n).map(|_| rng.below(1000) as u32).collect();
                let mut mine = v.clone();
                let mut std_sorted = v;
                sort_by_c(&mut mine, |a, b| a.cmp(b));
                std_sorted.sort();
                check!(format!("round {round}"), mine, std_sorted);
            }
        }

        #[test]
        fn a_panic_reaches_the_caller_with_its_payload() {
            let mut v: Vec<i32> = (0..50).rev().collect();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sort_by_c(&mut v, |a, b| {
                    if *a == 25 || *b == 25 {
                        panic!("boom at 25");
                    }
                    a.cmp(b)
                });
            }));
            let err = r.expect_err("the panic must propagate out of sort_by_c");
            let msg = err.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| err.downcast_ref::<String>().cloned());
            check!("same payload", msg, Some("boom at 25".to_string()));
        }

        #[test]
        fn the_closure_is_not_called_again_after_it_panicked() {
            use std::cell::Cell;
            let calls = Cell::new(0usize);
            let mut v: Vec<i32> = (0..200).rev().collect();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sort_by_c(&mut v, |a, b| {
                    calls.set(calls.get() + 1);
                    if calls.get() == 5 {
                        panic!("fifth call");
                    }
                    a.cmp(b)
                });
            }));
            check!("it panicked", r.is_err(), true);
            check!("exactly five calls: nothing runs after the panic", calls.get(), 5);
        }

        #[test]
        fn the_slice_stays_a_permutation_after_a_panic() {
            let mut v: Vec<i32> = (0..100).map(|i| (i * 37) % 101).collect();
            let mut before = v.clone();
            let mut n = 0;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sort_by_c(&mut v, |a, b| {
                    n += 1;
                    if n == 40 {
                        panic!("stop");
                    }
                    a.cmp(b)
                });
            }));
            before.sort();
            let mut after = v.clone();
            after.sort();
            check!("nothing lost or duplicated", after, before);
        }
        """,
    ],
    wrong=dict(
        sorts_descending=sub(QS, "        Ok(order) => order as c_int, // Less = -1, Equal = 0, Greater = 1", "        Ok(order) => order.reverse() as c_int,"),
        swallows_the_panic=sub(QS, "    if let Some(payload) = ctx.panic {\n        resume_unwind(payload); // back on the Rust side of the boundary\n    }\n", ""),
        keeps_calling_user_code_after_a_panic=sub(QS, "    if ctx.panic.is_some() {\n        return 0; // already failed: let C finish quickly without running user code again\n    }\n", ""),
        panics_with_a_different_payload=sub(QS, "        resume_unwind(payload); // back on the Rust side of the boundary", "        let _ = payload;\n        panic!(\"comparison panicked\");"),
    ),
    hints=[("approach", "Put the closure and a `panic: Option<Box<dyn Any + Send>>` slot in a context struct on the stack; pass its address as the `void *` argument. The trampoline is a generic `extern \"C\" fn trampoline<T>(a, b, arg)` that casts `arg` back to `&mut Ctx<T>`, casts `a` and `b` to `&T`, calls the closure inside `catch_unwind`, and maps `Ordering` to `c_int` (`order as c_int` is -1/0/1)."),
           ("rust", "`catch_unwind(AssertUnwindSafe(|| (ctx.cmp)(a, b)))`. Store `Err(payload)` in `ctx.panic` and return 0; at the top of the trampoline, if `ctx.panic.is_some()` return 0 immediately. After `qsort_r` returns, `resume_unwind(payload)` re-raises it with the original payload (a plain `panic!` would replace it). Pass `trampoline::<T>` as the function pointer and `std::mem::size_of::<T>()` as the element size."),
           ("edge", "Since Rust 1.81 a panic that escapes an `extern \"C\" fn` aborts the process; before that it was undefined behaviour. Either way you must catch it. After a panic the comparison function is inconsistent, so `qsort_r` may produce any order, but it still only moves whole elements, so the slice stays a permutation: that is what `T: Copy` buys.")],
    notes=("This is the standard way to expose a Rust closure to a C API that has a context parameter (`pthread_create`, `sqlite3_create_function`, `curl` callbacks, GLib). The panic-parking trick is what `pyo3`, `napi-rs` and `cxx` do at every boundary. Without a context argument (plain `qsort`) there is no safe way to pass a closure: you would need a global or thread-local, which is exactly why `qsort_r` exists.", "O(n log n) comparisons", "O(1) extra"),
    follow_up="1) Make a version for non-`Copy` types and explain why moving elements with `memcpy` while Rust still owns them is unsound if a comparison panics (double drop). 2) Use `bsearch`-style callback APIs the same way: write `binary_search_c`. 3) Replace the trampoline's `catch_unwind` with a `Drop` guard that aborts, and compare the failure behaviour. 4) Write the same wrapper for `pthread_create` and discuss what the lifetime of the closure must be. 5) Benchmark against `slice::sort_by`: why is the C version usually slower despite being 'C'?",
    related=["Y2", "C5", "Y3"],
))

# ---------------------------------------------------------------------------------------------------------------
# 4. Opaque handle API
OH_TYPES = """
use std::cell::Cell;
use std::ffi::{c_char, CStr};

/// Status codes returned to C callers.
pub const OK: i32 = 0;
pub const ERR_NULL: i32 = -1;
pub const ERR_OVERFLOW: i32 = -2;

thread_local! {
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

/// How many `Counter`s have been dropped on this thread (lets tests check that every handle is freed exactly once).
pub fn drops() -> usize {
    DROPS.with(|d| d.get())
}

/// The Rust object behind the handle. C never sees its fields, only a pointer to it.
pub struct Counter {
    name: String,
    value: i64,
}

impl Drop for Counter {
    fn drop(&mut self) {
        DROPS.with(|d| d.set(d.get() + 1));
    }
}
"""

OH_STARTER = OH_TYPES + """
/// Creates a counter named `name` (a C string), starting at 0. A null `name` means the default name "counter".
/// Returns null if the name is not valid UTF-8. The caller owns the result and must pass it to `counter_free`.
pub extern "C" fn counter_new(name: *const c_char) -> *mut Counter {
    todo!()
}

/// Adds `delta`. Returns `OK`, `ERR_NULL` for a null handle, or `ERR_OVERFLOW` (leaving the value unchanged) if the sum
/// does not fit in an `i64`.
///
/// # Safety
/// `c` is null or a live handle from `counter_new`.
pub unsafe extern "C" fn counter_add(c: *mut Counter, delta: i64) -> i32 {
    todo!()
}

/// Writes the current value to `*out`. Returns `OK`, or `ERR_NULL` if `c` or `out` is null.
///
/// # Safety
/// `c` is null or a live handle; `out` is null or valid for a write.
pub unsafe extern "C" fn counter_get(c: *const Counter, out: *mut i64) -> i32 {
    todo!()
}

/// Copies the name into the caller's buffer `buf` of capacity `cap`, `snprintf`-style: at most `cap - 1` bytes plus a NUL
/// (nothing at all if `cap` is 0 or `buf` is null). Returns the **full** length of the name in bytes (so the caller can detect
/// truncation), or `ERR_NULL as isize` for a null handle.
///
/// # Safety
/// `c` is null or a live handle; `buf` is null or valid for `cap` bytes.
pub unsafe extern "C" fn counter_name(c: *const Counter, buf: *mut c_char, cap: usize) -> isize {
    todo!()
}

/// Destroys the counter. Null is allowed and does nothing.
///
/// # Safety
/// `c` is null or a handle from `counter_new` that has not been freed yet.
pub unsafe extern "C" fn counter_free(c: *mut Counter) {
    todo!()
}
"""

OH = OH_TYPES + """
pub extern "C" fn counter_new(name: *const c_char) -> *mut Counter {
    let name = if name.is_null() {
        "counter".to_string()
    } else {
        match unsafe { CStr::from_ptr(name) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return std::ptr::null_mut(),
        }
    };
    Box::into_raw(Box::new(Counter { name, value: 0 })) // ownership moves to the caller
}

pub unsafe extern "C" fn counter_add(c: *mut Counter, delta: i64) -> i32 {
    let Some(c) = c.as_mut() else {
        return ERR_NULL;
    };
    match c.value.checked_add(delta) {
        Some(v) => {
            c.value = v;
            OK
        }
        None => ERR_OVERFLOW,
    }
}

pub unsafe extern "C" fn counter_get(c: *const Counter, out: *mut i64) -> i32 {
    if c.is_null() || out.is_null() {
        return ERR_NULL;
    }
    *out = (*c).value;
    OK
}

pub unsafe extern "C" fn counter_name(c: *const Counter, buf: *mut c_char, cap: usize) -> isize {
    let Some(c) = c.as_ref() else {
        return ERR_NULL as isize;
    };
    let bytes = c.name.as_bytes();
    if !buf.is_null() && cap > 0 {
        let n = bytes.len().min(cap - 1);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    bytes.len() as isize
}

pub unsafe extern "C" fn counter_free(c: *mut Counter) {
    if !c.is_null() {
        drop(Box::from_raw(c)); // take ownership back and let Drop run exactly once
    }
}
"""

P.append(dict(
    slug="opaque-handle-api", title="A C-style API around a Rust object: opaque handles", level="hard", stage="build",
    tags=["FFI", "opaque pointer", "Box::into_raw", "Box::from_raw", "status codes", "caller-allocated buffers", "null handling", "snprintf"],
    teaches=["The C idiom for hiding an object: `new` returns a pointer, other functions take it, `free` destroys it. In Rust that is `Box::into_raw` and `Box::from_raw`, and the ownership rule must be written in the docs.",
             "No exceptions cross the boundary: failure is a status code, and every pointer argument is checked for null before it is used.",
             "Returning a string with no allocation of your own: the caller supplies the buffer, you copy at most `cap - 1` bytes, always terminate, and report the full length so truncation is detectable."],
    statement="""
        Write the C-facing API of a small Rust object, the way a `.so` or `.dll` written in Rust exposes it. `Counter` (a name and
        an `i64`) is the Rust type; C only ever holds a `*mut Counter`.

        - `counter_new(name)`: allocate on the heap and give the pointer away. A null `name` means `"counter"`; a name that is not
          valid UTF-8 returns null.
        - `counter_add`: checked addition, returning a status code. **Overflow leaves the value unchanged.**
        - `counter_get`: store the value through an out-pointer.
        - `counter_name`: `snprintf`-style copy into the caller's buffer (at most `cap - 1` bytes plus a NUL; nothing if `cap == 0`
          or `buf` is null) returning the **full** name length.
        - `counter_free`: take the object back and drop it. Null is a no-op.

        Every function must tolerate null handles (returning `ERR_NULL`) instead of crashing. A `drops()` counter is provided so the
        tests can check that every handle is dropped exactly once.
    """,
    starter=OH_STARTER,
    solution=OH,
    visible=[
        T("new_add_get_free", "the happy path", "unsafe { let c = counter_new(std::ptr::null()); counter_add(c, 5); counter_add(c, -2); let mut out = 0i64; let st = counter_get(c, &mut out); counter_free(c); (st, out) }", "(OK, 3)"),
        T("null_handles_are_errors", "no crash", "unsafe { let mut out = 0i64; (counter_add(std::ptr::null_mut(), 1), counter_get(std::ptr::null(), &mut out), counter_name(std::ptr::null(), std::ptr::null_mut(), 0)) }", "(ERR_NULL, ERR_NULL, ERR_NULL as isize)"),
        T("overflow_is_reported_and_harmless", "i64::MAX + 1", "unsafe { let c = counter_new(std::ptr::null()); counter_add(c, i64::MAX); let st = counter_add(c, 1); let mut out = 0i64; counter_get(c, &mut out); counter_free(c); (st, out) }", "(ERR_OVERFLOW, i64::MAX)"),
        T("default_name_is_copied_out", "the caller's buffer", "unsafe { let c = counter_new(std::ptr::null()); let mut buf = [0x7Fu8; 16]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, buf.len()); counter_free(c); (n, buf[..8].to_vec()) }", "(7, b\"counter\\0\".to_vec())"),
        T("free_null_is_fine", "no crash", "{ unsafe { counter_free(std::ptr::null_mut()) }; true }", "true"),
    ],
    hidden=[
        T("truncation_is_snprintf_style", "cap 4 for a 5-byte name", "unsafe { let name = std::ffi::CString::new(\"hello\").unwrap(); let c = counter_new(name.as_ptr()); let mut buf = [0x7Fu8; 8]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, 4); counter_free(c); (n, buf.to_vec()) }", "(5, vec![b'h', b'e', b'l', 0, 0x7F, 0x7F, 0x7F, 0x7F])"),
        T("capacity_zero_writes_nothing", "size query", "unsafe { let name = std::ffi::CString::new(\"hello\").unwrap(); let c = counter_new(name.as_ptr()); let mut buf = [0x7Fu8; 4]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, 0); let m = counter_name(c, std::ptr::null_mut(), 0); counter_free(c); (n, m, buf.to_vec()) }", "(5, 5, vec![0x7F; 4])"),
        T("exact_fit_and_one_short", "cap = len + 1 and cap = len", "unsafe { let name = std::ffi::CString::new(\"abc\").unwrap(); let c = counter_new(name.as_ptr()); let mut a = [0x7Fu8; 4]; let mut b = [0x7Fu8; 4]; counter_name(c, a.as_mut_ptr() as *mut std::ffi::c_char, 4); counter_name(c, b.as_mut_ptr() as *mut std::ffi::c_char, 3); counter_free(c); (a.to_vec(), b.to_vec()) }", "(vec![b'a', b'b', b'c', 0], vec![b'a', b'b', 0, 0x7F])"),
        T("invalid_utf8_name_gives_null", "0xFF is not UTF-8", "{ let name = std::ffi::CString::new(vec![0xFFu8, b'x']).unwrap(); counter_new(name.as_ptr()).is_null() }", "true"),
        T("unicode_names_survive", "multi-byte name", "unsafe { let name = std::ffi::CString::new(\"caf\\u{e9}\").unwrap(); let c = counter_new(name.as_ptr()); let mut buf = [0u8; 16]; let n = counter_name(c, buf.as_mut_ptr() as *mut std::ffi::c_char, buf.len()); counter_free(c); (n, buf[..6].to_vec()) }", "(5, \"caf\\u{e9}\\0\".as_bytes().to_vec())"),
        T("negative_overflow_too", "i64::MIN - 1", "unsafe { let c = counter_new(std::ptr::null()); counter_add(c, i64::MIN); let st = counter_add(c, -1); let mut out = 0i64; counter_get(c, &mut out); counter_free(c); (st, out) }", "(ERR_OVERFLOW, i64::MIN)"),
        T("null_out_pointer", "get with nowhere to write", "unsafe { let c = counter_new(std::ptr::null()); let st = counter_get(c, std::ptr::null_mut()); counter_free(c); st }", "ERR_NULL"),
        T("handles_are_independent", "two counters", "unsafe { let a = counter_new(std::ptr::null()); let b = counter_new(std::ptr::null()); counter_add(a, 1); counter_add(b, 10); let (mut x, mut y) = (0i64, 0i64); counter_get(a, &mut x); counter_get(b, &mut y); counter_free(a); counter_free(b); (x, y) }", "(1, 10)"),
        T("every_handle_is_dropped_exactly_once", "free runs Drop, free(null) does not", "unsafe { let base = drops(); let a = counter_new(std::ptr::null()); let b = counter_new(std::ptr::null()); counter_free(std::ptr::null_mut()); let none = drops() - base; counter_free(a); let one = drops() - base; counter_free(b); (none, one, drops() - base) }", "(0, 1, 2)"),
        T("a_failed_new_does_not_allocate_a_handle_to_drop", "null result, nothing to free", "unsafe { let base = drops(); let name = std::ffi::CString::new(vec![0xFFu8]).unwrap(); let c = counter_new(name.as_ptr()); counter_free(c); drops() - base }", "0"),
    ],
    wrong=dict(
        add_wraps_silently=sub(OH, "    match c.value.checked_add(delta) {\n        Some(v) => {\n            c.value = v;\n            OK\n        }\n        None => ERR_OVERFLOW,\n    }", "    c.value = c.value.wrapping_add(delta);\n    OK"),
        overflow_still_changes_the_value=sub(OH, "        None => ERR_OVERFLOW,", "        None => {\n            c.value = c.value.wrapping_add(delta);\n            ERR_OVERFLOW\n        }"),
        name_is_not_terminated=sub(OH, "        *buf.add(n) = 0;\n", ""),
        name_returns_the_copied_length=sub(OH, "    bytes.len() as isize\n}", "    bytes.len().min(cap.saturating_sub(1)) as isize\n}"),
        name_ignores_the_capacity=sub(OH, "        let n = bytes.len().min(cap - 1);", "        let n = bytes.len();"),
        free_leaks_the_object=sub(OH, "        drop(Box::from_raw(c)); // take ownership back and let Drop run exactly once", "        std::mem::forget(Box::from_raw(c));"),
        invalid_utf8_is_accepted=sub(OH, "        match unsafe { CStr::from_ptr(name) }.to_str() {\n            Ok(s) => s.to_string(),\n            Err(_) => return std::ptr::null_mut(),\n        }", "        unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()"),
        get_ignores_a_null_out_check=sub(OH, "    if c.is_null() || out.is_null() {\n        return ERR_NULL;\n    }", "    if c.is_null() {\n        return ERR_NULL;\n    }\n    if out.is_null() {\n        return OK;\n    }"),
    ),
    hints=[("approach", "`Box::into_raw(Box::new(obj))` leaks the box and returns the raw pointer: that is 'giving ownership to C'. `Box::from_raw(ptr)` rebuilds the box so it drops normally: that is `free`. Everything in between borrows through the pointer (`ptr.as_mut()` and `as_ref()` turn null into `None`)."),
           ("rust", "`let Some(c) = c.as_mut() else { return ERR_NULL };` is the compact null check. `i64::checked_add` returns `None` on overflow without changing anything. For `counter_name`: `n = bytes.len().min(cap - 1)`, `std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n)`, then `*buf.add(n) = 0`, and always return `bytes.len()`."),
           ("edge", "Only write to `buf` when it is non-null and `cap > 0`; `cap - 1` underflows when `cap` is 0. A double `counter_free` is undefined behaviour in C-land and cannot be caught here: say so in the safety docs. Never let a Rust panic escape these functions: the checked arithmetic and the null checks are what keep them panic-free.")],
    notes=("This is how SQLite, libgit2, OpenSSL and every Rust crate built as a `cdylib` (`ring`, `rustls-ffi`, `tantivy-ffi`) look from the outside. The `snprintf` convention (return the needed length, never write past `cap`) is the standard answer to 'who allocates the string?', and the status-code-plus-out-pointer shape is what `cbindgen` generates headers for.", "O(name length) per call", "O(1) besides the object"),
    follow_up="1) Add `counter_name_alloc` that returns a `CString` pointer for C to free, plus `counter_string_free`, and explain why the second function must exist (allocator mismatch). 2) Add a `counter_set_callback(c, fn(i64, *mut c_void), user_data)` that fires on every change; what must the docs say about threads and lifetimes? 3) Make the handle thread-safe (`Mutex<Counter>` behind the pointer) and say what C callers may now do. 4) Generate the C header with `cbindgen` mentally: write the `.h` by hand. 5) Add a generation counter to the handle to make most double-frees detectable in debug builds.",
    related=["Y2", "Y3", "C2"],
))

STAGES = [("use", "Use", "easy"), ("understand", "Understand", "medium"), ("build", "Build", "hard")]

if __name__ == "__main__":
    # the stage order: Easy and Medium (use), Medium (understand), Hard (build); the problems are already in that order
    n = write_track("y3-ffi", "Y3", "FFI: C and Rust together", "Y", "sde3", 3,
                    "Crossing the C boundary safely: C strings and ownership, repr(C) layout, callbacks and panics, and a C-style API over a Rust object.",
                    STAGES, P)
    print("Y3", n)
