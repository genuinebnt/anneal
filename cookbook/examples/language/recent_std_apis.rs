//! RECIPE 201 (rust-features): recent STANDARD-LIBRARY additions, grouped by area, each with a use case and an assert.
//! LEVEL: intermediate
//! Use in: replacing hand-rolled helpers and small crates with std, reviewing code written for older compilers.   Run: cargo run --release --example recent_std_apis
#![allow(dead_code)]
//!
//! Everything here compiled on rustc 1.98.1 without `#![feature]`. A version in a comment was read in the release notes
//! (https://doc.rust-lang.org/stable/releases.html); "recent" means the item compiles but its version was not checked.
//! Areas: collections, slices/arrays, numbers, iterators/options, sync/cells, io/fs/paths, time/char/errors, pointers.
//! Each demo states the pre-feature workaround it replaces.
//!
//! COMMON USES: queue/cache eviction (extract_if, pop_if), batch processing and protocol framing (chunk_by, as_chunks,
//! first_chunk), pagination maths (div_ceil), checked arithmetic in parsers (checked_*, strict_*), global config
//! (LazyLock/OnceLock), single-instance locks (File::lock), pointer tagging (strict provenance), timeouts (Duration::from_mins).
//! Read: https://doc.rust-lang.org/stable/releases.html
//! Read: https://doc.rust-lang.org/std/
use std::cell::{Cell, LazyCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, OnceLock, RwLock};
use std::time::Duration;

// ===== collections =====
fn collections() {
    // Vec::extract_if (1.87): remove-and-collect matching items in one pass.
    // Replaces: `retain` + a separate pass to collect the removed, or the `drain_filter` crate.
    let mut jobs = vec![("a", 1), ("b", 9), ("c", 2), ("d", 8)];
    let urgent: Vec<_> = jobs.extract_if(.., |j| j.1 > 5).collect();
    assert_eq!((urgent.len(), jobs.len()), (2, 2));

    // Vec::pop_if: pop the last element only if a predicate holds. Replaces: `if v.last().is_some_and(..) { v.pop() }`.
    let mut stack = vec![1, 5, 10];
    assert_eq!(stack.pop_if(|top| *top > 7), Some(10));
    assert_eq!(stack.pop_if(|top| *top > 7), None);
    // VecDeque::pop_front_if (1.93): expire the oldest entry only when stale (sliding-window rate limiter).
    let mut window = VecDeque::from([100, 105, 160]);
    while window.pop_front_if(|t| *t < 150).is_some() {}
    assert_eq!(window, [160]);

    // BTreeMap::pop_first / first_entry / extract_if (1.91): priority queue and range eviction.
    // Replaces: `let k = *m.keys().next()?; m.remove(&k)` which does two lookups.
    let mut timers = BTreeMap::from([(30, "c"), (10, "a"), (20, "b")]);
    assert_eq!(timers.pop_first(), Some((10, "a")));
    if let Some(mut e) = timers.first_entry() {
        *e.get_mut() = "B";
    }
    assert_eq!(timers[&20], "B");
    let odd: Vec<_> = timers.extract_if(.., |k, _| k % 20 != 0).collect(); // removes 30
    assert_eq!(odd, [(30, "c")]);

    // HashMap::extract_if (1.88): evict expired sessions and get them back for logging.
    let mut sessions: HashMap<u32, u64> = (1..=6).map(|i| (i, i as u64 * 10)).collect();
    let mut expired: Vec<_> = sessions
        .extract_if(|_, ttl| *ttl < 35)
        .map(|(k, _)| k)
        .collect();
    expired.sort();
    assert_eq!((expired, sessions.len()), (vec![1, 2, 3], 3));

    // Box<[T]> by-value iteration (recent; edition 2024 makes `.into_iter()` yield T): no `into_vec()` detour.
    let boxed: Box<[String]> = vec!["x".to_string(), "y".to_string()].into_boxed_slice();
    let owned: Vec<String> = boxed.into_iter().collect();
    assert_eq!(owned, ["x", "y"]);
}

