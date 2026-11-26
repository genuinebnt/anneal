//! RECIPE 86 (perf): inlining, `#[inline]`, `#[cold]`, dynamic dispatch cost, and the build settings that matter.
//! LEVEL: advanced
//! Use in: hot small functions, trait-object-heavy code, deciding generics vs `dyn`, tuning release builds.   Run: cargo run --release --example inlining_codegen
//! COMMON USES: hot small functions, trait-object-heavy code, deciding generics vs dyn, tuning release profiles.
#![allow(dead_code)]
//!
//! INLINING pastes a function's body into the caller. It removes call overhead AND lets the optimiser see across the boundary
//! (constants propagate, loops vectorise). The compiler inlines small functions in the SAME crate by itself. Hints:
//!   #[inline]          "consider inlining, even across crates" (needed for small public lib fns, else cross-crate calls stay calls
//!                      unless LTO is on). Generic functions are always available for inlining.
//!   #[inline(always)]  force (rarely right; can bloat code)       #[inline(never)]  forbid (benchmarks, cold paths, shrink binary)
//!   #[cold]            "rarely called": moves error/panic paths away from the hot path and biases branch layout.
//! Dispatch cost: generic `F: Fn` = monomorphised + inlinable; `&dyn Fn`/`Box<dyn Trait>` = indirect call through a vtable, usually
//! NOT inlinable when the callee is really unknown at compile time, so it blocks vectorisation. (If the optimiser can PROVE the
//! concrete type it will devirtualise; do not rely on that across module/crate boundaries.) Use dyn when you need runtime
//! polymorphism, generics in hot loops.
//! Inspect what the compiler did: https://github.com/pacak/cargo-show-asm (`cargo asm`) or https://godbolt.org/
//! Read: https://nnethercote.github.io/perf-book/inlining.html and https://nnethercote.github.io/perf-book/build-configuration.html
use std::hint::black_box;
use std::time::Instant;

#[inline(never)]
fn step_never(x: u32) -> u32 {
    x.wrapping_mul(3).wrapping_add(1)
}
#[inline(always)]
fn step_always(x: u32) -> u32 {
    x.wrapping_mul(3).wrapping_add(1)
}

// Each element is independent, so an INLINED step lets the compiler vectorise the whole loop (SIMD).
// A call through `#[inline(never)]`, `&dyn Fn` or a fn pointer is a real call per element: no vectorisation.
fn sum_generic<F: Fn(u32) -> u32>(data: &[u32], f: F) -> u64 {
    data.iter().map(|&x| f(x) as u64).sum()
}
fn sum_dyn(data: &[u32], f: &dyn Fn(u32) -> u32) -> u64 {
    data.iter().map(|&x| f(x) as u64).sum()
}
fn sum_fn_ptr(data: &[u32], f: fn(u32) -> u32) -> u64 {
    data.iter().map(|&x| f(x) as u64).sum()
}

#[cold]
#[inline(never)]
fn report_overflow(x: u64) -> ! {
    panic!("value too large: {x}") // error paths: keep them out of the hot instruction stream
}
fn checked_double(x: u64) -> u64 {
    if x > u64::MAX / 2 {
        report_overflow(x);
    }
    x * 2
}

fn time(label: &str, f: impl Fn() -> u64) -> u64 {
    let t = Instant::now();
    let mut r = 0;
    for _ in 0..200 {
        r = black_box(f()); // 200 passes over a 4 MB array so the work dominates timer noise
    }
    println!("{label:<40} {:>9.2?}", t.elapsed());
    r
}

fn main() {
    let data: Vec<u32> = (0..1 << 20).collect();
    let data = black_box(data);
    let a = time("generic closure, inlined (SIMD)", || {
        sum_generic(&data, |x| step_always(x))
    });
    let b = time("generic, callee #[inline(never)]", || {
        sum_generic(&data, |x| step_never(x))
    });
    // black_box hides the concrete closure so the compiler cannot DEVIRTUALISE the call. Without it, LLVM often sees that
    // `&|x| step_always(x)` is the only possible callee, inlines it, and `dyn` becomes free (try removing black_box to see 14ms).
    let c = time("&dyn Fn, opaque vtable (call per element)", || {
        sum_dyn(&data, black_box(&|x| step_always(x)))
    });
    let d = time("fn pointer (indirect call per element)", || {
        sum_fn_ptr(&data, step_never)
    });
    assert!(a == b && b == c && c == d);
    println!(
        "(identical results: {a}). Same arithmetic; the difference is purely whether the compiler could see through the call."
    );
    println!("{}", checked_double(21));

    println!("\nrecommended release profile for production binaries (Cargo.toml):");
    println!(
        "  [profile.release]\n  lto = \"fat\"          # optimise across crates (slower build, faster/smaller code)\n  codegen-units = 1    # fewer units = more inlining/optimisation, slower build\n  panic = \"abort\"      # smaller code, no unwinding (only if you do not rely on catch_unwind/JoinError panics)\n  # debug = 1          # keep symbols for flamegraphs, as ripgrep does"
    );
    println!(
        "and optionally `RUSTFLAGS=\"-C target-cpu=native\"` for your own machine (not for distributed binaries), or PGO: https://github.com/Kobzol/cargo-pgo"
    );
}
