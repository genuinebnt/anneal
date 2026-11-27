//! Runs a user's solution against a problem's tests.
//!
//! Each run writes a throwaway cargo package (`solution`) with the user's
//! `src/lib.rs` and the problem's test files, then:
//!
//! 1. `cargo clippy --all-targets` for compiler errors and lints (skipped if disabled),
//! 2. `cargo test --no-run` under the compile time limit,
//! 3. `cargo test` with libtest's JSON output under the test time limit, one suite per test file.
//!
//! A problem's `[perf]` changes this: `release` builds and runs the tests with `--release`, one test at a
//! time; `asm` first compiles the library alone and writes its assembly next to the test prelude, where
//! `anneal_prelude::asm` reads it; `count_allocs` installs a counting global allocator in the tests.
//!
//! Builds share a target directory per cache key, so repeated runs of the same
//! problem compile incrementally. In [`Sandbox::Docker`] the container has no
//! network, a read-only root filesystem, capped memory, CPU and processes, and
//! runs as the invoking user.

mod ansi;
mod deps;
mod exec;
mod parse;
mod project;
mod python;
mod result;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use ansi::strip_ansi;
pub use deps::{VENDOR_DIR, known_crates};
pub use project::write as write_project;
pub use python::{PySubmission, SOLUTION_FILE as PYTHON_FILE};
pub use result::{
    Check, Diagnostic, Level, Outcome, RunResult, RunStatus, Span, Suite, TestOutcome,
};

/// What to run: the user's code plus the problem's tests.
#[derive(Debug, Clone, Copy)]
pub struct Submission<'a> {
    pub lib_rs: &'a str,
    pub visible_tests: &'a str,
    /// Present on Submit, absent on Run.
    pub hidden_tests: Option<&'a str>,
    /// Crates from the allowed set (`docker/deps/Cargo.toml`) the problem depends on.
    pub crates: &'a [String],
    /// Release build, assembly and allocation counting for performance problems.
    pub perf: Perf,
}

/// How a performance problem is built and measured (`[perf]` in problem.toml). All off by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Perf {
    /// `--release`, one test at a time.
    pub release: bool,
    /// Emit the library's release assembly for `anneal_prelude::asm`.
    pub asm: bool,
    /// Install the counting allocator behind `anneal_prelude::allocs`.
    pub count_allocs: bool,
}