// ===== slices and arrays =====
fn slices_and_arrays() {
    // get_disjoint_mut (recent): two &mut into one slice. Replaces: `split_at_mut` index juggling or unsafe.
    let mut balances = [100, 50, 0];
    let [from, to] = balances.get_disjoint_mut([0, 2]).unwrap();
    *from -= 30;
    *to += 30;
    assert_eq!(balances, [70, 50, 30]);
    assert!(balances.get_disjoint_mut([1, 1]).is_err()); // overlapping indices are rejected

    // chunk_by (recent): group consecutive equal-key runs (run-length encoding). Replaces: manual loop with `last()`.
    let runs: Vec<(u8, usize)> = [1u8, 1, 2, 2, 2, 3]
        .chunk_by(|a, b| a == b)
        .map(|c| (c[0], c.len()))
        .collect();
    assert_eq!(runs, [(1, 2), (2, 3), (3, 1)]);

    // as_chunks (1.88) / first_chunk / split_first_chunk: fixed-size views with compile-time length, no panics or `try_into`.
    let packet = [0xCAu8, 0xFE, 0, 0, 0, 7, 9, 9];
    let (magic, rest) = packet.split_first_chunk::<2>().unwrap();
    let (len_bytes, body) = rest.split_first_chunk::<4>().unwrap();
    assert_eq!(
        (
            u16::from_be_bytes(*magic),
            u32::from_be_bytes(*len_bytes),
            body.len()
        ),
        (0xCAFE, 7, 2)
    );
    let (words, tail) = [1u8, 2, 3, 4, 5].as_chunks::<2>();
    assert_eq!((words, tail), (&[[1, 2], [3, 4]][..], &[5u8][..]));
    assert_eq!([1, 2, 3].first_chunk::<4>(), None);

    // array::from_fn / each_ref / repeat (1.91) / array_windows (1.94, windows of const size).
    let squares: [usize; 5] = std::array::from_fn(|i| i * i);
    let names = [String::from("a"), String::from("bb")];
    let lens = names.each_ref().map(|s| s.len()); // [&String; N] then map: no clone, array stays usable
    let zeros: [u8; 3] = std::array::repeat(0);
    let rising = [1, 3, 6, 10].array_windows::<2>().all(|[a, b]| a < b);
    assert_eq!(
        (squares[4], lens, zeros, rising, names.len()),
        (16, [1, 2], [0; 3], true, 2)
    );

    // split_off_first / split_off (1.87): consume a prefix of a slice reference. Replaces: reassigning `buf = &buf[n..]`.
    let mut buf: &[u8] = &[1, 2, 3, 4];
    assert_eq!(buf.split_off_first(), Some(&1));
    assert_eq!(buf.split_off(..2), Some(&[2u8, 3][..]));
    assert_eq!(buf, [4]);
}

// ===== numbers =====
fn numbers() {
    // Pagination: div_ceil replaces `(n + size - 1) / size`, which overflows near MAX.
    assert_eq!(10u32.div_ceil(3), 4);
    // isqrt / ilog / checked_ilog: integer-exact, no float rounding. Replaces `(n as f64).sqrt() as u64` (wrong above 2^53).
    assert_eq!(u64::MAX.isqrt(), u32::MAX as u64);
    assert_eq!(1000u32.ilog10(), 3);
    assert_eq!(81u32.ilog(3), 4);
    assert_eq!(0u32.checked_ilog2(), None);
    // midpoint: average without overflow. Replaces `a + (b - a) / 2` special cases (binary search, bucket centres).
    assert_eq!(250u8.midpoint(254), 252);
    assert_eq!(i8::MAX.midpoint(i8::MAX), i8::MAX); // `(a + b) / 2` would overflow
    // abs_diff: distance between unsigned values with no underflow. Replaces `max(a,b) - min(a,b)`.
    assert_eq!(3u32.abs_diff(10), 7);
    assert_eq!((-5i32).abs_diff(5), 10u32);
    // is_multiple_of (1.87) replaces `x % n == 0` (which also panics for n == 0 in the old form).
    assert!(12u32.is_multiple_of(4) && !12u32.is_multiple_of(5));
    // checked_sub_signed (1.90): apply a signed delta to an unsigned cursor/index; replaces casts through isize.
    assert_eq!(5usize.checked_sub_signed(-3), Some(8));
    assert_eq!(2usize.checked_sub_signed(3), None);
    // strict_* (1.91): panics on overflow in EVERY profile; release builds do not silently wrap like `+`.
    assert_eq!(250u8.strict_add(5), 255);
    // unbounded_shl (1.87): shifting by >= bit width yields 0 instead of panic/UB-like masking.
    assert_eq!(1u32.unbounded_shl(40), 0);
    // cast_signed/cast_unsigned (1.87): named, lint-friendly `as` replacement for sign reinterpretation.
    assert_eq!(255u8.cast_signed(), -1i8);
    // highest_one / bit_width (1.97): position/width of the top bit, replaces `BITS - leading_zeros`.
    assert_eq!((40u32.bit_width(), 40u32.highest_one()), (6, Some(5)));
    // NonZero ::div_ceil (1.92): no divide-by-zero check needed.
    let n = std::num::NonZero::new(7u32).unwrap();
    assert_eq!(n.div_ceil(std::num::NonZero::new(2).unwrap()).get(), 4);
}

