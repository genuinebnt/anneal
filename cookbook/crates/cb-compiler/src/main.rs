//! `pebblec`: the PEBBLE compiler driver.
//!
//! Exit codes: 0 success (warnings allowed); 1 compile errors; 2 usage or I/O
//! error; 3 the program ran and failed at run time.

use pebble::diag::{self, RenderOptions};
use pebble::driver::{self, Backend};
use pebble::interp::RunOptions;
use pebble::opt::OptOptions;
use pebble::sema::CheckOptions;
use pebble::source::SourceFile;
use std::io::Write;
use std::process::ExitCode;

const USAGE: &str = "usage: pebblec <command> [options] FILE

commands:
  check FILE         type-check and print diagnostics
  run FILE           check, then run with the interpreter
  dump-tokens FILE   print the token stream
  dump-ast FILE      print the syntax tree
  dump-hir FILE      print the typed program
  dump-mir FILE      print the monomorphic MIR (after the optimiser at --opt-level)
  dump-cfg FILE      print the MIR control-flow graphs as Graphviz DOT
  dump-bc FILE       print the bytecode listing
  bench FILE|DIR     time interpreter vs VM -O0 vs VM -O3 on .peb files

options:
  --backend=interp|vm       how `run` executes the program (default vm)
  --opt-level=0..3          optimisation level (default 2; 0 for dump-mir/dump-cfg)
  --dump-after=PASS         print the MIR after a pass (simplify-cfg, const-prop, copy-prop,
                            cse, strength, dce, inline, tco, bce, or `all`); repeatable
  --time                    print phase timings to stderr
  --color                   colourise diagnostics
  --error-format=human|json diagnostic format (default human)
  --warn-shadow             enable the shadowing warning (W0505)";

