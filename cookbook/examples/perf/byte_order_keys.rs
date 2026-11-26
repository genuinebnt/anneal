//! RECIPE 88 (perf): byte order: endianness, memcmp-sortable keys, bulk byte swaps, unaligned reads, and `align_to`.
//! LEVEL: intermediate
//! Use in: storage formats, index keys, network protocols, checksums, reading binary files fast.   Run: cargo run --release --example byte_order_keys
//! COMMON USES: index keys in databases (LSM/B-trees), protocols, file formats, checksums.
#![allow(dead_code)]
//!
//! Little-endian (x86, Apple Silicon) stores the LEAST significant byte first; big-endian ("network order") stores the MOST
//! significant first. Consequences: (1) file/wire formats must pick one explicitly (`to_le_bytes`/`to_be_bytes`), never `as` or transmute;
//! (2) BIG-endian bytes compare lexicographically in numeric order, so databases encode integer KEYS big-endian and compare with plain
//! `memcmp`. Signed ints and floats need a tweak (flip the sign bit / invert negatives) to sort correctly as bytes.
//! (3) Reading `u32` from an arbitrary byte offset may be unaligned: use `from_le_bytes`, `read_unaligned`, or the `bytemuck`
//! crate https://docs.rs/bytemuck for safe casts. Docs: https://doc.rust-lang.org/std/primitive.u32.html#method.swap_bytes
use std::hint::black_box;
use std::time::Instant;

fn enc_u32(x: u32) -> [u8; 4] {
    x.to_be_bytes()
}
fn enc_i32(x: i32) -> [u8; 4] {
    ((x as u32) ^ 0x8000_0000).to_be_bytes() // flip the sign bit: negatives now sort before positives
}
fn enc_f64(x: f64) -> [u8; 8] {
    let bits = x.to_bits();
    let key = if bits >> 63 == 1 {
        !bits
    } else {
        bits | (1 << 63)
    }; // negatives: invert all; positives: set sign bit
    key.to_be_bytes()
}

fn main() {
    println!(
        "this machine is {}-endian",
        if cfg!(target_endian = "little") {
            "little"
        } else {
            "big"
        }
    );
    let n: u32 = 0x0A0B_0C0D;
    println!(
        "0x0A0B0C0D  le={:02x?}  be={:02x?}  swapped={:#x}  to_be()={:#x}",
        n.to_le_bytes(),
        n.to_be_bytes(),
        n.swap_bytes(),
        n.to_be()
    );

    // 1. sortable keys
    let nums = [300u32, 5, 70_000, 1, 256, 65_535, 65_536];
    let mut by_be: Vec<_> = nums.iter().map(|&x| (enc_u32(x), x)).collect();
    by_be.sort();
    let mut by_le: Vec<_> = nums.iter().map(|&x| (x.to_le_bytes(), x)).collect();
    by_le.sort();
    println!(
        "\nsorted by big-endian bytes:    {:?}",
        by_be.iter().map(|p| p.1).collect::<Vec<_>>()
    );
    println!(
        "sorted by little-endian bytes: {:?}  <- not numeric order!",
        by_le.iter().map(|p| p.1).collect::<Vec<_>>()
    );
    let mut expect = nums;
    expect.sort();
    assert_eq!(by_be.iter().map(|p| p.1).collect::<Vec<_>>(), expect);
    assert_ne!(by_le.iter().map(|p| p.1).collect::<Vec<_>>(), expect);

    let ints = [5i32, -3, 0, -1_000_000, 42, i32::MIN, i32::MAX];
    let mut k: Vec<_> = ints.iter().map(|&x| (enc_i32(x), x)).collect();
    k.sort();
    let mut e = ints;
    e.sort();
    assert_eq!(k.iter().map(|p| p.1).collect::<Vec<_>>(), e);
    println!("signed ints sort correctly as bytes after flipping the sign bit");

    let floats = [-1.5f64, 0.0, 2.5, -100.0, 3.14159, 1e-9, -1e-9, 1e300];
    let mut fk: Vec<_> = floats.iter().map(|&x| (enc_f64(x), x)).collect();
    fk.sort_by(|a, b| a.0.cmp(&b.0));
    let mut fe = floats;
    fe.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(fk.iter().map(|p| p.1).collect::<Vec<_>>(), fe);
    println!("floats sort correctly as bytes with the bit trick");

    // composite key: (tenant: u16, timestamp: u64) -> concatenated big-endian bytes sorts by tenant, then time
    let mut key = Vec::new();
    key.extend_from_slice(&7u16.to_be_bytes());
    key.extend_from_slice(&1_700_000_000u64.to_be_bytes());
    println!("composite key = {key:02x?}");

    // 2. unaligned and byte-wise reads
    let buf: Vec<u8> = (0..16).collect();
    let safe = u32::from_le_bytes(buf[1..5].try_into().unwrap());
    let fast = unsafe { (buf.as_ptr().add(1) as *const u32).read_unaligned() }; // same result on little-endian
    println!(
        "\nunaligned u32 at offset 1: from_le_bytes={safe:#x} read_unaligned={fast:#x} equal={}",
        safe == fast
    );
    let (pre, mid, post) = unsafe { buf[1..].align_to::<u32>() };
    println!(
        "align_to::<u32>: prefix {} bytes, {} aligned u32s, suffix {} bytes (alignment decides the split)",
        pre.len(),
        mid.len(),
        post.len()
    );

    // 3. bulk byte swapping autovectorises
    let mut v: Vec<u32> = (0..32_000_000).collect();
    let t = Instant::now();
    for x in v.iter_mut() {
        *x = x.swap_bytes();
    }
    let a = t.elapsed();
    let t = Instant::now();
    for x in v.iter_mut() {
        *x = ((*x & 0xff) << 24) | ((*x & 0xff00) << 8) | ((*x >> 8) & 0xff00) | (*x >> 24);
    }
    let b = t.elapsed();
    black_box(&v);
    println!(
        "\nbulk swap of 32M u32: swap_bytes {a:.2?}, hand-written shifts {b:.2?} (both typically become one SIMD instruction per vector)"
    );
    assert_eq!(v[1], 1, "swapped twice = original");
}