// ===== iterators, options, results =====
fn iterators() {
    // repeat_n: padding without cloning forever. Replaces `repeat(x).take(n)`, and the last item is moved not cloned.
    let padded: String = "7".chars().chain(std::iter::repeat_n('0', 3)).collect();
    assert_eq!(padded, "7000");
    // is_sorted / is_sorted_by_key: validate sorted input before binary search. Replaces `windows(2).all(|w| w[0] <= w[1])`.
    assert!([1, 2, 2, 9].is_sorted() && ![3, 1].is_sorted());
    assert!(["a", "bb", "ccc"].iter().is_sorted_by_key(|s| s.len()));
    // std::iter::zip / chain (1.91): free functions that take IntoIterator. Replaces `a.into_iter().zip(b)` noise.
    let pairs: Vec<_> = std::iter::zip([1, 2], ["a", "b"]).collect();
    assert_eq!(pairs, [(1, "a"), (2, "b")]);
    assert_eq!(std::iter::chain([1], [2, 3]).count(), 3);
    // Option::take_if: take the value only when it satisfies a predicate (e.g. cancel an expired deadline).
    let mut deadline = Some(5);
    assert_eq!(deadline.take_if(|d| *d > 10), None);
    assert_eq!(deadline.take_if(|d| *d > 3), Some(5));
    assert_eq!(deadline, None);
    // Result::flatten (1.89): `Result<Result<T,E>,E>` from nested fallible steps. Replaces `.and_then(|r| r)`.
    let nested: Result<Result<u8, String>, String> = Ok(Err("inner".into()));
    assert_eq!(nested.flatten(), Err("inner".to_string()));
    // bool::ok_or / Option::map_or_default (1.98).
    assert_eq!((5 > 3).ok_or("no"), Ok(()));
    assert_eq!(None::<&str>.map_or_default(str::len), 0);
    // Peekable::next_if_map (1.94) is also available for tokenizers; std::assert_matches! (1.96) for tests.
}

// ===== sync and cells =====
static CONFIG: LazyLock<HashMap<&'static str, u32>> =
    LazyLock::new(|| HashMap::from([("workers", 4)]));
static SEED: OnceLock<u64> = OnceLock::new();
fn sync_and_cells() {
    // LazyLock: global computed on first use. Replaces the `lazy_static`/`once_cell` crates.
    assert_eq!(CONFIG["workers"], 4);
    // OnceLock::get_or_init: first caller wins, everyone sees the same value. Replaces `Mutex<Option<T>>` + check.
    assert_eq!(*SEED.get_or_init(|| 42), 42);
    assert_eq!(*SEED.get_or_init(|| 99), 42);
    // LazyCell (thread-local lazy) + LazyCell::get (1.94): peek without forcing initialisation.
    let cell = LazyCell::new(|| "expensive".len());
    assert_eq!(LazyCell::get(&cell), None);
    assert_eq!(*cell, 9);
    assert_eq!(LazyCell::get(&cell), Some(&9));
    // Cell::update (1.88): read-modify-write on a Cell without get/set pair.
    let hits = Cell::new(0);
    hits.update(|n| n + 1);
    hits.update(|n| n * 10);
    assert_eq!(hits.get(), 10);
    // AtomicBool::update/try_update (1.95): CAS loop built in. Replaces hand-written `compare_exchange_weak` loops.
    let flag = AtomicBool::new(false);
    let prev = flag.update(Ordering::SeqCst, Ordering::SeqCst, |b| !b);
    assert_eq!((prev, flag.load(Ordering::SeqCst)), (false, true));
    // RwLockWriteGuard::downgrade (1.92): publish a write then keep reading without letting other writers in.
    let lock = RwLock::new(vec![1]);
    let mut w = lock.write().unwrap();
    w.push(2);
    let r = std::sync::RwLockWriteGuard::downgrade(w);
    assert_eq!(r.len(), 2);
}

