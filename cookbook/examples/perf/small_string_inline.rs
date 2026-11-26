//! RECIPE 90 (perf): small-value optimisation: store short strings INLINE and avoid the heap entirely.
//! LEVEL: advanced
//! Use in: millions of short ids/names/keys (task ids, labels, tags) where `String` allocation dominates.   Run: cargo run --release --example small_string_inline
//! COMMON USES: interners, symbol tables, DB keys, tags/labels: strings that are almost always short.
#![allow(dead_code)]
//!
//! A `String` is 24 bytes (ptr,len,cap) PLUS a heap allocation for the text. If most values are short, store up to 22 bytes
//! directly inside the 24-byte value and only allocate for long ones (an enum with an inline variant and a heap variant).
//! Allocation count and cache misses both fall. Production crates: compact_str https://docs.rs/compact_str ,
//! smol_str https://docs.rs/smol_str , and for Vecs smallvec https://docs.rs/smallvec ; for dedup also consider interning (index_graph).
//! We reuse the counting allocator from counting_allocator so the claim is MEASURED, not assumed.
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn allocs<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.load(Relaxed);
    let r = f();
    (r, ALLOCS.load(Relaxed) - before)
}

const INLINE_CAP: usize = 22;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Id {
    Inline { len: u8, buf: [u8; INLINE_CAP] },
    Heap(Box<str>),
}

impl Id {
    fn new(s: &str) -> Id {
        if s.len() <= INLINE_CAP {
            let mut buf = [0u8; INLINE_CAP];
            buf[..s.len()].copy_from_slice(s.as_bytes());
            Id::Inline {
                len: s.len() as u8,
                buf,
            }
        } else {
            Id::Heap(s.into())
        }
    }
    fn as_str(&self) -> &str {
        match self {
            Id::Inline { len, buf } => {
                std::str::from_utf8(&buf[..*len as usize]).expect("valid UTF-8 by construction")
            }
            Id::Heap(b) => b,
        }
    }
    fn is_inline(&self) -> bool {
        matches!(self, Id::Inline { .. })
    }
}

fn main() {
    println!(
        "size_of: String={} Id={} (same footprint, but Id owns no heap memory for short text)",
        std::mem::size_of::<String>(),
        std::mem::size_of::<Id>()
    );
    assert!(std::mem::size_of::<Id>() <= 24);

    let names: Vec<String> = (0..1000).map(|i| format!("task-{i}")).collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

    let (strings, a1) = allocs(|| refs.iter().map(|s| s.to_string()).collect::<Vec<String>>());
    let (ids, a2) = allocs(|| refs.iter().map(|s| Id::new(s)).collect::<Vec<Id>>());
    println!(
        "1000 short ids as String: {a1} allocations; as inline Id: {a2} allocation(s) (just the Vec)"
    );
    assert!(a1 >= 1000 && a2 <= 2);
    assert!(ids.iter().all(|i| i.is_inline()));

    let long = "a-very-long-identifier-that-does-not-fit-inline";
    let (id_long, a3) = allocs(|| Id::new(long));
    println!(
        "long id spills to the heap: inline={} allocations={a3}",
        id_long.is_inline()
    );
    assert!(!id_long.is_inline() && a3 == 1);

    assert_eq!(ids[42].as_str(), "task-42");
    black_box((strings, ids));
    println!(
        "trade-offs: more code, a branch on every access, and (de)serialisation/Display impls to write: use compact_str/smol_str in practice."
    );
}
