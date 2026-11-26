//! RECIPE 1021 (std): `String` building and mutation, `fmt::Write`, allocation counting, UTF-8 encoding by hand, and string-type memory sizes.
//! LEVEL: intermediate
//! Use in: report/CSV/SQL/log builders, template output, protocol encoders, anything that assembles text in a hot path.   Run: cargo run --release --example std_string_building_utf8
//! COMMON USES: building HTTP responses and log lines without reallocating, JSON/CSV writers, code generators, text editors (insert/remove/replace_range),
//! choosing Box<str>/Rc<str>/Cow<str> for interned or shared text, validating and decoding UTF-8 from sockets.
//! METHODS: String::new, String::with_capacity, String::push, String::push_str, String::pop, String::insert, String::insert_str, String::remove, String::clear,
//! String::len, String::capacity, String::reserve, String::shrink_to_fit, String::truncate, String::retain, String::drain, String::replace_range, String::split_off,
//! String::extend, String::from_utf8, String::into_bytes, String::into_boxed_str, String::as_mut_str, String::as_str, String::as_bytes, String::reserve_exact,
//! String::shrink_to, String::from_utf8_lossy, String::from_utf16, String::leak, String::remove_matches, String::extend_from_within, String::into_chars,
//! std::fmt::Write::write_str, std::fmt::Write::write_fmt, std::format, std::write, slice::concat, slice::join, char::len_utf8, char::len_utf16,
//! char::encode_utf8, char::encode_utf16, std::mem::size_of, std::rc::Rc, std::borrow::Cow, std::alloc::GlobalAlloc, str::from_utf8
//! DOCS: https://doc.rust-lang.org/std/string/struct.String.html
//! DEFINED IN: String/format! alloc; `fmt::Write` and char encoding core; GlobalAlloc core (`core::alloc`), System std.
//! NOT COVERED (and why): from_utf8_unchecked / as_mut_vec / from_raw_parts / into_raw_parts (unsafe), from_utf16_lossy (analogue of from_utf8_lossy),
//! into_chars / remove_matches / from_utf8_lossy_owned (unstable in 1.98.1), try_reserve_exact (twin of try_reserve),
//! from_utf16le / from_utf16be and their _lossy forms (since 1.98; byte-slice versions of from_utf16), clone_from (trait), Deref-provided str
//! methods (see std_str_search_split_parse).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   build a string of known size       -> `String::with_capacity(n)` then `push_str`
//!   append text / one char             -> `push_str(&str)` / `push(char)`
//!   append formatted text, no temp     -> `write!(s, "{x}")` with `use std::fmt::Write` (clippy::format_push_string)
//!   join many pieces                   -> `parts.join(sep)` / `parts.concat()`
//!   insert in the middle               -> `insert(i, c)` / `insert_str(i, s)` (O(n) shift; i is a byte index on a boundary)
//!   remove one char by byte index      -> `remove(i)` ; last char -> `pop()`
//!   filter characters in place         -> `retain(|c| ...)`
//!   cut the tail                       -> `truncate(n)` ; take the tail -> `split_off(n)`
//!   remove a range and iterate it      -> `drain(a..b)` ; replace a range -> `replace_range(a..b, "x")`
//!   bytes <-> String                   -> `String::from_utf8(vec)` / `into_bytes()`
//!   shrink a long-lived string         -> `into_boxed_str()` / `shrink_to_fit()`
//!   a read-only shared string          -> `Rc<str>` / `Arc<str>`; maybe-owned -> `Cow<str>`
//!   how many bytes does a char take    -> `c.len_utf8()` ; write it -> `c.encode_utf8(&mut buf)`
//!   reuse a buffer across iterations   -> `buf.clear()` (keeps capacity) instead of `String::new()`
//!
//! WHAT TO KNOW: a `String` is `Vec<u8>` guaranteed UTF-8: (ptr, len, capacity). Growth is amortised (capacity at least doubles) so N pushes
//! cost O(N) total with O(log N) allocations. Positions are BYTE indices and must be char boundaries or the call panics. The counting global
//! allocator below makes allocation counts an assertion instead of a guess.
//!
//! Read: https://doc.rust-lang.org/std/string/struct.String.html, https://doc.rust-lang.org/std/fmt/trait.Write.html,
//! https://en.wikipedia.org/wiki/UTF-8 (the bit layout used in section 7), https://www.unicode.org/reports/tr29/ (grapheme clusters)
//!
//! TRY NEXT:
//!   - [S] In `alloc_experiments`, add a fourth strategy (`collect::<String>()` from `iter::repeat_n`) and assert its allocation count is <= the `push_str` loop's.
//!   - [S] Predict: how many allocations does `String::new()` followed by 1000 `push('x')` make? Measure with `count_allocs` and compare to your `log2` guess.
//!   - [M] Extend `decode_utf8` to reject overlong encodings and surrogates; fuzz it against `std::str::from_utf8` on random bytes (differential test).
//!   - [M] Write `fn display_width(&str) -> usize` that treats combining marks (U+0300..U+036F) as zero width; test on "e\u{301}".
//!   - [A] PROJECT: combine this with `std_str_search_split_parse` and `std_vec_vecdeque_methods`: a line-oriented text editor buffer (Vec<String> + gap-free edits) with undo.
#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