// ===== io, fs, paths =====
fn io_and_paths() {
    // File::lock / try_lock (1.89): cross-process advisory locks. Replaces the `fs2`/`fs4` crates.
    let path = std::env::temp_dir().join(format!("cookbook_r201_{}.lock", std::process::id()));
    let a = std::fs::File::create(&path).unwrap();
    a.lock().unwrap();
    let b = std::fs::File::open(&path).unwrap();
    assert!(
        b.try_lock().is_err(),
        "second exclusive lock must fail while the first is held"
    ); // WouldBlock
    a.unlock().unwrap();
    assert!(b.try_lock().is_ok());
    drop((a, b));
    std::fs::remove_file(&path).unwrap();
    // PathBuf::add_extension / Path::file_prefix (1.91): `x.tar` + `gz`, and "x" from "x.tar.gz".
    let p = std::path::PathBuf::from("backup.tar").with_added_extension("gz");
    assert_eq!(p, std::path::Path::new("backup.tar.gz"));
    assert_eq!(p.file_prefix().unwrap(), "backup");
    // io::pipe (1.87): anonymous pipe, e.g. to capture a child's stdout and stderr in one stream.
    use std::io::{Read, Write};
    let (mut rd, mut wr) = std::io::pipe().unwrap();
    wr.write_all(b"ping").unwrap();
    drop(wr);
    let mut s = String::new();
    rd.read_to_string(&mut s).unwrap();
    assert_eq!(s, "ping");
}

// ===== time, char, errors =====
fn time_char_errors() {
    // Duration::from_mins/from_hours (1.91), const: replaces `Duration::from_secs(60 * 60)` magic numbers.
    const SESSION: Duration = Duration::from_hours(2);
    const POLL: Duration = Duration::from_mins(5);
    assert_eq!(SESSION.as_secs() / POLL.as_secs(), 24);
    // char utilities: from_digit, MAX_LEN_UTF8 (1.93) to size a stack buffer, is_ascii_hexdigit.
    let mut buf = [0u8; char::MAX_LEN_UTF8];
    assert_eq!('é'.encode_utf8(&mut buf).len(), 2);
    assert_eq!(char::from_digit(11, 16), Some('b'));
    // core::error::Error: the Error trait usable from no_std libraries; same trait as std::error::Error.
    let e: Box<dyn core::error::Error> = "bad input".parse::<u8>().unwrap_err().into();
    assert!(e.to_string().contains("invalid digit"));
    // str::floor_char_boundary (1.91): truncate to a byte budget without splitting UTF-8.
    let s = "héllo";
    assert_eq!(&s[..s.floor_char_boundary(2)], "h");
    // fmt::from_fn (1.93): ad-hoc Display without a wrapper struct.
    let list = std::fmt::from_fn(|f| (1..=3).try_for_each(|i| write!(f, "{i};")));
    assert_eq!(list.to_string(), "1;2;3;");
}

// ===== pointers (strict provenance) =====
fn pointers() {
    // addr/with_addr/map_addr: treat the numeric address as data WITHOUT losing provenance. Replaces `ptr as usize` round trips.
    // Use case: tag low alignment bits of an aligned pointer.
    let x = Box::new(7u64); // 8-byte aligned, so the low 3 bits are free
    let raw: *mut u64 = Box::into_raw(x);
    let tagged = raw.map_addr(|a| a | 0b101);
    assert_eq!(tagged.addr() & 0b111, 0b101);
    let untagged = tagged.map_addr(|a| a & !0b111);
    assert_eq!(untagged, raw);
    assert_eq!(unsafe { *Box::from_raw(untagged) }, 7);
    let same = raw.with_addr(raw.addr());
    assert_eq!(same, raw);
    // mem::offset_of!: field offsets without pointer maths (see recent_stable_language_features).
    #[repr(C)]
    struct Rec {
        a: u8,
        b: u32,
    }
    assert_eq!(std::mem::offset_of!(Rec, b), 4);
    // hint::select_unpredictable (1.88): branchless choice when the condition is random (e.g. binary search step).
    assert_eq!(
        std::hint::select_unpredictable(raw.addr() % 2 == 0, 1, 1),
        1
    );
}

fn main() {
    let areas: [(&str, fn()); 8] = [
        ("collections", collections),
        ("slices/arrays", slices_and_arrays),
        ("numbers", numbers),
        ("iterators/options", iterators),
        ("sync/cells", sync_and_cells),
        ("io/fs/paths", io_and_paths),
        ("time/char/errors", time_char_errors),
        ("pointers", pointers),
    ];
    for (name, f) in areas {
        f();
        println!("ok  {name}");
    }
}
