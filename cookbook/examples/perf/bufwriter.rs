//! RECIPE 06 (perf): buffered output (and why `println!` in a loop is slow).
//! LEVEL: simple
//! Use in: stages 6 and 8.   Run: cargo run --release --example bufwriter
//! COMMON USES: log writers, CLI output, file exporters: buffering turns millions of syscalls into thousands.
//!
//! Pattern: each unbuffered write is a syscall. `BufWriter` batches them. For stdout, also take
//! `stdout().lock()` once instead of locking per line. Always `flush()` (or drop) before exiting.
//! Real code: ripgrep locks output in its message macros ->
//!   https://github.com/BurntSushi/ripgrep/blob/master/crates/core/messages.rs
#![allow(dead_code)]
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

fn main() -> std::io::Result<()> {
    let dir = std::env::temp_dir();
    let n = 200_000;

    let t = Instant::now();
    let mut raw = File::create(dir.join("cookbook_raw.txt"))?;
    for i in 0..n {
        writeln!(raw, "line {i}")?;
    }
    println!("unbuffered: {:?}", t.elapsed());

    let t = Instant::now();
    let mut buffered = BufWriter::new(File::create(dir.join("cookbook_buf.txt"))?);
    for i in 0..n {
        writeln!(buffered, "line {i}")?;
    }
    buffered.flush()?;
    println!("buffered:   {:?}", t.elapsed());
    Ok(())
}