// ---------------------------------------------------------------- counting allocator

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: we forward every call unchanged to `System`, only adding relaxed counter increments; layouts and pointers are never altered.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size(), Relaxed);
        // SAFETY: caller upholds GlobalAlloc::alloc's contract; we pass it through.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: `p` came from this allocator (hence System) with layout `l`.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        // A growing realloc is a "new allocation" for our purposes: it is what Vec/String growth costs.
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new, Relaxed);
        // SAFETY: forwarded unchanged.
        unsafe { System.realloc(p, l, new) }
    }
}
#[global_allocator]
static A: Counting = Counting;

/// Run `f` and return (result, allocation calls). Single-threaded demo: no other thread allocates meanwhile.
fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let before = ALLOCS.load(Relaxed);
    let r = f();
    (r, ALLOCS.load(Relaxed) - before)
}

const COVERED: &[&str] = &[
    "extend_from_within",
    "try_reserve",
    "new",
    "with_capacity",
    "push",
    "push_str",
    "pop",
    "insert",
    "insert_str",
    "remove",
    "clear",
    "len",
    "capacity",
    "reserve",
    "reserve_exact",
    "shrink_to_fit",
    "shrink_to",
    "truncate",
    "retain",
    "drain",
    "replace_range",
    "split_off",
    "extend",
    "from_utf8",
    "from_utf8_lossy",
    "from_utf16",
    "into_bytes",
    "into_boxed_str",
    "as_mut_str",
    "as_str",
    "as_bytes",
    "leak",
    "write_str",
    "write_fmt",
    "format",
    "write",
    "concat",
    "join",
    "len_utf8",
    "len_utf16",
    "encode_utf8",
    "encode_utf16",
    "size_of",
];

// ---------------------------------------------------------------- 0. Everyday core

fn section_everyday_core() {
    let mut s = String::new();
    s.push_str("hello");
    s.push(',');
    s.push_str(" world");
    assert_eq!(s, "hello, world");
    s.insert(0, '>');
    s.insert_str(1, "> ");
    assert_eq!(s, ">> hello, world");
    assert_eq!(s.pop(), Some('d'));
    assert_eq!(s.remove(0), '>');
    s.truncate(7);
    assert_eq!(s, "> hello");
    s.retain(|c| c != 'l');
    assert_eq!(s, "> heo");
    assert_eq!(s.len(), 5);
    s.clear();
    assert!(s.is_empty() && s.capacity() > 0, "clear keeps the buffer");
    let words = ["a", "b", "c"];
    assert_eq!(words.join("-"), "a-b-c");
    assert_eq!(words.concat(), "abc");
    assert_eq!(format!("{}-{:03}", "id", 7), "id-007");
}

// ---------------------------------------------------------------- 1. capacity and growth