fn main() -> ExitCode {
    // Deeply nested programs recurse deeply in the checker; give it room.
    let h = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(real_main);
    match h.map(|h| h.join()) {
        Ok(Ok(code)) => ExitCode::from(code),
        _ => {
            eprintln!("pebblec: internal failure");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> u8 {
    let mut color = false;
    let mut json = false;
    let mut shadow = false;
    let mut pos: Vec<String> = Vec::new();
    let mut backend = Backend::Vm;
    let mut opt_level: Option<u8> = None;
    let mut dump_after: Vec<String> = Vec::new();
    let mut time = false;
    let mut args = std::env::args().skip(1);
    while let Some(mut a) = args.next() {
        // Accept `--flag value` as well as `--flag=value` for the valued options.
        if matches!(a.as_str(), "--opt-level" | "--dump-after" | "--backend")
            && let Some(v) = args.next()
        {
            a = format!("{a}={v}");
        }
        match a.as_str() {
            "--time" => time = true,
            "--backend=interp" => backend = Backend::Interp,
            "--backend=vm" => backend = Backend::Vm,
            s if s.starts_with("--opt-level=") => match s["--opt-level=".len()..].parse::<u8>() {
                Ok(n) if n <= 3 => opt_level = Some(n),
                _ => {
                    eprintln!("pebblec: --opt-level must be 0, 1, 2 or 3");
                    return 2;
                }
            },
            s if s.starts_with("--dump-after=") => {
                let name = &s["--dump-after=".len()..];
                if name != "all" && !pebble::opt::PASS_NAMES.contains(&name) {
                    eprintln!(
                        "pebblec: unknown pass `{name}`; passes: {}",
                        pebble::opt::PASS_NAMES.join(", ")
                    );
                    return 2;
                }
                dump_after.push(name.to_string());
            }
            "--color" => color = true,
            "--warn-shadow" => shadow = true,
            "--error-format=json" => json = true,
            "--error-format=human" => json = false,
            "-h" | "--help" => {
                println!("{USAGE}");
                return 0;
            }
            s if s.starts_with("--") => {
                eprintln!("pebblec: unknown option `{s}`\n{USAGE}");
                return 2;
            }
            _ => pos.push(a),
        }
    }
    let [cmd, path] = pos.as_slice() else {
        eprintln!("{USAGE}");
        return 2;
    };
    if !matches!(
        cmd.as_str(),
        "check"
            | "run"
            | "dump-tokens"
            | "dump-ast"
            | "dump-hir"
            | "dump-mir"
            | "dump-cfg"
            | "dump-bc"
            | "bench"
    ) {
        eprintln!("pebblec: unknown command `{cmd}`\n{USAGE}");
        return 2;
    }
    if cmd == "bench" {
        return bench(path);
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("pebblec: cannot read `{path}`: {e}");
            return 2;
        }
    };
    let file = SourceFile::new(path.clone(), text);
    if cmd == "dump-tokens" {
        let (toks, _, _) = pebble::syntax::lex(&file.text);
        for t in toks {
            let lc = file.line_col_chars(t.span.start as usize);
            println!(
                "{}:{}  {:?}  {:?}",
                lc.line + 1,
                lc.col + 1,
                t.kind,
                file.slice(t.span)
            );
        }
        return 0;
    }
    let t_front = std::time::Instant::now();
    let an = pebble::analyze(
        &file.text,
        &CheckOptions {
            warn_shadow: shadow,
        },
    );
    let front = t_front.elapsed();
    if json {
        for d in &an.diagnostics {
            eprintln!("{}", diag::to_json(d, &file));
        }
    } else if !an.diagnostics.is_empty() {
        eprint!(
            "{}",
            diag::render_all(&an.diagnostics, &file, &RenderOptions { color })
        );
    }
    if an.has_errors() {
        return 1;
    }
    match cmd.as_str() {
        "dump-ast" => println!("{:#?}", an.ast),
        "dump-hir" => {
            for f in &an.hir.fns {
                println!("{:#?}", f);
            }
            for c in &an.hir.consts {
                println!("{:#?}", c);
            }
        }
        "dump-mir" | "dump-cfg" | "dump-bc" => {
            return dump_backend(cmd, &an.hir, opt_level, dump_after);
        }
        "run" if backend == Backend::Vm => {
            return run_vm(
                &an.hir,
                &file,
                opt_level.unwrap_or(2),
                dump_after,
                time,
                front,
            );
        }
        "run" => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let res = pebble::interp::run(&an.hir, &mut lock);
            let _ = lock.flush();
            if let Err(e) = res {
                eprintln!("{}", e.render(&file));
                return 3;
            }
        }
        _ => {}
    }
    0
}

fn dump_backend(
    cmd: &str,
    hir: &pebble::sema::Program,
    level: Option<u8>,
    dump_after: Vec<String>,
) -> u8 {
    let default = if cmd == "dump-bc" { 2 } else { 0 };
    let opts = OptOptions {
        level: level.unwrap_or(default),
        dump_after,
        verify: true,
    };
    let mut dump =
        |pass: &str, p: &pebble::mir::MirProgram| eprintln!("=== MIR after `{pass}` ===\n{p}");
    match driver::compile(hir, &opts, &mut dump) {
        Ok(c) => {
            match cmd {
                "dump-mir" => print!("{}", c.mir),
                "dump-cfg" => print!("{}", pebble::mir::print::dot(&c.mir)),
                _ => print!("{}", pebble::bc::text::disassemble(&c.chunk)),
            }
            0
        }
        Err(e) => {
            eprintln!("pebblec: {e}");
            2
        }
    }
}

fn run_vm(
    hir: &pebble::sema::Program,
    file: &SourceFile,
    level: u8,
    dump_after: Vec<String>,
    time: bool,
    front: std::time::Duration,
) -> u8 {
    let opts = OptOptions {
        level,
        dump_after,
        verify: cfg!(debug_assertions),
    };
    let mut dump =
        |pass: &str, p: &pebble::mir::MirProgram| eprintln!("=== MIR after `{pass}` ===\n{p}");
    let c = match driver::compile(hir, &opts, &mut dump) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("pebblec: {e}");
            return 2;
        }
    };
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let t = std::time::Instant::now();
    let (res, stats) = driver::run_chunk(&c.chunk, &mut lock, &RunOptions::default());
    let run = t.elapsed();
    let _ = lock.flush();
    if time {
        eprintln!(
            "phase timings: parse+check {front:?}, lower {:?}, optimise {:?}, codegen {:?}, run {run:?} ({} instructions)",
            c.timings.lower, c.timings.optimize, c.timings.codegen, stats.instructions
        );
        eprintln!(
            "monomorphisation: {} instances",
            c.mir.stats.total_instances
        );
    }
    if let Err(e) = res {
        eprintln!("{}", e.render(file));
        return 3;
    }
    0
}

fn bench(path: &str) -> u8 {
    let p = std::path::Path::new(path);
    let mut files: Vec<std::path::PathBuf> = if p.is_dir() {
        std::fs::read_dir(p)
            .map(|d| {
                d.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|f| f.extension().is_some_and(|x| x == "peb"))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![p.to_path_buf()]
    };
    files.sort();
    let mut rows = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            eprintln!("pebblec: cannot read `{}`", f.display());
            return 2;
        };
        let an = pebble::analyze(&text, &CheckOptions::default());
        if an.has_errors() {
            eprintln!("pebblec: `{}` has compile errors", f.display());
            return 1;
        }
        let name = f
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        match driver::bench_program(&name, &an.hir) {
            Ok(r) => rows.push(r),
            Err(e) => {
                eprintln!("pebblec: {e}");
                return 2;
            }
        }
    }
    print!("{}", driver::format_bench(&rows));
    0
}
