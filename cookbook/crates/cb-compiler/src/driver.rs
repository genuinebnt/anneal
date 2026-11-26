//! The back-end pipeline in one place: HIR -> MIR -> optimised MIR -> bytecode -> VM.
//! Used by the CLI, the benchmarks and the differential tests.

use crate::bc::{self, Chunk};
use crate::interp::{self, RunOptions, RuntimeError};
use crate::mir::{self, MirProgram};
use crate::opt::{self, OptOptions};
use crate::sema::Program;
use crate::vm::{self, VmStats};
use std::io::Write;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Interp,
    Vm,
}

#[derive(Default, Clone, Debug)]
pub struct Timings {
    pub lower: Duration,
    pub optimize: Duration,
    pub codegen: Duration,
}

pub struct Compiled {
    pub mir: MirProgram,
    pub chunk: Chunk,
    pub timings: Timings,
}

/// Lower, optimise (verifying after every pass when `opts.verify`) and generate bytecode.
pub fn compile(
    hir: &Program,
    opts: &OptOptions,
    dump: &mut dyn FnMut(&str, &MirProgram),
) -> Result<Compiled, String> {
    let t0 = Instant::now();
    let mut mir = mir::lower::lower(hir).map_err(|e| e.to_string())?;
    if opts.verify {
        mir::verify::verify(&mir)
            .map_err(|e| format!("MIR verification failed after lowering:\n{}", e.join("\n")))?;
    }
    let t1 = Instant::now();
    opt::optimize(&mut mir, opts, dump)?;
    let t2 = Instant::now();
    let chunk = bc::codegen::generate(&mir);
    if opts.verify {
        bc::verify::verify(&chunk)
            .map_err(|e| format!("bytecode verification failed:\n{}", e.join("\n")))?;
    }
    let t3 = Instant::now();
    Ok(Compiled {
        mir,
        chunk,
        timings: Timings {
            lower: t1 - t0,
            optimize: t2 - t1,
            codegen: t3 - t2,
        },
    })
}

/// Run an already compiled chunk on the VM.
pub fn run_chunk(
    c: &Chunk,
    out: &mut dyn Write,
    ro: &RunOptions,
) -> (Result<(), RuntimeError>, VmStats) {
    vm::run(c, out, ro)
}

/// Run `hir` on the chosen back end; the VM path compiles at `opts.level`.
pub fn run(
    hir: &Program,
    backend: Backend,
    opts: &OptOptions,
    out: &mut dyn Write,
    ro: &RunOptions,
) -> Result<Result<(), RuntimeError>, String> {
    match backend {
        Backend::Interp => Ok(interp::run_with(hir, out, ro)),
        Backend::Vm => {
            let c = compile(hir, opts, &mut |_, _| {})?;
            Ok(run_chunk(&c.chunk, out, ro).0)
        }
    }
}

/// One row of the `bench` table.
#[derive(Clone, Debug)]
pub struct BenchRow {
    pub name: String,
    pub interp: Duration,
    pub vm_o0: Duration,
    pub vm_o3: Duration,
    pub instrs_o0: u64,
    pub instrs_o3: u64,
    /// All three back ends printed the same output and ended the same way.
    pub agree: bool,
}

/// Time one program on the interpreter, VM -O0 and VM -O3 (a high fuel limit
/// so benchmarks are not cut short). Must run on a big-stack thread.
pub fn bench_program(name: &str, hir: &Program) -> Result<BenchRow, String> {
    let ro = RunOptions {
        max_depth: 10_000,
        fuel: u64::MAX / 2,
    };
    let t = Instant::now();
    let mut out_i = Vec::new();
    let r_i = interp::run_with(hir, &mut out_i, &ro);
    let interp = t.elapsed();
    let mut times = Vec::new();
    let mut outs = Vec::new();
    let mut res = Vec::new();
    for level in [0u8, 3] {
        let c = compile(
            hir,
            &OptOptions {
                level,
                dump_after: vec![],
                verify: true,
            },
            &mut |_, _| {},
        )?;
        let mut out = Vec::new();
        let t = Instant::now();
        let (r, stats) = run_chunk(&c.chunk, &mut out, &ro);
        times.push((t.elapsed(), stats.instructions));
        outs.push(out);
        res.push(r);
    }
    let same = |r: &Result<(), RuntimeError>| {
        r.as_ref().err().map(|e| e.to_string()) == r_i.as_ref().err().map(|e| e.to_string())
    };
    let agree = outs.iter().all(|o| *o == out_i) && res.iter().all(same);
    Ok(BenchRow {
        name: name.into(),
        interp,
        vm_o0: times[0].0,
        vm_o3: times[1].0,
        instrs_o0: times[0].1,
        instrs_o3: times[1].1,
        agree,
    })
}

pub fn format_bench(rows: &[BenchRow]) -> String {
    let ms = |d: Duration| format!("{:.1}", d.as_secs_f64() * 1000.0);
    let mut s = format!(
        "{:<20} {:>10} {:>10} {:>10} {:>9} {:>14} {:>14}  agree\n",
        "program", "interp ms", "vm-O0 ms", "vm-O3 ms", "speedup", "instrs O0", "instrs O3"
    );
    for r in rows {
        let speed = r.interp.as_secs_f64() / r.vm_o3.as_secs_f64().max(1e-9);
        s.push_str(&format!(
            "{:<20} {:>10} {:>10} {:>10} {:>8.1}x {:>14} {:>14}  {}\n",
            r.name,
            ms(r.interp),
            ms(r.vm_o0),
            ms(r.vm_o3),
            speed,
            r.instrs_o0,
            r.instrs_o3,
            if r.agree { "yes" } else { "NO" }
        ));
    }
    s
}

/// Lower, run exactly the given passes (verifying after each), generate bytecode.
/// Used by the per-pass tests and the golden MIR tests.
pub fn compile_passes(hir: &Program, passes: &[&str]) -> Result<Compiled, String> {
    let t0 = Instant::now();
    let mut mir = mir::lower::lower(hir).map_err(|e| e.to_string())?;
    mir::verify::verify(&mir)
        .map_err(|e| format!("MIR verification failed after lowering:\n{}", e.join("\n")))?;
    let t1 = Instant::now();
    opt::run_passes(
        &mut mir,
        passes,
        &OptOptions {
            level: 0,
            dump_after: vec![],
            verify: true,
        },
        &mut |_, _| {},
    )?;
    let t2 = Instant::now();
    // Code generation only understands the register form (it would silently drop phis),
    // so a pass list that ends in SSA form is lowered out of it on a copy: `mir` keeps
    // the SSA form for printing.
    let chunk = if mir.funcs.iter().any(|f| f.ssa) {
        let mut registers = mir.clone();
        registers.funcs.iter_mut().for_each(|f| {
            opt::ssa::from_ssa(f);
        });
        mir::verify::verify(&registers).map_err(|e| {
            format!(
                "MIR verification failed leaving SSA form:\n{}",
                e.join("\n")
            )
        })?;
        bc::codegen::generate(&registers)
    } else {
        bc::codegen::generate(&mir)
    };
    bc::verify::verify(&chunk)
        .map_err(|e| format!("bytecode verification failed:\n{}", e.join("\n")))?;
    Ok(Compiled {
        mir,
        chunk,
        timings: Timings {
            lower: t1 - t0,
            optimize: t2 - t1,
            codegen: t2.elapsed(),
        },
    })
}