fn section_capacity() {
    // with_capacity: ONE allocation when the size is known. reserve: grow ahead of a big append. reserve_exact: no amortisation slack.
    let (s, n) = count_allocs(|| {
        let mut s = String::with_capacity(100);
        for _ in 0..100 {
            s.push('x');
        }
        s
    });
    assert_eq!(n, 1);
    assert_eq!(s.capacity(), 100);
    // extend_from_within (since 1.87): append a copy of a byte range of itself; panics off a char boundary or out of range.
    let mut rep = String::from("ab-");
    rep.extend_from_within(..2);
    assert_eq!(rep, "ab-ab");
    // try_reserve (since 1.57): an allocation request that returns Err instead of aborting; use it for untrusted sizes.
    assert!(String::new().try_reserve(usize::MAX).is_err());
    assert!(String::new().try_reserve(64).is_ok());
    // leak (since 1.72): give up ownership for a &'static mut str (process-lifetime values only).
    let forever: &'static mut str = String::from("v1").leak();
    assert_eq!(forever, "v1");
    // Observed growth with the CURRENT std policy (at least doubling when full). The docs do not promise a factor:
    // rely only on "amortised O(1) push" and capacity >= len.
    let mut g = String::new();
    let mut caps = vec![];
    for _ in 0..4096 {
        g.push('a');
        if caps.last() != Some(&g.capacity()) {
            caps.push(g.capacity());
        }
    }
    assert!(caps.windows(2).all(|w| w[1] >= 2 * w[0]), "{caps:?}");
    assert!(caps.len() <= 12, "{caps:?}");
    // reserve vs reserve_exact vs shrink
    let mut r = String::from("abc");
    r.reserve(100);
    assert!(r.capacity() >= 103);
    r.shrink_to_fit();
    assert!(r.capacity() >= 3 && r.capacity() < 100);
    r.reserve_exact(10);
    assert!(r.capacity() >= 13);
    r.shrink_to(5); // shrink but keep at least 5
    assert!(r.capacity() >= 5 && r.capacity() < 13);
    // clear() keeps capacity => reuse ONE buffer in a loop instead of allocating per iteration.
    let (_, allocs) = count_allocs(|| {
        let mut buf = String::with_capacity(64);
        for i in 0..1000 {
            buf.clear();
            write!(buf, "row-{i}").unwrap();
            assert!(buf.starts_with("row-"));
        }
    });
    assert_eq!(allocs, 1);
}

// ---------------------------------------------------------------- 2. editing: insert/remove/retain/drain/replace_range/split_off

fn section_editing() {
    let mut s = String::from("héllo wörld");
    // Indices are BYTES. 'é' is 2 bytes, so "llo" starts at 3.
    s.insert(3, 'L');
    assert_eq!(s, "héLllo wörld");
    assert!(std::panic::catch_unwind(|| String::from("é").insert(1, 'x')).is_err()); // inside é
    // drain(range): removes AND yields the range lazily (the removal happens when the Drain drops).
    let mut d = String::from("key: value");
    let key: String = d.drain(..3).collect();
    assert_eq!((key.as_str(), d.as_str()), ("key", ": value"));
    // replace_range: splice in place, new text may differ in length. Equivalent to drain + insert_str but one memmove.
    let mut r = String::from("2026-01-02");
    r.replace_range(5..7, "12");
    assert_eq!(r, "2026-12-02");
    r.replace_range(..4, "19");
    assert_eq!(r, "19-12-02");
    // split_off(at): returns the tail, self keeps the head. Zero re-scan; used for splitting a buffer at a delimiter you found.
    let mut buf = String::from("HEADER\nbody text");
    let at = buf.find('\n').unwrap() + 1;
    let body = buf.split_off(at);
    assert_eq!((buf.as_str(), body.as_str()), ("HEADER\n", "body text"));
    // retain vs filter+collect: retain is in place (no new allocation).
    let mut digits = String::from("a1b2c3");
    let (_, n) = count_allocs(|| digits.retain(|c| c.is_ascii_digit()));
    assert_eq!((digits.as_str(), n), ("123", 0));
    // pop/truncate edge cases
    let mut e = String::new();
    assert_eq!(e.pop(), None);
    e.truncate(10); // truncate beyond len is a no-op, not a panic
    assert!(std::panic::catch_unwind(|| String::from("é").truncate(1)).is_err()); // not a boundary: panic
    // as_mut_str: &mut str for make_ascii_*
    let mut m = String::from("abc");
    m.as_mut_str().make_ascii_uppercase();
    assert_eq!(m, "ABC");
    // extend: from chars, &strs, Strings, even &chars.
    let mut x = String::new();
    x.extend(['a', 'b']);
    x.extend(["cd", "ef"]);
    x.extend(vec![String::from("g")]);
    assert_eq!(x, "abcdefg");
    assert_eq!(x.as_str().len(), x.as_bytes().len());
}