/// What to run for "Run": the user's library plus a scratch `main` that calls into it.
#[derive(Debug, Clone, Copy)]
pub struct Scratch<'a> {
    pub lib_rs: &'a str,
    /// A binary crate that can `use solution::*;`.
    pub main_rs: &'a str,
    pub crates: &'a [String],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchStatus {
    /// Ran and exited with 0.
    Ok,
    /// Ran and exited with an error, usually a panic.
    Exited,
    CompileError,
    Timeout,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScratchResult {
    pub status: ScratchStatus,
    pub diagnostics: Vec<Diagnostic>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// Program output kept per stream; the rest is cut.
const OUTPUT_LIMIT: usize = 64 * 1024;

fn clip(mut s: String) -> String {
    if s.len() > OUTPUT_LIMIT {
        ansi::cut(&mut s, OUTPUT_LIMIT);
        s.push_str("\n… output cut at 64 KiB");
    }
    s
}

#[derive(Debug, Clone)]
pub enum Sandbox {
    /// Plain `cargo` on this machine. For development and tests only.
    Host,
    Docker {
        image: String,
        memory: String,
        cpus: String,
        /// Docker context to run in. `None` uses whatever context is current.
        context: Option<String>,
    },
}

/// anneal runs its sandbox on OrbStack unless told otherwise.
pub const DEFAULT_DOCKER_CONTEXT: &str = "orbstack";

impl Sandbox {
    /// A Docker sandbox on the OrbStack context.
    pub fn docker(image: impl Into<String>) -> Self {
        Sandbox::Docker {
            image: image.into(),
            memory: "1g".into(),
            cpus: "2".into(),
            context: Some(DEFAULT_DOCKER_CONTEXT.into()),
        }
    }

    /// Runs in `context` instead; an empty string means the current context.
    pub fn in_context(mut self, context: &str) -> Self {
        if let Sandbox::Docker { context: c, .. } = &mut self {
            *c = (!context.is_empty()).then(|| context.to_owned());
        }
        self
    }

    fn dir_name(&self) -> &'static str {
        match self {
            Sandbox::Host => "host",
            Sandbox::Docker { .. } => "docker",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub sandbox: Sandbox,
    /// Holds per-run work directories and the shared target directories.
    pub work_root: PathBuf,
    pub clippy: bool,
    pub compile_timeout: Duration,
    pub test_timeout: Duration,
}

impl RunnerConfig {
    pub fn new(sandbox: Sandbox, work_root: impl Into<PathBuf>) -> Self {
        RunnerConfig {
            sandbox,
            work_root: work_root.into(),
            clippy: true,
            compile_timeout: Duration::from_secs(90),
            test_timeout: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("{what}: {source}")]
    Io {
        what: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("cache key {0:?} may only contain letters, digits, '-' and '_'")]
    BadCacheKey(String),
    #[error("crate {0:?} isn't in the sandbox's crate set (docker/deps/Cargo.toml)")]
    UnknownCrate(String),
}

impl RunnerError {
    pub(crate) fn io(what: &'static str, source: std::io::Error) -> Self {
        RunnerError::Io { what, source }
    }
}

pub struct Runner {
    config: RunnerConfig,
}

impl Runner {
    pub fn new(config: RunnerConfig) -> Self {
        Runner { config }
    }

    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    /// Runs `sub`. `cache_key` names the shared build cache, normally the problem id.
    pub async fn run(
        &self,
        cache_key: &str,
        sub: &Submission<'_>,
    ) -> Result<RunResult, RunnerError> {
        let cfg = &self.config;
        // Runs sharing a target directory must not overlap: building and running are
        // separate cargo calls, so another build could replace the test binary in
        // between. A file lock covers other Runners and other processes too.
        let (target, _lock, work) = self.prepare(cache_key).await?;
        let vendored = matches!(cfg.sandbox, Sandbox::Docker { .. });
        project::write(work.path(), sub, vendored)?;
        // Docker builds use the image's vendored crates. Host builds (development) may fetch
        // from crates.io, but only need to when the problem has dependencies.
        let offline = vendored || sub.crates.is_empty();
        let with_offline = |args: &[&'static str]| -> Vec<&'static str> {
            args.iter().copied().filter(|a| offline || *a != "--offline").collect()
        };

        let start = Instant::now();
        let mut diagnostics = Vec::new();
        if cfg.clippy {
            let out = exec::cargo(
                &cfg.sandbox,
                work.path(),
                &target,
                &with_offline(&[
                    "clippy",
                    "--offline",
                    "--all-targets",
                    "--message-format=json-diagnostic-rendered-ansi",
                ]),
                cfg.compile_timeout,
            )
            .await?;
            if out.timed_out {
                return Ok(finish(RunStatus::Timeout, diagnostics, Vec::new(), start));
            }
            diagnostics = parse::diagnostics(&out.stdout);
            if parse::build_failed(&out.stdout) || diagnostics.iter().any(Diagnostic::is_error) {
                explain(&mut diagnostics, &out.stderr);
                return Ok(finish(
                    RunStatus::CompileError,
                    diagnostics,
                    Vec::new(),
                    start,
                ));
            }
        }

        let mut targets = vec!["--test", "visible"];
        if sub.hidden_tests.is_some() {
            targets.extend(["--test", "hidden"]);
        }
        if sub.perf.release {
            targets.push("--release");
        }
        if sub.perf.asm {
            // The library alone, in one codegen unit so every function lands in one file. The tests
            // `include_str!` the result, so this runs before they're built.
            let mut asm = with_offline(&["rustc", "--offline", "--lib", "--release", "--message-format=json-diagnostic-rendered-ansi"]);
            // rustc leaves small non-generic functions to their callers (cross-crate inlining), so a
            // library compiled alone wouldn't contain them. The threshold flag is unstable, which
            // `RUSTC_BOOTSTRAP` (set for the JSON test output) allows.
            asm.extend(["--", "--emit", project::ASM_EMIT, "-C", "codegen-units=1", "-Z", "cross-crate-inline-threshold=never"]);
            let out = exec::cargo(&cfg.sandbox, work.path(), &target, &asm, cfg.compile_timeout).await?;
            if out.timed_out {
                return Ok(finish(RunStatus::Timeout, diagnostics, Vec::new(), start));
            }
            if parse::build_failed(&out.stdout) {
                diagnostics.extend(parse::diagnostics(&out.stdout).into_iter().filter(Diagnostic::is_error));
                explain(&mut diagnostics, &out.stderr);
                return Ok(finish(RunStatus::CompileError, diagnostics, Vec::new(), start));
            }
        }
        // Build first, under the compile limit, so the test limit only counts test time.
        let mut build = with_offline(&["test", "--offline", "--no-run", "--message-format=json-diagnostic-rendered-ansi"]);
        build.extend(&targets);
        let out = exec::cargo(
            &cfg.sandbox,
            work.path(),
            &target,
            &build,
            cfg.compile_timeout,
        )
        .await?;
        if out.timed_out {
            return Ok(finish(RunStatus::Timeout, diagnostics, Vec::new(), start));
        }
        // With clippy off, this build is the first place compile errors show up.
        let build_diags = parse::diagnostics(&out.stdout);
        if !cfg.clippy {
            diagnostics = build_diags;
        } else {
            diagnostics.extend(build_diags.into_iter().filter(Diagnostic::is_error));
        }
        if parse::build_failed(&out.stdout) {
            explain(&mut diagnostics, &out.stderr);
            return Ok(finish(
                RunStatus::CompileError,
                diagnostics,
                Vec::new(),
                start,
            ));
        }

        let mut run = with_offline(&["test", "--offline", "--no-fail-fast"]);
        run.extend(&targets);
        run.extend([
            "--",
            "-Z",
            "unstable-options",
            "--format",
            "json",
            "--report-time",
            // Keep println!/dbg! output from passing tests too, for the Output panel.
            "--show-output",
        ]);
        if sub.perf.release {
            // Timings mean nothing while other tests compete for the CPU.
            run.push("--test-threads=1");
        }
        let out = exec::cargo(&cfg.sandbox, work.path(), &target, &run, cfg.test_timeout).await?;
        let tests = parse::tests(&out.stdout, &out.stderr);
        let status = if out.timed_out {
            RunStatus::Timeout
        } else if !tests.is_empty() && tests.iter().all(|t| t.outcome == Outcome::Passed) {
            RunStatus::Passed
        } else {
            RunStatus::Failed
        };
        Ok(finish(status, diagnostics, tests, start))
    }
}

/// Adds cargo's own account of a failed build when no compiler error says why.
fn explain(diagnostics: &mut Vec<Diagnostic>, stderr: &str) {
    if !diagnostics.iter().any(Diagnostic::is_error) {
        diagnostics.push(parse::build_failure(stderr));
    }
}

/// Holds an exclusive lock on `<target>/.anneal-lock` until dropped.
impl Runner {
    /// Builds the scratch binary and runs it once. Shares `cache_key`'s build cache with [`Runner::run`].
    pub async fn run_scratch(&self, cache_key: &str, s: &Scratch<'_>) -> Result<ScratchResult, RunnerError> {
        let cfg = &self.config;
        let (target, _lock, work) = self.prepare(cache_key).await?;
        let vendored = matches!(cfg.sandbox, Sandbox::Docker { .. });
        project::write_scratch(work.path(), s, vendored)?;
        let offline = vendored || s.crates.is_empty();
        let args = |a: &[&'static str]| -> Vec<&'static str> { a.iter().copied().filter(|x| offline || *x != "--offline").collect() };
        let start = Instant::now();
        let done = |status, diagnostics, out: Option<exec::Captured>| {
            let (stdout, stderr, exit_code) = out.map_or((String::new(), String::new(), None), |o| (clip(o.stdout), clip(o.stderr), o.exit_code));
            ScratchResult { status, diagnostics, stdout, stderr, exit_code, duration_ms: start.elapsed().as_millis() as u64 }
        };

        let build = exec::cargo(&cfg.sandbox, work.path(), &target, &args(&["build", "--offline", "--bin", "scratch", "--message-format=json-diagnostic-rendered-ansi"]), cfg.compile_timeout).await?;
        if build.timed_out {
            return Ok(done(ScratchStatus::Timeout, Vec::new(), None));
        }
        let mut diagnostics = parse::diagnostics(&build.stdout);
        if parse::build_failed(&build.stdout) {
            explain(&mut diagnostics, &build.stderr);
            return Ok(done(ScratchStatus::CompileError, diagnostics, None));
        }
        // Run the built binary itself: `cargo run` would replay cached compiler warnings into stderr.
        let out = exec::run(&cfg.sandbox, work.path(), &target, exec::Program::Built("debug/scratch"), &[], cfg.test_timeout).await?;
        let status = if out.timed_out {
            ScratchStatus::Timeout
        } else if out.exit_code == Some(0) {
            ScratchStatus::Ok
        } else {
            ScratchStatus::Exited
        };
        Ok(done(status, diagnostics, Some(out)))
    }

    /// The shared target directory (locked) and a fresh work directory for one run.
    async fn prepare(&self, cache_key: &str) -> Result<(PathBuf, File, tempfile::TempDir), RunnerError> {
        if cache_key.is_empty() || !cache_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(RunnerError::BadCacheKey(cache_key.to_owned()));
        }
        let cfg = &self.config;
        let runs = cfg.work_root.join("runs");
        let target = cfg.work_root.join("target").join(cfg.sandbox.dir_name()).join(cache_key);
        std::fs::create_dir_all(&runs).map_err(|e| RunnerError::io("create work root", e))?;
        std::fs::create_dir_all(&target).map_err(|e| RunnerError::io("create target dir", e))?;
        let lock = lock_target(&target).await?;
        let work = tempfile::Builder::new().prefix("run-").tempdir_in(&runs).map_err(|e| RunnerError::io("create run dir", e))?;
        Ok((target, lock, work))
    }
}

async fn lock_target(target: &Path) -> Result<File, RunnerError> {
    let path = target.join(".anneal-lock");
    tokio::task::spawn_blocking(move || {
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)?;
        file.lock()?;
        Ok(file)
    })
    .await
    .map_err(|e| RunnerError::io("lock target dir", std::io::Error::other(e)))?
    .map_err(|e| RunnerError::io("lock target dir", e))
}

fn finish(
    status: RunStatus,
    diagnostics: Vec<Diagnostic>,
    tests: Vec<TestOutcome>,
    start: Instant,
) -> RunResult {
    let passed = tests
        .iter()
        .filter(|t| t.outcome == Outcome::Passed)
        .count();
    let total = tests.len();
    RunResult {
        status,
        diagnostics,
        tests,
        passed,
        total,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Default location for run directories and build caches.
pub fn default_work_root() -> PathBuf {
    std::env::var_os("ANNEAL_WORK_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("anneal"))
}
