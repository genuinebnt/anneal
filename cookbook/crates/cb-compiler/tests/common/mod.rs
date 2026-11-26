//! Shared helpers for the integration tests.
#![allow(dead_code)]

pub mod impgen;
pub mod progen;

use std::path::{Path, PathBuf};

pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run `f` on a thread with a big stack (the checker recurses on nested programs).
pub fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("test thread panicked")
}

pub fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .collect();
    v.sort();
    v
}

pub fn bless() -> bool {
    std::env::var_os("BLESS").is_some()
}

/// Compare `actual` with the file `expected`, or rewrite the file under `BLESS=1`.
pub fn expect_file(expected: &Path, actual: &str, failures: &mut Vec<String>) {
    if bless() {
        if actual.is_empty() {
            let _ = std::fs::remove_file(expected);
        } else {
            std::fs::write(expected, actual).expect("write blessed file");
        }
        return;
    }
    let want = std::fs::read_to_string(expected).unwrap_or_default();
    if want != actual {
        failures.push(format!(
            "--- {} ---\nexpected:\n{want}\nactual:\n{actual}",
            expected.display()
        ));
    }
}

/// Tiny deterministic PRNG (xorshift64*), so failures are reproducible from the seed.
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

/// The span-containment invariant: every node lies inside the file and inside its
/// parent, and node ids are unique. Returns a description of the first violation.
pub fn check_ast_invariants(m: &pebble::syntax::ast::Module, src_len: usize) -> Result<(), String> {
    use pebble::syntax::ast::Node;
    let mut ids = std::collections::HashSet::new();
    fn walk(
        n: Node,
        src_len: usize,
        ids: &mut std::collections::HashSet<u32>,
    ) -> Result<(), String> {
        let sp = n.span();
        if sp.end as usize > src_len || sp.start > sp.end {
            return Err(format!("span {sp} outside file of {src_len} bytes: {n:?}"));
        }
        if let Some(id) = n.id()
            && !ids.insert(id.0)
        {
            return Err(format!("duplicate node id {}", id.0));
        }
        for c in n.children() {
            if !sp.contains_span(c.span()) {
                return Err(format!(
                    "child span {} not inside parent span {sp}: child {c:?}",
                    c.span()
                ));
            }
            walk(c, src_len, ids)?;
        }
        Ok(())
    }
    walk(m.node(), src_len, &mut ids)
}

// ---------------------------------------------------------------- back ends

use pebble::driver::{self, Backend};
use pebble::interp::{RunOptions, RuntimeErrorKind};
use pebble::opt::OptOptions;

/// What one execution produced: output plus how it ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub stdout: String,
    /// `None` on success; otherwise the full error display (message and trace).
    pub error: Option<String>,
    pub kind: Option<RuntimeErrorKind>,
    /// The `-->` location (span of the failing expression).
    pub span: Option<pebble::source::Span>,
    /// Call site of every frame except the outermost (`main`'s is not recorded by the VM).
    pub call_sites: Vec<pebble::source::Span>,
}

pub fn analyze_ok(src: &str) -> pebble::Analysis {
    let an = pebble::analyze(src, &pebble::sema::CheckOptions::default());
    assert!(!an.has_errors(), "program has compile errors:\n{src}");
    an
}

pub fn run_on(
    hir: &pebble::sema::Program,
    backend: Backend,
    level: u8,
    ro: &RunOptions,
) -> Outcome {
    let mut out = Vec::new();
    let opts = OptOptions {
        level,
        dump_after: vec![],
        verify: true,
    };
    let res = driver::run(hir, backend, &opts, &mut out, ro)
        .unwrap_or_else(|e| panic!("compilation failed at -O{level}: {e}"));
    let (error, kind, span, call_sites) = match res {
        Ok(()) => (None, None, None, vec![]),
        Err(e) => {
            let n = e.trace.len();
            (
                Some(e.to_string()),
                Some(e.kind),
                e.span,
                e.trace
                    .iter()
                    .take(n.saturating_sub(1))
                    .map(|f| f.call_site)
                    .collect(),
            )
        }
    };
    Outcome {
        stdout: String::from_utf8_lossy(&out).to_string(),
        error,
        kind,
        span,
        call_sites,
    }
}

/// The interpreter and the VM at every optimisation level, labelled.
pub fn run_everywhere(hir: &pebble::sema::Program, ro: &RunOptions) -> Vec<(String, Outcome)> {
    let mut v = vec![("interp".to_string(), run_on(hir, Backend::Interp, 0, ro))];
    for l in 0..=3 {
        v.push((format!("vm-O{l}"), run_on(hir, Backend::Vm, l, ro)));
    }
    v
}

/// Do two outcomes agree? Compared: stdout, error kind, the full message with
/// the function trace, the `-->` span and the call site of every frame (so a
/// VM trap at any `-O` level points at the same source location as the
/// interpreter). Fuel is counted in different units (tree nodes vs
/// instructions), so when the oracle ran out of fuel the others only need to
/// run out of fuel too; where exactly the cut-off falls (output, span, trace) differs.
pub fn outcomes_agree(oracle: &Outcome, other: &Outcome) -> bool {
    if oracle.kind == Some(RuntimeErrorKind::FuelExhausted) {
        return other.kind == oracle.kind;
    }
    oracle == other
}

/// Assert that every back end agrees with the interpreter (the oracle).
pub fn assert_all_agree(what: &str, hir: &pebble::sema::Program, ro: &RunOptions) {
    let all = run_everywhere(hir, ro);
    let (_, oracle) = &all[0];
    for (label, o) in &all[1..] {
        assert!(
            outcomes_agree(oracle, o),
            "{what}: {label} differs from the interpreter\noracle: {oracle:?}\n{label}: {o:?}"
        );
    }
}

/// Find the `// passes: a,b,c` directive of a MIR golden test.
pub fn passes_directive(src: &str) -> Vec<String> {
    src.lines()
        .find_map(|l| l.strip_prefix("// passes:"))
        .map(|p| {
            p.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