// ---------------------------------------------------------------- 3. format! vs write! vs push_str vs join

fn build_with_format(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s = format!("{s}{i},"); // anti-pattern: allocates a new string every iteration and copies all of s (O(n^2))
    }
    s
}
fn build_with_push_format(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!("{i},")); // one temporary String per item (clippy::format_push_string)
    }
    s
}
fn build_with_write(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        write!(s, "{i},").unwrap(); // writes straight into s, no temporary. Infallible for String, so unwrap is fine.
    }
    s
}
fn build_with_join(n: usize) -> String {
    (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(",") + ","
}

fn alloc_experiments() {
    let n = 500;
    let (a, ca) = count_allocs(|| build_with_format(n));
    let (b, cb) = count_allocs(|| build_with_push_format(n));
    let (c, cc) = count_allocs(|| build_with_write(n));
    let (d, cd) = count_allocs(|| build_with_join(n));
    assert_eq!(a, b);
    assert_eq!(b, c);
    assert_eq!(c, d);
    println!(
        "allocations for {n} items: format-rebuild={ca}, push_str(&format!)={cb}, write!={cc}, collect+join={cd}"
    );
    assert!(
        ca > cb && cb > cc,
        "format-rebuild > push_str(&format!) > write!"
    );
    // write! allocates only for growth, so O(log n); format!-per-item allocates once PER ITEM.
    assert!(cc < 30 && cb >= n);
    // fmt::Write is a trait you can implement for your own sinks: here, count bytes without storing them.
    struct Count(usize);
    impl std::fmt::Write for Count {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0 += s.len();
            Ok(())
        }
    }
    let mut c = Count(0);
    write!(c, "{:>8}|{:<4}|", 3.5, "ab").unwrap(); // write_fmt via the macro; computes length with zero allocation
    assert_eq!(c.0, 8 + 1 + 4 + 1);
    let (len, allocs) = count_allocs(|| {
        let mut c = Count(0);
        write!(c, "{}", 12345).unwrap();
        c.0
    });
    assert_eq!((len, allocs), (5, 0));
    // write_str directly
    let mut s = String::new();
    s.write_str("a").unwrap();
    s.write_char('b').unwrap();
    assert_eq!(s, "ab");
}

// ---------------------------------------------------------------- 4. bytes in and out

fn section_bytes() {
    let v = vec![0xe4, 0xb8, 0x96]; // 世
    let s = String::from_utf8(v).unwrap();
    assert_eq!(s, "世");
    // Failure returns the ORIGINAL Vec so you don't lose the bytes: into_bytes() on the error.
    let err = String::from_utf8(vec![b'a', 0xff]).unwrap_err();
    assert_eq!(err.utf8_error().valid_up_to(), 1);
    assert_eq!(err.into_bytes(), vec![b'a', 0xff]);
    assert_eq!(String::from("ab").into_bytes(), vec![97, 98]);
    assert_eq!(String::from_utf8_lossy(&[b'o', b'k', 0xc3]), "ok\u{fffd}");
    // UTF-16 (Windows APIs): from_utf16 fails on unpaired surrogates.
    let units: Vec<u16> = "a😀".encode_utf16().collect();
    assert_eq!(units.len(), 3);
    assert_eq!(String::from_utf16(&units).unwrap(), "a😀");
    assert!(String::from_utf16(&[0xD800]).is_err());
    // into_boxed_str drops the capacity word and shrinks the allocation: for long-lived read-only text.
    let mut big = String::with_capacity(1000);
    big.push_str("small");
    let boxed = big.into_boxed_str();
    assert_eq!(boxed.len(), 5);
    // leak: a &'static mut str for process-lifetime config. Never in a loop.
    let leaked: &'static str = String::from("static for the process").leak();
    assert!(leaked.starts_with("static"));
}

