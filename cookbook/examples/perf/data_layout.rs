//! RECIPE 58 (perf): memory layout: struct size/padding, niches, and array-of-structs vs struct-of-arrays.
//! LEVEL: intermediate
//! Use in: hot loops over many records (tasks, events, vectors), reducing cache misses.   Run: cargo run --release --example data_layout
//! COMMON USES: columnar engines, ECS game engines, analytics: scan one field without dragging cold data through the cache.
#![allow(dead_code)]
//!
//! The CPU reads memory in 64-byte cache lines. If a loop only needs one small field but each record is large, every
//! cache line mostly carries data you do not use. Struct-of-arrays (SoA) stores each field in its own contiguous Vec
//! so a scan over one field touches only that data. `#[repr(C)]` keeps declared field order (padding follows);
//! default `repr(Rust)` is free to reorder fields to shrink the struct.
//! Read: https://nnethercote.github.io/perf-book/type-sizes.html and https://en.algorithmica.org/hpc/cpu-cache/
use std::hint::black_box;
use std::mem::{align_of, size_of};
use std::time::Instant;

#[repr(C)]
struct CLayout {
    a: u8,
    b: u64,
    c: u8,
}
struct RustLayout {
    a: u8,
    b: u64,
    c: u8,
}

struct TaskAos {
    id: u64,
    duration_ms: u32,
    payload: [u8; 52], // pretend cold data nobody reads in the hot loop
}

struct TasksSoa {
    id: Vec<u64>,
    duration_ms: Vec<u32>,
    payload: Vec<[u8; 52]>,
}

fn main() {
    println!(
        "repr(C) {} bytes, repr(Rust) {} bytes (align {})",
        size_of::<CLayout>(),
        size_of::<RustLayout>(),
        align_of::<RustLayout>()
    );
    println!(
        "niche: Option<Box<u8>>={} Box<u8>={} | Option<u32>={} u32={} | Option<&u8>={}",
        size_of::<Option<Box<u8>>>(),
        size_of::<Box<u8>>(),
        size_of::<Option<u32>>(),
        size_of::<u32>(),
        size_of::<Option<&u8>>()
    );
    println!(
        "&str={} String={} Vec<u8>={} Box<dyn Fn()>={}",
        size_of::<&str>(),
        size_of::<String>(),
        size_of::<Vec<u8>>(),
        size_of::<Box<dyn Fn()>>()
    );
    println!("TaskAos={} bytes", size_of::<TaskAos>());

    let n = 2_000_000;
    let aos: Vec<TaskAos> = (0..n)
        .map(|i| TaskAos {
            id: i as u64,
            duration_ms: (i % 100) as u32,
            payload: [0; 52],
        })
        .collect();
    let soa = TasksSoa {
        id: (0..n as u64).collect(),
        duration_ms: (0..n).map(|i| (i % 100) as u32).collect(),
        payload: vec![[0; 52]; n],
    };
    let _ = (&soa.id, &soa.payload);

    let t = Instant::now();
    let sum_aos: u64 = black_box(&aos).iter().map(|t| t.duration_ms as u64).sum();
    let aos_time = t.elapsed();
    let t = Instant::now();
    let sum_soa: u64 = black_box(&soa.duration_ms).iter().map(|&d| d as u64).sum();
    let soa_time = t.elapsed();
    assert_eq!(sum_aos, sum_soa);
    println!(
        "sum of one field over {n} tasks: AoS {aos_time:.2?} vs SoA {soa_time:.2?} (SoA reads {}x less memory)",
        size_of::<TaskAos>() / size_of::<u32>()
    );
    println!("(timings vary by machine; the memory ratio is the point)");
}
