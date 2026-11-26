//! RECIPE 87 (perf): struct layout: padding, field order, repr(C/packed/align), niches, bit-packing, and compile-time size checks.
//! LEVEL: intermediate
//! Use in: millions of small records (events, graph nodes, index entries), wire formats, cache-friendly design.   Run: cargo run --release --example struct_packing
//! COMMON USES: millions of small records: events, graph nodes, index entries; wire/file layouts.
#![allow(dead_code)]
//!
//! Each field must sit at an address that is a multiple of its ALIGNMENT, so the compiler inserts padding. Default `repr(Rust)` is
//! free to REORDER fields to minimise it; `repr(C)` keeps your order (needed for FFI/file layouts). `repr(packed)` removes padding but
//! makes fields unaligned: you cannot take references to them (UB), only copy values out. `repr(align(N))` raises alignment.
//! Smaller types mean more records per cache line. Check sizes at COMPILE time with `const _: () = assert!(...)` so a refactor
//! cannot silently double your memory. Read: https://doc.rust-lang.org/reference/type-layout.html and
//! https://nnethercote.github.io/perf-book/type-sizes.html  (clippy lint: large_enum_variant)
use std::mem::{align_of, size_of};
use std::num::NonZeroU32;

struct Rust {
    a: u8,
    b: u64,
    c: u8,
}
#[repr(C)]
struct C {
    a: u8,
    b: u64,
    c: u8,
}
#[repr(C)]
struct CSorted {
    b: u64,
    a: u8,
    c: u8,
}
#[repr(C, packed)]
struct Packed {
    a: u8,
    b: u64,
    c: u8,
}
#[repr(align(64))]
struct CacheLine(u8);

const _: () = assert!(size_of::<Rust>() == 16); // fails the BUILD if someone adds a field and bloats the type
const _: () = assert!(size_of::<CacheLine>() == 64);

// bit-packing: 3 bits of kind + 5 bits of priority in one byte
#[derive(Clone, Copy, PartialEq, Debug)]
struct Tag(u8);
impl Tag {
    fn new(kind: u8, priority: u8) -> Tag {
        assert!(kind < 8 && priority < 32);
        Tag(kind << 5 | priority)
    }
    fn kind(self) -> u8 {
        self.0 >> 5
    }
    fn priority(self) -> u8 {
        self.0 & 0b1_1111
    }
}

// flag sets as a single integer (what the `bitflags` crate generates for you)
#[derive(Clone, Copy, PartialEq, Debug)]
struct Perm(u8);
impl Perm {
    const READ: Perm = Perm(1);
    const WRITE: Perm = Perm(2);
    const EXEC: Perm = Perm(4);
    fn has(self, p: Perm) -> bool {
        self.0 & p.0 == p.0
    }
    fn with(self, p: Perm) -> Perm {
        Perm(self.0 | p.0)
    }
    fn without(self, p: Perm) -> Perm {
        Perm(self.0 & !p.0)
    }
}

enum BigVariant {
    Small(u8),
    Big([u8; 1000]),
}
enum BoxedVariant {
    Small(u8),
    Big(Box<[u8; 1000]>), // the large payload moves to the heap, the enum stays small
}

fn main() {
    println!("{:<22} size  align", "type");
    println!(
        "{:<22} {:>4}  {:>4}",
        "repr(Rust) u8,u64,u8",
        size_of::<Rust>(),
        align_of::<Rust>()
    );
    println!(
        "{:<22} {:>4}  {:>4}",
        "repr(C)   u8,u64,u8",
        size_of::<C>(),
        align_of::<C>()
    );
    println!(
        "{:<22} {:>4}  {:>4}",
        "repr(C) sorted u64 first",
        size_of::<CSorted>(),
        align_of::<CSorted>()
    );
    println!(
        "{:<22} {:>4}  {:>4}",
        "repr(packed)",
        size_of::<Packed>(),
        align_of::<Packed>()
    );
    println!(
        "{:<22} {:>4}  {:>4}",
        "repr(align(64))",
        size_of::<CacheLine>(),
        align_of::<CacheLine>()
    );
    println!("rule of thumb: declare fields largest-alignment first (or let repr(Rust) do it)");

    let p = Packed {
        a: 1,
        b: 0xdead_beef,
        c: 2,
    };
    // println!("{}", p.b);  // ERROR: reference to packed field is unaligned (E0793). Copy it out instead:
    let b = { p.b };
    let c = unsafe { std::ptr::addr_of!(p.c).read_unaligned() };
    println!("packed field copied out: {b:#x} {c}");

    println!("\nniches (the compiler hides the Option tag inside unused bit patterns):");
    println!(
        "  Option<NonZeroU32>={} Option<u32>={} Option<Box<u8>>={} Option<&u8>={}",
        size_of::<Option<NonZeroU32>>(),
        size_of::<Option<u32>>(),
        size_of::<Option<Box<u8>>>(),
        size_of::<Option<&u8>>()
    );
    println!(
        "  bool={} Option<bool>={} char={} Option<char>={}",
        size_of::<bool>(),
        size_of::<Option<bool>>(),
        size_of::<char>(),
        size_of::<Option<char>>()
    );

    println!("\nlarge enum variants make EVERY value that big:");
    println!(
        "  enum with [u8;1000] variant = {} bytes; with Box<[u8;1000]> = {} bytes",
        size_of::<BigVariant>(),
        size_of::<BoxedVariant>()
    );

    println!(
        "\nindex width: 1M edges as (usize,usize) = {} MB; as (u32,u32) = {} MB",
        1_000_000 * size_of::<(usize, usize)>() / 1_000_000,
        1_000_000 * size_of::<(u32, u32)>() / 1_000_000
    );

    let t = Tag::new(5, 17);
    println!(
        "\nTag packs kind={} priority={} into {} byte(s)",
        t.kind(),
        t.priority(),
        size_of::<Tag>()
    );
    assert_eq!((t.kind(), t.priority()), (5, 17));
    let rw = Perm::READ.with(Perm::WRITE);
    println!(
        "rw has write: {}, exec: {}, after removing write: {:?}",
        rw.has(Perm::WRITE),
        rw.has(Perm::EXEC),
        rw.without(Perm::WRITE)
    );
    assert!(size_of::<Rust>() < size_of::<C>() && size_of::<Packed>() == 10);
}