// ---------------------------------------------------------------- 5. chars vs graphemes

fn section_graphemes() {
    let precomposed = "\u{e9}"; // é as one code point
    let combining = "e\u{301}"; // e + COMBINING ACUTE ACCENT: looks identical
    println!(
        "{precomposed} vs {combining}: bytes {} vs {}, chars {} vs {}",
        precomposed.len(),
        combining.len(),
        precomposed.chars().count(),
        combining.chars().count()
    );
    assert_ne!(
        precomposed, combining,
        "different bytes although they render the same"
    );
    assert_eq!((precomposed.len(), combining.len()), (2, 3));
    assert_eq!(
        (precomposed.chars().count(), combining.chars().count()),
        (1, 2)
    );
    // A family emoji is 7 chars (4 people + 3 ZWJ) but ONE visible glyph; flags are two regional indicators.
    let family = "👨\u{200d}👩\u{200d}👧\u{200d}👦";
    assert_eq!(family.chars().count(), 7);
    assert_eq!(family.len(), 25);
    let flag = "🇯🇵";
    assert_eq!(flag.chars().count(), 2);
    // Consequences: `.chars().rev()` breaks combining marks; truncating by chars can strip an accent; len() is not display width.
    let rev: String = combining.chars().rev().collect();
    assert_eq!(
        rev, "\u{301}e",
        "the accent now precedes e: wrong rendering"
    );
    // Without a crate (unicode-segmentation) the std answer: treat a base char + following combining marks as a unit (approximate).
    fn approx_clusters(s: &str) -> usize {
        s.chars()
            .filter(|c| !('\u{300}'..='\u{36f}').contains(c) && *c != '\u{200d}')
            .count()
    }
    assert_eq!(approx_clusters(combining), 1);
    // Normalisation (NFC/NFD) is NOT in std: compare user-visible identifiers after normalising (unicode-normalization crate).
}

// ---------------------------------------------------------------- 6. UTF-8 by hand

fn encode_utf8(cp: u32) -> Option<Vec<u8>> {
    Some(match cp {
        0..=0x7f => vec![cp as u8],
        0x80..=0x7ff => vec![0xc0 | (cp >> 6) as u8, 0x80 | (cp & 0x3f) as u8],
        0xd800..=0xdfff => return None, // surrogates are not scalar values
        0x800..=0xffff => vec![
            0xe0 | (cp >> 12) as u8,
            0x80 | ((cp >> 6) & 0x3f) as u8,
            0x80 | (cp & 0x3f) as u8,
        ],
        0x10000..=0x10ffff => vec![
            0xf0 | (cp >> 18) as u8,
            0x80 | ((cp >> 12) & 0x3f) as u8,
            0x80 | ((cp >> 6) & 0x3f) as u8,
            0x80 | (cp & 0x3f) as u8,
        ],
        _ => return None,
    })
}

/// Decode one char from the front; returns (char, bytes used). Strict enough for the demo (no overlong check).
fn decode_utf8(b: &[u8]) -> Option<(char, usize)> {
    let first = *b.first()?;
    let (len, init) = match first {
        0x00..=0x7f => (1, first as u32),
        0xc0..=0xdf => (2, (first & 0x1f) as u32),
        0xe0..=0xef => (3, (first & 0x0f) as u32),
        0xf0..=0xf7 => (4, (first & 0x07) as u32),
        _ => return None,
    };
    let tail = b.get(1..len)?;
    let mut cp = init;
    for &c in tail {
        if c & 0xc0 != 0x80 {
            return None; // continuation bytes are 10xxxxxx
        }
        cp = (cp << 6) | (c & 0x3f) as u32;
    }
    char::from_u32(cp).map(|c| (c, len))
}

fn section_utf8_by_hand() {
    for (c, bytes) in [('A', 1), ('é', 2), ('世', 3), ('😀', 4)] {
        assert_eq!(c.len_utf8(), bytes);
        let mut buf = [0u8; 4]; // 4 is always enough: char::MAX_LEN_UTF8
        let s = c.encode_utf8(&mut buf); // returns &mut str, no heap
        assert_eq!(s.as_bytes(), encode_utf8(c as u32).unwrap().as_slice());
        assert_eq!(decode_utf8(s.as_bytes()), Some((c, bytes)));
    }
    assert_eq!('😀'.len_utf16(), 2);
    let mut u = [0u16; 2];
    assert_eq!('😀'.encode_utf16(&mut u), &[0xd83d, 0xde00]);
    // Differential test against std over every scalar value (1.1M) is fast enough: the whole range.
    let mut buf = [0u8; 4];
    let mut checked = 0u32;
    for cp in 0..=0x10ffffu32 {
        match char::from_u32(cp) {
            Some(c) => {
                let std_bytes = c.encode_utf8(&mut buf).as_bytes().to_vec();
                assert_eq!(encode_utf8(cp).as_deref(), Some(std_bytes.as_slice()));
                assert_eq!(decode_utf8(&std_bytes), Some((c, std_bytes.len())));
                checked += 1;
            }
            None => assert_eq!(encode_utf8(cp), None),
        }
    }
    assert_eq!(checked, 0x110000 - 0x800); // minus 2048 surrogates
}

// ---------------------------------------------------------------- 7. sizes of string types

fn section_sizes() {
    use std::mem::size_of;
    let w = size_of::<usize>();
    assert_eq!(size_of::<&str>(), 2 * w); // ptr + len (a "fat pointer")
    assert_eq!(size_of::<String>(), 3 * w); // ptr + len + capacity
    assert_eq!(size_of::<Box<str>>(), 2 * w);
    assert_eq!(size_of::<Rc<str>>(), 2 * w); // pointer to RcBox{strong,weak,bytes}, plus len
    assert_eq!(size_of::<Cow<str>>(), 3 * w); // niche in capacity makes Cow<str> the same size as String
    assert_eq!(
        size_of::<Option<String>>(),
        3 * w,
        "niche optimisation: Option adds nothing"
    );
    println!(
        "sizes on this target: &str={} String={} Box<str>={} Rc<str>={} Cow<str>={}",
        size_of::<&str>(),
        size_of::<String>(),
        size_of::<Box<str>>(),
        size_of::<Rc<str>>(),
        size_of::<Cow<str>>()
    );
    // Rc<str> clone is a refcount bump (no byte copy): intern repeated names.
    let name: Rc<str> = Rc::from("shared-name");
    let (copies, allocs) = count_allocs(|| (0..100).map(|_| Rc::clone(&name)).collect::<Vec<_>>());
    assert_eq!(copies.len(), 100);
    assert_eq!(allocs, 1, "only the Vec allocates; the strings are shared");
    assert_eq!(Rc::strong_count(&name), 101);
    // Cow: borrow when unchanged, own only when modified.
    fn normalise(s: &str) -> Cow<'_, str> {
        if s.contains('\r') {
            Cow::Owned(s.replace('\r', ""))
        } else {
            Cow::Borrowed(s)
        }
    }
    let (r, a) = count_allocs(|| normalise("clean"));
    assert!(matches!(r, Cow::Borrowed(_)) && a == 0);
    // Literals: byte strings, raw strings, escapes.
    let bytes: &[u8; 3] = b"abc";
    assert_eq!(bytes, &[b'a', b'b', b'c']);
    assert_eq!(b'x', 120);
    assert_eq!(br"a\n".len(), 3);
    let raw = r#"{"k": "v\n"}"#;
    assert_eq!(raw.len(), 12);
    assert_eq!(
        "line1\
               line2",
        "line1line2"
    ); // backslash-newline eats the indentation
    assert_eq!("tab\there".len(), 8);
}

// ---------------------------------------------------------------- Production tips and tricks

fn section_production_tips() {
    // 1. Reuse one buffer in a loop (clear keeps capacity). Slow way: String::new() per row.
    let mut line = String::with_capacity(32);
    let mut total = 0;
    for i in 0..100 {
        line.clear();
        write!(line, "{i}").unwrap();
        total += line.len();
    }
    assert_eq!(total, 10 + 90 * 2);
    // 2. Pre-size from the parts you already know. (clippy has no lint; Vec::with_capacity is the analogue of clippy::vec_init_then_push)
    let parts = ["alpha", "beta", "gamma"];
    let cap: usize = parts.iter().map(|p| p.len()).sum::<usize>() + parts.len() - 1;
    let mut s = String::with_capacity(cap);
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(p);
    }
    assert_eq!(s.capacity(), cap);
    // 3. write! not push_str(&format!()) : clippy::format_push_string. Proven above by allocation counts.
    // 4. `s += "x"` and `push_str` are the same; `s = s + "x"` reuses s too. But `s = format!("{s}x")` is O(n) per call.
    let mut acc = String::new();
    acc += "a";
    acc = acc + "b";
    assert_eq!(acc, "ab");
    // 5. Treat input strings as untrusted: bound them BEFORE allocating (take the first N bytes safely).
    fn bounded(s: &str, max: usize) -> &str {
        &s[..s.floor_char_boundary(max)]
    }
    assert_eq!(bounded("aé", 2), "a");
    // 6. Parse bytes with from_utf8 then error-handle; never from_utf8_unchecked on network data.
    assert!(
        String::from_utf8(vec![0xc0, 0x80]).is_err(),
        "overlong NUL is rejected by std"
    );
    // 7. Use `to_owned()`/`into()` for &str -> String, `String::from` in struct literals; `to_string()` goes through Display (slower on old compilers). clippy::inefficient_to_string is for &&str.
    let a: String = "x".into();
    assert_eq!(a, "x".to_owned());
    // 8. Avoid `&String` / `&Vec<T>` params (clippy::ptr_arg); take &str.
    // 9. Case-insensitive map keys: store lowercased keys once (to_ascii_lowercase at insert) instead of lowercasing every lookup.
    let mut m = std::collections::HashMap::new();
    m.insert("Content-Type".to_ascii_lowercase(), 1);
    assert_eq!(m.get(&"CONTENT-TYPE".to_ascii_lowercase()), Some(&1));
    // 10. `chars().rev()` on user text breaks grapheme clusters: reverse only ASCII/identifiers.
    assert_eq!("abc".chars().rev().collect::<String>(), "cba");
    // 11. Interned names: Rc<str>/Arc<str> cost 2 words and clone for free; String costs 3 words and a malloc per clone (clippy::redundant_clone catches the cases it can see).
    let arc: std::sync::Arc<str> = "tenant-42".into();
    let c = arc.clone();
    assert!(std::ptr::eq(arc.as_ptr(), c.as_ptr()));
    // 12. Return Cow<str> from "usually unchanged" transforms (see section_sizes): zero allocation on the fast path.
    // 13. Compare without allocating: `a.eq_ignore_ascii_case(b)`; `s == *"lit"`; `s.as_str() == "x"` instead of `s == "x".to_string()` (clippy::cmp_owned).
    assert!(String::from("x").as_str() == "x");
    // 14. Don't `unwrap()` the fmt::Result in LIBRARY code that writes to arbitrary fmt::Write; propagate with `?`. For String it is infallible.
    fn render(out: &mut impl std::fmt::Write, n: u32) -> std::fmt::Result {
        write!(out, "<{n}>")?;
        Ok(())
    }
    let mut o = String::new();
    render(&mut o, 5).unwrap();
    assert_eq!(o, "<5>");
}

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // hide only the deliberately demonstrated boundary panics
        let p = info.payload();
        let msg = p
            .downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| p.downcast_ref::<&str>().copied())
            .unwrap_or("");
        if !msg.contains("char boundary") && !msg.contains("is_char_boundary") {
            default_hook(info);
        }
    }));
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday_core();
    section_capacity();
    section_editing();
    alloc_experiments();
    section_bytes();
    section_graphemes();
    section_utf8_by_hand();
    section_sizes();
    section_production_tips();
    println!(
        "covered {} String entries; total allocations so far: {}, bytes requested: {}",
        COVERED.len(),
        ALLOCS.load(Relaxed),
        BYTES.load(Relaxed)
    );
}
