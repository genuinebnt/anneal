//! `anneal course`: a CodeCrafters-style course (docs/BUSTUB.md). The learner works in a normal Cargo repo,
//! `anneal course test` runs the current stage's tests, and a git `pre-push` hook records the result.
//!
//! Authoring side: `courses/<id>/reference/` is the whole repo with the solution written out and each part
//! a learner writes marked `// @begin <stage>` ... `// @end`, with the stub in `//~` lines. `build` renders the repo as it
//! is after any stage; `verify` checks that every stage's tests fail before it and pass after it.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use clap::Subcommand;

use crate::course_sync;
use crate::course_unlock;
use crate::{render, term};
use serde::{Deserialize, Serialize};

#[derive(Subcommand)]
pub enum CourseCmd {
    /// Create a repo for a course: copies the template, runs `git init`, installs the hooks.
    Init {
        /// Course id, e.g. bustub.
        course: String,
        /// Where to create the repo (default: the course's repo name in the current directory).
        dir: Option<PathBuf>,
        /// The directory holding the courses.
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
    },
    /// Sign in to the anneal web app, so runs are reported there and its stage pages show your progress.
    Login {
        /// The app's address, e.g. https://anneal.genuinebasil.dev (or http://127.0.0.1:8787 locally).
        url: String,
    },
    /// Authoring: check the course against docs/COURSE_STANDARDS.md (learn lines, concepts, a Performance section, hints, a short Tests summary).
    Lint {
        #[arg(long, default_value = "bustub")]
        course: String,
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
        /// Check every module, not only the published ones.
        #[arg(long)]
        all: bool,
    },
    /// Authoring: upload each stage's solution diff (from reference/) to the web app you signed in to.
    Solutions {
        #[arg(long, default_value = "bustub")]
        course: String,
        /// The directory holding the courses.
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
        /// Write the diffs to this JSON file instead of uploading them.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Only these stage ids.
        stages: Vec<String>,
    },
    /// Put our solution of a stage into your repo (from the web app; the stage is then marked as helped). Parts that do not fit your code are saved for you to merge by hand.
    Adopt {
        /// A stage id, e.g. 1a-03 (default: the current stage).
        stage: Option<String>,
        /// Do not ask for confirmation.
        #[arg(long, short)]
        yes: bool,
    },
    /// Forget your progress for one module or the whole course (your code is not touched). The web app's progress is reset too when you are signed in.
    Reset {
        /// A module code, e.g. 1a.
        #[arg(long, conflicts_with = "all")]
        module: Option<String>,
        /// The whole course.
        #[arg(long)]
        all: bool,
        /// Do not ask for confirmation.
        #[arg(long, short)]
        yes: bool,
    },
    /// Where you are: the current module and stage, and what's done.
    Status {
        /// Print the progress as JSON (for scripts and editors).
        #[arg(long)]
        json: bool,
    },
    /// Print a stage's README (default: the current stage), formatted for the terminal and paged on a terminal.
    Show {
        /// A stage id, e.g. 1a-03.
        stage: Option<String>,
        /// Open the Nth hint of the stage. The website records that the stage was helped, so this needs `anneal course login`.
        #[arg(long, value_name = "N")]
        hint: Option<usize>,
        /// Print everything at once instead of through a pager (the `PAGER` environment variable picks the pager).
        #[arg(long)]
        no_pager: bool,
    },
    /// Run a stage's tests (default: the current stage, then the earlier stages as a regression check).
    Test {
        /// A stage id, e.g. 1a-03.
        stage: Option<String>,
        /// Run every passed stage's tests too (the default for the current stage) and nothing is skipped.
        #[arg(long)]
        all: bool,
        /// Only this stage's tests: skip the regression run of the stages you already passed.
        #[arg(long)]
        only: bool,
        /// Show every failure in full (the default shows each distinct failure once, cleaned of thread ids and backtrace notes).
        #[arg(short, long)]
        verbose: bool,
        /// Run only the tests whose name contains this text (in the test files of the stage).
        #[arg(short, long)]
        filter: Option<String>,
        /// Run again whenever a file under src/ or tests/ changes, until the stage passes (Ctrl-C stops).
        #[arg(short, long)]
        watch: bool,
    },
    /// Show the next stage's README. Run after the current stage passes.
    Next,
    /// List the test cases a stage must pass (default: the current stage), with how your last run went.
    Tests {
        /// The stage id, e.g. 1a-01.
        stage: Option<String>,
    },
    /// The one-screen guide: the loop of reading, testing and moving on, and the commands around it.
    Guide,
    /// Check the tools and the setup a learner needs: rust, git, curl, the hook, the login, the queue, the build directory.
    Doctor,
    /// Send the test runs that could not be reported earlier (no network, an expired session).
    Sync,
    /// Install (or reinstall) the git hooks in this repo.
    Hooks,
    /// Link this repo to your own GitHub repository (the `origin` remote) and tell the app, so `anneal course restore` can fetch it on another laptop.
    Remote {
        /// The repository's URL, e.g. git@github.com:you/bustub-rs.git. Without it, the current `origin` is recorded.
        url: Option<String>,
    },
    /// Continue on another laptop: clone your repo (committed work only), install the hooks and show where you are.
    Restore {
        /// The repository's URL. Without it, the one recorded by `anneal course remote` is asked from the app you signed in to.
        url: Option<String>,
        /// Where to clone it. Default: the repository's name.
        dir: Option<PathBuf>,
        #[arg(long, default_value = "bustub")]
        course: String,
    },
    /// Bring in the stages, tests and stubs added since `init` (new modules), without touching your code.
    Update {
        #[arg(long, default_value = "bustub")]
        course: String,
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
        /// The app to fetch the newest course files from. Default: the one you signed in to.
        #[arg(long)]
        from: Option<String>,
        /// Use only the course files built into this anneal; do not ask the app.
        #[arg(long)]
        offline: bool,
    },
    /// What a git hook runs. `anneal course hook pre-push`.
    Hook { name: String },
    /// Authoring: render the repo as it is after a stage into a directory.
    Build {
        /// Render after this stage. Without it: the template (before stage 1).
        #[arg(long, conflicts_with = "full")]
        stage: Option<String>,
        /// Render the whole solution (all stages done).
        #[arg(long)]
        full: bool,
        /// Output directory (created; existing files are overwritten).
        out: PathBuf,
        #[arg(long, default_value = "bustub")]
        course: String,
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
    },
    /// Authoring: check every stage's tests fail before the stage and pass after it.
    Verify {
        #[arg(long, default_value = "bustub")]
        course: String,
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
        /// Only this stage (and the regression of the stages before it); `unlock` checks only that every module's unlock state compiles.
        #[arg(long)]
        stage: Option<String>,
        /// Start at this stage (earlier stages are assumed fine).
        #[arg(long)]
        from: Option<String>,
        /// Scratch directory for the rendered repo. Reused between runs so cargo's cache stays warm.
        #[arg(long)]
        work: Option<PathBuf>,
    },
    /// Authoring: regenerate courses/<id>/template from the reference (the repo learners start from).
    Template {
        #[arg(long, default_value = "bustub")]
        course: String,
        #[arg(long, default_value = "courses")]
        courses: PathBuf,
    },
}

// ---------------------------------------------------------------------------------------------------------------
// Course definition files

#[derive(Debug, Deserialize)]
struct CourseToml {
    id: String,
    title: String,
    repo_name: String,
    /// Modules still to be (re)written: a learner's copy never contains their stages or their code.
    #[serde(default)]
    planned_modules: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModuleToml {
    code: String,
    title: String,
    /// Not on the main path: the next stage is never picked from it (the learner opens it on purpose).
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    summary: String,
    /// Ids from `lectures.toml`: watch or read these before the module.
    #[serde(default)]
    lectures: Vec<String>,
    /// BusTub's own files this module ports.
    #[serde(default)]
    bustub: Vec<String>,
    /// Everything else worth reading: docs, books, papers, blogs, videos, other projects' code.
    #[serde(default)]
    resources: Vec<Resource>,
    /// Files that must arrive with this module although a later module's stage also touches them (see course_unlock.rs).
    #[serde(default)]
    files: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    /// docs · book · paper · blog · video · code · man
    kind: String,
    title: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct LecturesToml {
    #[serde(default)]
    lecture: Vec<Lecture>,
}

#[derive(Debug, Clone, Deserialize)]
struct Lecture {
    id: String,
    title: String,
    term: String,
    slides: String,
    notes: Option<String>,
    video: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StageToml {
    id: String,
    title: String,
    /// learn · build · boss
    kind: String,
    /// very-easy · easy · medium · hard, as CodeCrafters labels its stages.
    difficulty: String,
    /// `bin` or `bin::filter`: a test binary, optionally only the tests whose name contains `filter`.
    tests: Vec<String>,
    /// A boss stage may pass before it starts: it is a milestone, not new code.
    #[serde(default)]
    retest: bool,
}

#[derive(Debug)]
struct Stage {
    def: StageToml,
    module: String,
    /// The stage's module is optional: it is never the "next" stage.
    optional: bool,
    module_title: String,
    /// Lectures, reading and BusTub files of the stage's module, for the stage page.
    resources: String,
    readme: String,
    /// 1-based position in the whole course.
    rank: usize,
}

#[derive(Debug)]
struct Course {
    meta: CourseToml,
    stages: Vec<Stage>,
    /// module code -> the `files` its module.toml lists.
    module_files: BTreeMap<String, Vec<String>>,
}

impl Course {
    fn load(root: &Path) -> anyhow::Result<Course> {
        let meta: CourseToml = read_toml(&root.join("course.toml"))?;
        let mut stages = Vec::new();
        let mut module_files = BTreeMap::new();
        let lectures: Vec<Lecture> = match fs::read_to_string(root.join("lectures.toml")) {
            Ok(t) => toml::from_str::<LecturesToml>(&t).context("parsing lectures.toml")?.lecture,
            Err(_) => Vec::new(),
        };
        let modules = sorted_dirs(&root.join("modules"))?;
        for m in modules {
            let mt: ModuleToml = read_toml(&m.join("module.toml"))?;
            let mut resources = String::new();
            for id in &mt.lectures {
                let l = lectures.iter().find(|l| &l.id == id).with_context(|| format!("module {}: no lecture {id:?} in lectures.toml", mt.code))?;
                resources.push_str(&format!("  Lecture  {} (CMU 15-445, {})\n    slides  {}\n", l.title, l.term, l.slides));
                if let Some(n) = &l.notes {
                    resources.push_str(&format!("    notes   {n}\n"));
                }
                if let Some(v) = &l.video {
                    resources.push_str(&format!("    video   {v}\n"));
                }
            }
            for f in &mt.bustub {
                resources.push_str(&format!("  BusTub   https://github.com/cmu-db/bustub/blob/master/{f}\n"));
            }
            for kind in ["docs", "book", "man", "paper", "blog", "video", "code"] {
                for r in mt.resources.iter().filter(|r| r.kind == kind) {
                    resources.push_str(&format!("  {:<8} {}\n    {}\n", kind, r.title, r.url));
                }
            }
            for s in sorted_dirs(&m.join("stages"))? {
                let def: StageToml = read_toml(&s.join("stage.toml"))?;
                let readme = fs::read_to_string(s.join("stage.md"))
                    .with_context(|| format!("reading {}", s.join("stage.md").display()))?;
                if !matches!(def.kind.as_str(), "learn" | "build" | "boss" | "challenge") {
                    bail!("{}: kind must be learn, build, boss or challenge", def.id);
                }
                if !matches!(def.difficulty.as_str(), "very-easy" | "easy" | "medium" | "hard") {
                    bail!("{}: difficulty must be very-easy, easy, medium or hard", def.id);
                }
                if def.tests.is_empty() {
                    bail!("{}: a stage needs at least one test entry", def.id);
                }
                // a challenge is never on the main path: it does not block the next stage or module
                let optional = mt.optional || def.kind == "challenge";
                stages.push(Stage {
                    def,
                    module: mt.code.clone(),
                    optional,
                    module_title: mt.title.clone(),
                    resources: resources.clone(),
                    readme,
                    rank: 0,
                });
            }
            module_files.insert(mt.code.clone(), mt.files.clone());
            let _ = &mt.summary;
        }
        for (i, s) in stages.iter_mut().enumerate() {
            s.rank = i + 1;
        }
        let mut seen = HashMap::new();
        for s in &stages {
            if let Some(prev) = seen.insert(s.def.id.clone(), s.rank) {
                bail!("stage id {} is used twice (positions {prev} and {})", s.def.id, s.rank);
            }
        }
        Ok(Course { meta, stages, module_files })
    }

    fn stage(&self, id: &str) -> anyhow::Result<&Stage> {
        self.stages
            .iter()
            .find(|s| s.def.id == id)
            .with_context(|| format!("no stage {id:?}; `anneal course status` lists them"))
    }

    fn ranks(&self) -> HashMap<String, usize> {
        self.stages.iter().map(|s| (s.def.id.clone(), s.rank)).collect()
    }
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn sorted_dirs(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    Ok(v)
}

// ---------------------------------------------------------------------------------------------------------------
// Rendering the reference: @begin / @end regions

/// Renders one file for the state "stages up to `cutoff` are done".
///
/// A region is `// @begin <stage>` … `// @end`. Inside it, plain lines are the solution and `//~ ` lines are the stub that stands in
/// for it. A done region keeps its solution lines; a region that isn't done keeps its stub lines (with the `//~` removed).
/// Regions may nest (a stage that adds three lines inside a function another stage wrote): when the outer region isn't done,
/// everything inside it, nested regions included, is replaced by the outer stub.
fn render_text(text: &str, ranks: &HashMap<String, usize>, cutoff: usize, name: &str) -> anyhow::Result<String> {
    struct Frame {
        done: bool,
        id: String,
        at: usize,
    }
    let mut out = String::with_capacity(text.len());
    let mut stack: Vec<Frame> = Vec::new();
    for (n, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_start();
        let marker = body.strip_prefix("//").or_else(|| body.strip_prefix('#')).map(|r| r.trim_start());
        if let Some(m) = marker {
            if let Some(id) = m.strip_prefix("@begin ") {
                let id = id.trim();
                let rank = *ranks.get(id).with_context(|| format!("{name}:{}: unknown stage {id:?}", n + 1))?;
                stack.push(Frame { done: rank <= cutoff, id: id.to_owned(), at: n + 1 });
                continue;
            }
            if m.trim_end() == "@end" {
                if stack.pop().is_none() {
                    bail!("{name}:{}: @end without @begin", n + 1);
                }
                continue;
            }
        }
        // Which frame decides what this line becomes: the outermost region that isn't done, else the innermost one.
        let Some(top) = stack.last() else {
            out.push_str(line);
            continue;
        };
        let first_undone = stack.iter().position(|f| !f.done);
        let indent = &line[..line.len() - body.len()];
        let stub = body.strip_prefix("//~").or_else(|| body.strip_prefix("#~")).map(|r| r.strip_prefix(' ').unwrap_or(r));
        match first_undone {
            // Everything in the stack is done: the solution lines of the innermost region.
            None => {
                if stub.is_none() {
                    out.push_str(line);
                }
            }
            // The innermost region is the first undone one: its stub lines. Deeper than that, the line is replaced.
            Some(i) if i == stack.len() - 1 => {
                if let Some(s) = stub {
                    out.push_str(indent);
                    out.push_str(s);
                }
            }
            Some(_) => {}
        }
        let _ = top;
    }
    if let Some(f) = stack.last() {
        bail!("{name}:{}: region {} is never closed", f.at, f.id);
    }
    Ok(out)
}

/// Writes the rendered repo into `out`, touching only files whose contents changed (so cargo's cache survives).
fn render_tree(reference: &Path, out: &Path, ranks: &HashMap<String, usize>, cutoff: usize) -> anyhow::Result<()> {
    fn walk(base: &Path, dir: &Path, out: &Path, ranks: &HashMap<String, usize>, cutoff: usize, keep: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        for e in fs::read_dir(dir)? {
            let e = e?;
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), "target" | ".git" | ".DS_Store") {
                continue;
            }
            let rel = path.strip_prefix(base)?.to_path_buf();
            if path.is_dir() {
                walk(base, &path, out, ranks, cutoff, keep)?;
                continue;
            }
            let bytes = fs::read(&path)?;
            let data = match String::from_utf8(bytes.clone()) {
                Ok(text) if text.contains("@begin ") => render_text(&text, ranks, cutoff, &rel.display().to_string())?.into_bytes(),
                _ => bytes,
            };
            let dest = out.join(&rel);
            fs::create_dir_all(dest.parent().unwrap())?;
            if fs::read(&dest).map(|old| old != data).unwrap_or(true) {
                fs::write(&dest, &data)?;
            }
            keep.push(rel);
        }
        Ok(())
    }
    let mut keep = Vec::new();
    walk(reference, reference, out, ranks, cutoff, &mut keep)?;
    // Remove files that no longer exist in the reference (a renamed file), but never target/.git/.anneal.
    fn prune(base: &Path, dir: &Path, keep: &[PathBuf]) -> anyhow::Result<()> {
        for e in fs::read_dir(dir)? {
            let e = e?;
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), "target" | ".git" | ".anneal" | "Cargo.lock") {
                continue;
            }
            if path.is_dir() {
                prune(base, &path, keep)?;
            } else if !keep.contains(&path.strip_prefix(base)?.to_path_buf()) {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
    if out.exists() {
        prune(out, out, &keep)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Running tests

#[derive(Debug, Clone)]
struct TestOutcome {
    name: String,
    ok: bool,
    /// The failure message (first lines of the test's stdout), if it failed.
    detail: String,
}

#[derive(Debug, Default)]
struct RunReport {
    tests: Vec<TestOutcome>,
    /// Compiler errors or a timeout, when no test ran.
    problem: Option<String>,
}

impl RunReport {
    fn passed(&self) -> bool {
        self.problem.is_none() && !self.tests.is_empty() && self.tests.iter().all(|t| t.ok)
    }
}

fn split_entry(entry: &str) -> (&str, Option<&str>) {
    match entry.split_once("::") {
        Some((bin, filter)) => (bin, Some(filter)),
        None => (entry, None),
    }
}

/// Runs the entries (`bin` or `bin::filter`) of several stages: one `cargo test` per binary, with all its filters.
fn run_entries(repo: &Path, entries: &[String], target_dir: Option<&Path>, timeout: Duration) -> RunReport {
    let mut by_bin: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut whole: Vec<&str> = Vec::new();
    for e in entries {
        let (bin, filter) = split_entry(e);
        match filter {
            Some(f) => by_bin.entry(bin).or_default().push(f),
            None => whole.push(bin),
        }
    }
    let bins: Vec<&str> = by_bin.keys().copied().chain(whole.iter().copied()).collect();
    // Build every needed test binary with ONE cargo call, then run them side by side: a regression over fifty binaries used to be fifty
    // `cargo test` calls one after the other.
    let mut build = Command::new("cargo");
    build.current_dir(repo).args(["test", "--no-run", "--message-format=json-render-diagnostics"]);
    for bin in &bins {
        build.args(["--test", bin]);
    }
    build.env("RUST_BACKTRACE", "0").env("CARGO_TERM_COLOR", "never");
    if let Some(t) = target_dir {
        build.env("CARGO_TARGET_DIR", t);
    }
    let mut report = RunReport::default();
    let (stdout, stderr, code) = match crate::term::with_spinner("compiling", || run_with_timeout(build, timeout.max(Duration::from_secs(600)))) {
        Err(e) => {
            report.problem = Some(format!("{}: {e}", bins.first().copied().unwrap_or("build")));
            return report;
        }
        Ok(r) => r,
    };
    let mut exes: HashMap<String, PathBuf> = HashMap::new();
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["reason"] == "compiler-artifact"
            && v["target"]["kind"].as_array().is_some_and(|k| k.iter().any(|x| x == "test"))
            && let (Some(name), Some(exe)) = (v["target"]["name"].as_str(), v["executable"].as_str())
        {
            exes.insert(name.to_owned(), PathBuf::from(exe));
        }
    }
    if code != Some(0) {
        let tail: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
        let shown = if tail.len() > 40 { &tail[..40] } else { &tail[..] };
        report.problem = Some(format!("{}: the tests didn't compile or run\n{}", bins.first().copied().unwrap_or("build"), shown.join("\n")));
        return report;
    }
    let jobs: Vec<(&str, &[&str])> = bins.iter().map(|b| (*b, if whole.contains(b) { &[][..] } else { &by_bin[b][..] })).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<Option<RunOutput>>> = std::sync::Mutex::new((0..jobs.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..3 {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let Some((bin, filters)) = jobs.get(k) else { break };
                    let out = match exes.get(*bin) {
                        None => Err(anyhow::anyhow!("no test binary was built")),
                        Some(exe) => {
                            let mut cmd = Command::new(exe);
                            cmd.current_dir(repo).args(*filters);
                            cmd.env("RUST_BACKTRACE", "0").env("CARGO_MANIFEST_DIR", repo).env("CARGO_TERM_COLOR", "never");
                            run_until(cmd, timeout)
                        }
                    };
                    results.lock().unwrap()[k] = Some(out);
                }
            });
        }
    });
    for ((bin, filters), result) in jobs.iter().zip(results.into_inner().unwrap()) {
        match result.expect("every job ran") {
            Err(e) => {
                report.problem = Some(format!("{bin}: {e}"));
                return report;
            }
            Ok(ran) => {
                let before = report.tests.len();
                parse_tests(&ran.stdout, &mut report.tests);
                if ran.timed_out {
                    // keep the tests that finished; those that did not are failures that say why
                    let wanted = test_fn_names(repo, bin, filters);
                    if mark_unfinished(&mut report.tests, before, wanted, timeout.as_secs()) == 0 {
                        report.problem = Some(format!("{bin}: Time limit exceeded: stopped after {}s (a deadlock or an endless loop?)", timeout.as_secs()));
                        return report;
                    }
                    continue;
                }
                let (stderr, code) = (ran.stderr, ran.code);
                if report.tests.len() == before {
                    let tail: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
                    let shown = if tail.len() > 40 { &tail[..40] } else { &tail[..] };
                    report.problem = Some(if code == Some(0) {
                        format!("{bin}: no test matched {filters:?}")
                    } else {
                        format!("{bin}: the tests didn't compile or run\n{}", shown.join("\n"))
                    });
                    return report;
                }
            }
        }
    }
    report
}

/// What one test binary printed: stdout, stderr and the exit code, or why it could not run.
type RunOutput = anyhow::Result<Ran>;

/// What a process printed and how it ended.
struct Ran {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    /// It was stopped for running past its time limit. What it had printed by then is in `stdout`: libtest prints a line as each test
    /// finishes, so the tests that did finish are all there.
    timed_out: bool,
}

/// Runs `cmd`, killing it after `timeout`, and returns what it printed either way.
fn run_until(mut cmd: Command, timeout: Duration) -> anyhow::Result<Ran> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("starting cargo")?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let t_out = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut s);
        String::from_utf8_lossy(&s).into_owned()
    });
    let t_err = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err, &mut s);
        String::from_utf8_lossy(&s).into_owned()
    });
    let start = Instant::now();
    let (status, timed_out) = loop {
        if let Some(st) = child.try_wait()? {
            break (Some(st), false);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    Ok(Ran { stdout: t_out.join().unwrap_or_default(), stderr: t_err.join().unwrap_or_default(), code: status.and_then(|s| s.code()), timed_out })
}

/// Like `run_until`, but a process that runs past its limit is an error (a build, a `cargo` call whose output is useless half done).
fn run_with_timeout(cmd: Command, timeout: Duration) -> anyhow::Result<(String, String, Option<i32>)> {
    let ran = run_until(cmd, timeout)?;
    if ran.timed_out {
        bail!("Time limit exceeded: stopped after {}s (a deadlock or an endless loop?)", timeout.as_secs());
    }
    Ok((ran.stdout, ran.stderr, ran.code))
}

/// The tests that were meant to run and gave no result when the binary was stopped: each one is a failure that says so. Returns how
/// many there are. `done` are the outcomes already read from the output (from index `from` of `tests`).
fn mark_unfinished(tests: &mut Vec<TestOutcome>, from: usize, wanted: Vec<String>, secs: u64) -> usize {
    let finished: std::collections::HashSet<String> = tests[from..].iter().map(|t| t.name.rsplit("::").next().unwrap_or(&t.name).to_owned()).collect();
    let detail = format!("Time limit exceeded: the run was stopped after {secs} s and this test had not finished (an endless loop? a latch that is never released?)");
    let mut n = 0;
    for name in wanted {
        if !finished.contains(&name) {
            tests.push(TestOutcome { name, ok: false, detail: detail.clone() });
            n += 1;
        }
    }
    n
}

/// Reads libtest's output: `test a::b ... ok` lines, and the `---- a::b stdout ----` blocks of the failures.
fn parse_tests(stdout: &str, into: &mut Vec<TestOutcome>) {
    let mut details: HashMap<String, String> = HashMap::new();
    let mut current: Option<String> = None;
    let mut buf: Vec<&str> = Vec::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("---- ").and_then(|r| r.strip_suffix(" stdout ----")) {
            if let Some(c) = current.take() {
                details.insert(c, buf.join("\n"));
            }
            buf.clear();
            current = Some(rest.to_owned());
        } else if line.starts_with("failures:") || line.starts_with("test result:") {
            if let Some(c) = current.take() {
                details.insert(c, buf.join("\n"));
            }
            buf.clear();
        } else if current.is_some() {
            buf.push(line);
        }
    }
    if let Some(c) = current.take() {
        details.insert(c, buf.join("\n"));
    }
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("test ") else { continue };
        if let Some(name) = rest.strip_suffix(" ... ok") {
            into.push(TestOutcome { name: name.to_owned(), ok: true, detail: String::new() });
        } else if let Some(name) = rest.strip_suffix(" ... FAILED") {
            let detail = details.get(name).cloned().unwrap_or_default();
            into.push(TestOutcome { name: name.to_owned(), ok: false, detail });
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The learner's repo: state, hooks, commands

const STATE_DIR: &str = ".anneal";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Progress {
    /// stage id -> when it first passed.
    passed: BTreeMap<String, Passed>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Passed {
    at: u64,
    commit: String,
    ms: u64,
}

#[derive(Debug, Deserialize)]
struct LocalConfig {
    #[allow(dead_code)]
    course: String,
    /// Fail the push when the current stage's tests fail.
    #[serde(default)]
    block_on_fail: bool,
}

fn state_path(repo: &Path, name: &str) -> PathBuf {
    repo.join(STATE_DIR).join(name)
}

fn find_repo() -> anyhow::Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join(STATE_DIR).join("course.toml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("not inside a course repo (no {STATE_DIR}/course.toml); create one with `anneal course init bustub`");
        }
    }
}

fn load_progress(repo: &Path) -> Progress {
    fs::read_to_string(state_path(repo, "progress.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_progress(repo: &Path, p: &Progress) -> anyhow::Result<()> {
    fs::write(state_path(repo, "progress.json"), serde_json::to_string_pretty(p)? + "\n")?;
    Ok(())
}

fn learner_course(repo: &Path) -> anyhow::Result<Course> {
    Course::load(&repo.join(STATE_DIR).join("course"))
}

fn current<'a>(course: &'a Course, progress: &Progress) -> Option<&'a Stage> {
    course.stages.iter().find(|s| !s.optional && !progress.passed.contains_key(&s.def.id))
}

fn head_commit(repo: &Path) -> String {
    Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "uncommitted".into())
}

pub fn run(cmd: CourseCmd) -> anyhow::Result<ExitCode> {
    match cmd {
        CourseCmd::Init { course, dir, courses } => init(&course, dir, &courses),
        CourseCmd::Login { url } => {
            course_sync::login(&url)?;
            Ok(ExitCode::SUCCESS)
        }
        CourseCmd::Lint { course, courses, all } => lint(&course, &courses, all),
        CourseCmd::Solutions { course, courses, out, stages } => solutions(&course, &courses, out, &stages),
        CourseCmd::Adopt { stage, yes } => adopt(stage.as_deref(), yes),
        CourseCmd::Reset { module, all, yes } => reset(module.as_deref(), all, yes),
        CourseCmd::Status { json } => {
            let done = status(json);
            if !json {
                crate::update::hint("bustub");
            }
            done
        }
        CourseCmd::Show { stage, hint, no_pager } => show(stage.as_deref(), hint, no_pager),
        CourseCmd::Test { stage, all, only, verbose, filter, watch } => {
            let done = test_command(stage.as_deref(), TestOpts { all, only, verbose, hook: false }, filter.as_deref(), watch);
            if !watch {
                crate::update::hint("bustub");
            }
            done
        }
        CourseCmd::Next => next(),
        CourseCmd::Tests { stage } => tests_command(stage.as_deref()),
        CourseCmd::Guide => {
            println!("{}", crate::ui::guide());
            Ok(ExitCode::SUCCESS)
        }
        CourseCmd::Doctor => doctor(),
        CourseCmd::Sync => {
            course_sync::sync(&find_repo()?)?;
            Ok(ExitCode::SUCCESS)
        }
        CourseCmd::Update { course, courses, from, offline } => update(&course, &courses, from.as_deref(), offline),
        CourseCmd::Hooks => {
            install_hooks(&find_repo()?)?;
            println!("hooks installed");
            Ok(ExitCode::SUCCESS)
        }
        CourseCmd::Remote { url } => remote(url),
        CourseCmd::Restore { url, dir, course } => restore(url, dir, &course),
        CourseCmd::Hook { name } => match name.as_str() {
            "pre-push" => test(None, TestOpts { all: false, only: false, verbose: false, hook: true }, None),
            other => bail!("no hook {other:?}"),
        },
        CourseCmd::Build { stage, full, out, course, courses } => {
            let root = courses.join(&course);
            let c = Course::load(&root)?;
            let cutoff = if full {
                usize::MAX
            } else if let Some(s) = stage {
                c.stage(&s)?.rank
            } else {
                0
            };
            render_tree(&root.join("reference"), &out, &c.ranks(), cutoff)?;
            println!("rendered {} into {}", if full { "the full solution".to_owned() } else { format!("stage cutoff {cutoff}") }, out.display());
            Ok(ExitCode::SUCCESS)
        }
        CourseCmd::Template { course, courses } => {
            let root = courses.join(&course);
            let c = Course::load(&root)?;
            let out = root.join("template");
            render_tree(&root.join("reference"), &out, &c.ranks(), 0)?;
            let map = files_map_for(&c, &root.join("reference"))?;
            fs::write(out.join(course_unlock::FILES_JSON), serde_json::to_string_pretty(&map)? + "\n")?;
            println!("template written to {} ({} files tied to a module)", out.display(), map.len());
            Ok(ExitCode::SUCCESS)
        }
        CourseCmd::Verify { course, courses, stage, from, work } => verify(&course, &courses, stage.as_deref(), from.as_deref(), work),
    }
}

/// Copies the course definition into a learner's `.anneal/course`, leaving out the planned modules (their stages will change).
fn copy_definition(root: &Path, defs: &Path, planned: &[String]) -> anyhow::Result<()> {
    fs::create_dir_all(defs)?;
    fs::copy(root.join("course.toml"), defs.join("course.toml"))?;
    if root.join("lectures.toml").exists() {
        fs::copy(root.join("lectures.toml"), defs.join("lectures.toml"))?;
    }
    copy_dir(&root.join("modules"), &defs.join("modules"))?;
    for dir in sorted_dirs(&defs.join("modules"))? {
        let module: ModuleToml = read_toml(&dir.join("module.toml"))?;
        if planned.contains(&module.code) {
            fs::remove_dir_all(&dir)?;
        }
    }
    Ok(())
}

/// Copies the template into a learner's `.anneal/template` without the files that belong to planned modules, so their code never
/// reaches the learner's machine, not even in the hidden copy.
fn copy_template(template: &Path, kept: &Path, planned: &[String]) -> anyhow::Result<()> {
    copy_dir(template, kept)?;
    let map = load_files_map(kept);
    let mut left = map.clone();
    for (file, module) in &map {
        if planned.contains(module) {
            let _ = fs::remove_file(kept.join(file));
            left.remove(file);
        }
    }
    if left.len() != map.len() {
        fs::write(kept.join(course_unlock::FILES_JSON), serde_json::to_string_pretty(&left)?)?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if matches!(name.to_string_lossy().as_ref(), "target" | ".git" | ".DS_Store") {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Which module first needs each reference file (see course_unlock.rs).
fn files_map_for(course: &Course, reference: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let stage_module: HashMap<String, String> = course.stages.iter().map(|s| (s.def.id.clone(), s.module.clone())).collect();
    let mut boss_bins: HashMap<String, String> = HashMap::new();
    for s in course.stages.iter().filter(|s| s.def.kind == "boss") {
        for t in &s.def.tests {
            boss_bins.insert(split_entry(t).0.to_owned(), s.module.clone());
        }
    }
    course_unlock::file_modules(reference, &module_order(course), &stage_module, &course.module_files, &boss_bins)
}

// ---------------------------------------------------------------------------------------------------------------
// Progressive reveal (course_unlock.rs): the learner's repo shows only the modules reached so far.

fn module_order(course: &Course) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for s in &course.stages {
        if v.last() != Some(&s.module) {
            v.push(s.module.clone());
        }
    }
    v
}

/// The modules whose files the learner sees: every module up to and including the first one with an unpassed stage.
fn unlocked_modules(course: &Course, progress: &Progress) -> Vec<String> {
    let order = module_order(course);
    let current = current(course, progress).map(|s| s.module.clone());
    match current.and_then(|c| order.iter().position(|m| *m == c)) {
        Some(i) => order[..=i].to_vec(),
        None => order,
    }
}

fn load_files_map(template: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(template.join(course_unlock::FILES_JSON)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

#[derive(Debug, Default)]
struct Sync {
    added: usize,
    updated: usize,
    conflicts: Vec<PathBuf>,
}

/// Brings the visible template files into `repo`: new files are created, files you haven't edited are refreshed, files you have
/// edited are never overwritten (the new version goes next to yours as `.new`). `base` remembers what was handed out.
fn sync_files(repo: &Path, template: &Path, unlocked: &[String]) -> anyhow::Result<Sync> {
    let map = load_files_map(template);
    let files: Vec<PathBuf> = list_files(template)?;
    let vis = course_unlock::visible(&files, &map, unlocked, &|rel| fs::read_to_string(template.join(rel)).map(|t| course_unlock::only_declares(&t)).unwrap_or(true));
    let base = repo.join(STATE_DIR).join("base");
    let mut report = Sync::default();
    for rel in &vis {
        let raw = fs::read(template.join(rel))?;
        let new = if course_unlock::is_module_file(rel) {
            let text = String::from_utf8_lossy(&raw).into_owned();
            course_unlock::filter_mod(&text, rel.parent().unwrap_or(Path::new("")), &vis).into_bytes()
        } else {
            raw
        };
        let cur_path = repo.join(rel);
        let cur = fs::read(&cur_path).ok();
        let old = fs::read(base.join(rel)).ok();
        let mut hand_out = true;
        match (&cur, &old) {
            (None, _) => {
                fs::create_dir_all(cur_path.parent().unwrap())?;
                fs::write(&cur_path, &new)?;
                report.added += 1;
            }
            (Some(c), _) if *c == new => {}
            (Some(c), Some(o)) if c == o => {
                fs::write(&cur_path, &new)?;
                report.updated += 1;
            }
            (Some(_), Some(o)) if *o == new => {} // the template didn't change; the edit is yours
            (Some(_), _) => {
                let mut name = cur_path.clone().into_os_string();
                name.push(".new");
                fs::write(&name, &new)?;
                report.conflicts.push(rel.clone());
                hand_out = false;
            }
        }
        if hand_out {
            let b = base.join(rel);
            fs::create_dir_all(b.parent().unwrap())?;
            fs::write(b, &new)?;
        }
    }
    Ok(report)
}

/// After progress changed: brings in the files of any module that has just been reached.
fn maybe_unlock(repo: &Path, course: &Course, progress: &Progress) -> anyhow::Result<()> {
    let template = repo.join(STATE_DIR).join("template");
    if !template.is_dir() {
        return Ok(()); // a repo made before modules were revealed progressively: everything is already there
    }
    let unlocked = unlocked_modules(course, progress);
    let r = sync_files(repo, &template, &unlocked)?;
    if r.added > 0 {
        let latest = unlocked.last().cloned().unwrap_or_default();
        println!("\nModule {} unlocked: {} new files (stubs, and the tests for the stages in it).", latest.to_uppercase(), r.added);
    }
    for c in &r.conflicts {
        println!("  you edited {}: the new version is in {}.new (merge by hand)", c.display(), c.display());
    }
    Ok(())
}

/// Where the course files are: `courses/<id>` here, else the checkout this binary was built from, else the courses compiled into the
/// binary, so `anneal course init bustub` works from any directory after `cargo install --git ... anneal-cli`.
fn course_root(courses: &Path, id: &str) -> PathBuf {
    let here = courses.join(id);
    // ANNEAL_EMBEDDED_ONLY=1 skips the two directories (the smoke test uses it to check the compiled-in copy)
    let embedded_only = std::env::var_os("ANNEAL_EMBEDDED_ONLY").is_some();
    if !embedded_only && here.join("course.toml").exists() {
        return here;
    }
    let built_from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../courses").join(id);
    if !embedded_only && built_from.join("course.toml").exists() {
        return built_from;
    }
    // Neither: the courses compiled into this binary (a `cargo install --git` has no checkout).
    match crate::embedded::courses_dir() {
        Ok(Some(dir)) if dir.join(id).join("course.toml").exists() => dir.join(id),
        _ => here,
    }
}

fn init(course_id: &str, dir: Option<PathBuf>, courses: &Path) -> anyhow::Result<ExitCode> {
    let root = course_root(courses, course_id);
    let course = Course::load(&root)?;
    if course.meta.id != course_id {
        bail!("{} says its id is {:?}", root.join("course.toml").display(), course.meta.id);
    }
    let template = root.join("template");
    if !template.is_dir() {
        bail!("{} has no template; run `anneal course template --course {course_id}` first", root.display());
    }
    let dir = dir.unwrap_or_else(|| PathBuf::from(&course.meta.repo_name));
    if dir.exists() && fs::read_dir(&dir)?.next().is_some() {
        bail!("{} already exists and isn't empty", dir.display());
    }
    // The whole template stays in .anneal/template; only the modules reached so far are in the working tree.
    copy_template(&template, &dir.join(STATE_DIR).join("template"), &course.meta.planned_modules)?;
    sync_files(&dir, &dir.join(STATE_DIR).join("template"), &unlocked_modules(&course, &Progress::default()))?;
    // The learner's copy of the stage definitions, so the CLI works without the anneal checkout.
    copy_definition(&root, &dir.join(STATE_DIR).join("course"), &course.meta.planned_modules)?;
    fs::write(
        state_path(&dir, "course.toml"),
        format!("course = \"{course_id}\"\n# Set to true to stop `git push` when the current stage's tests fail.\nblock_on_fail = false\n"),
    )?;
    save_progress(&dir, &Progress::default())?;
    let git = |args: &[&str]| Command::new("git").current_dir(&dir).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !dir.join(".git").exists() {
        git(&["init", "-q", "-b", "main"]).context("running git init")?;
    }
    install_hooks(&dir)?;
    git(&["add", "-A"])?;
    let committed = git(&["commit", "-q", "-m", "Start the course"]).map(|s| s.success()).unwrap_or(false);
    let shipped = learner_course(&dir)?;
    println!("Created {} ({} stages in {} modules).", dir.display(), shipped.stages.len(), count_modules(&shipped));
    if !committed {
        println!("(The first commit didn't happen: set git user.name and user.email, then `git commit -am start`.)");
    }
    println!("\n{}\n\n  {}\n", crate::ui::guide(), term::bold(&format!("cd {}  and start with: anneal course show", dir.display())));
    Ok(ExitCode::SUCCESS)
}

/// `anneal course remote [url]`: makes `origin` point at the learner's own repository and records it in the app.
fn remote(url: Option<String>) -> anyhow::Result<ExitCode> {
    let repo = find_repo()?;
    let course = learner_course(&repo)?.meta.id;
    let git = |args: &[&str]| Command::new("git").current_dir(&repo).args(args).output();
    let current = git(&["remote", "get-url", "origin"]).ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let url = match (url, &current) {
        (Some(u), _) => u,
        (None, Some(c)) => c.clone(),
        (None, None) => bail!("no remote yet: create an empty private repository on GitHub and run `anneal course remote git@github.com:YOU/{}.git`", repo.file_name().and_then(|n| n.to_str()).unwrap_or("bustub-rs")),
    };
    if url.chars().any(char::is_whitespace) {
        bail!("{url:?} is not a repository URL");
    }
    if current.as_deref() != Some(url.as_str()) {
        let verb = if current.is_some() { "set-url" } else { "add" };
        let out = git(&["remote", verb, "origin", &url])?;
        if !out.status.success() {
            bail!("git remote {verb} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
    }
    println!("origin is {url}");
    match course_sync::set_repo_remote(&course, &url) {
        None => println!("Not signed in, so the app doesn't know it yet: `anneal course login <app url>`, then run this again."),
        Some(Ok(())) => println!("The app remembers it: on another laptop, `anneal course login <app url>` and `anneal course restore`."),
        Some(Err(e)) => println!("The app wasn't told: {e}"),
    }
    println!("\nYour work reaches GitHub when you commit and push it:\n  git push -u origin HEAD     # the hook runs the current stage's tests and reports the result");
    Ok(ExitCode::SUCCESS)
}

/// `anneal course restore [url] [dir]`: your committed work on a new laptop.
fn restore(url: Option<String>, dir: Option<PathBuf>, course: &str) -> anyhow::Result<ExitCode> {
    let url = match url {
        Some(u) => u,
        None => match course_sync::repo_remote(course) {
            Some(Ok(Some(u))) => u,
            Some(Ok(None)) => bail!("the app has no repository for {course} yet: run `anneal course remote <url>` on the laptop that has your work, or pass the URL here"),
            Some(Err(e)) => bail!("asking the app for your repository failed: {e}"),
            None => bail!("pass the repository's URL, or sign in first (`anneal course login <app url>`) so the app can tell it"),
        },
    };
    let dir = dir.unwrap_or_else(|| {
        // "git@github.com:you/bustub-rs.git" -> "bustub-rs"
        let last = url.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("");
        let name = last.strip_suffix(".git").unwrap_or(last);
        PathBuf::from(if name.is_empty() { format!("{course}-rs") } else { name.to_owned() })
    });
    if dir.exists() && fs::read_dir(&dir)?.next().is_some() {
        bail!("{} already exists and isn't empty", dir.display());
    }
    println!("Cloning {url} into {} ...", dir.display());
    let status = Command::new("git").args(["clone", &url]).arg(&dir).status().context("running git clone (is git installed?)")?;
    if !status.success() {
        bail!("git clone failed: check the URL and that this laptop can reach the repository (an SSH key or `gh auth login`)");
    }
    if !dir.join(STATE_DIR).join("course.toml").exists() {
        bail!("{} has no {STATE_DIR}/course.toml: it isn't a repository made by `anneal course init`", dir.display());
    }
    install_hooks(&dir)?;
    let shipped = learner_course(&dir)?;
    let progress = load_progress(&dir);
    let done = shipped.stages.iter().filter(|s| progress.passed.contains_key(&s.def.id)).count();
    println!("\nRestored {}: {done} of {} stages passed in your committed work.", dir.display(), shipped.stages.len());
    if let Some(next) = current(&shipped, &progress) {
        println!("Next: {} {}", next.def.id, next.def.title);
    }
    println!("\n  cd {}\n  anneal course login <app url>   # so test runs are reported\n  anneal course test", dir.display());
    println!("(Anything you had not committed on the other laptop is not here. `anneal course update` brings in modules added since.)");
    Ok(ExitCode::SUCCESS)
}

/// Files of `dir` as relative paths.
fn list_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        for e in fs::read_dir(dir)? {
            let p = e?.path();
            if p.is_dir() {
                walk(base, &p, out)?;
            } else {
                out.push(p.strip_prefix(base)?.to_path_buf());
            }
        }
        Ok(())
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v)?;
    Ok(v)
}

/// `anneal course update`: new stages and files arrive; your edited files are never overwritten.
fn update(course_id: &str, courses: &Path, from: Option<&str>, offline: bool) -> anyhow::Result<ExitCode> {
    let repo = find_repo()?;
    // A checkout's own `courses/<id>` (an author working in the repo) wins. Otherwise ask the app for the newest files, so new stages
    // and tests need no new binary; the files built into this anneal are the fallback.
    let fetched = if offline || courses.join(course_id).join("course.toml").exists() {
        None
    } else {
        match crate::update::fetch_bundle(course_id, from) {
            crate::update::Bundle::Fresh(dir) => {
                println!("Course files: the newest ones from the app.");
                Some(dir)
            }
            crate::update::Bundle::Current => {
                println!("Course files: the ones built into this anneal are the newest.");
                None
            }
            crate::update::Bundle::Unavailable(why) => {
                eprintln!("note: using the course files built into this anneal ({why})");
                None
            }
        }
    };
    let courses = fetched.as_deref().unwrap_or(courses);
    let root = course_root(courses, course_id);
    let template = root.join("template");
    if !template.is_dir() {
        bail!("{} has no template", root.display());
    }
    let kept = repo.join(STATE_DIR).join("template");
    if kept.exists() {
        fs::remove_dir_all(&kept)?;
    }
    let planned = read_toml::<CourseToml>(&root.join("course.toml"))?.planned_modules;
    copy_template(&template, &kept, &planned)?;
    // The stage definitions and the base snapshot move forward.
    let defs = repo.join(STATE_DIR).join("course");
    if defs.exists() {
        fs::remove_dir_all(&defs)?;
    }
    copy_definition(&root, &defs, &planned)?;
    let course = learner_course(&repo)?;
    let progress = load_progress(&repo);
    let Sync { added, updated, conflicts } = sync_files(&repo, &kept, &unlocked_modules(&course, &progress))?;
    println!("Updated: {added} new files, {updated} given files refreshed, {} stages in the course.", course.stages.len());
    for c in &conflicts {
        println!("  you edited {}: the new version is in {}.new (merge by hand)", c.display(), c.display());
    }
    println!("`anneal course status` shows where you are.");
    Ok(ExitCode::SUCCESS)
}

fn count_modules(c: &Course) -> usize {
    let mut m: Vec<&str> = c.stages.iter().map(|s| s.module.as_str()).collect();
    m.dedup();
    m.len()
}

fn install_hooks(repo: &Path) -> anyhow::Result<()> {
    let hooks = repo.join(".git").join("hooks");
    if !hooks.is_dir() {
        bail!("{} isn't a git repo", repo.display());
    }
    // The binary that installed the hook runs it, so the hook works without `anneal` being on the PATH.
    let exe = std::env::current_exe().ok().map(|p| p.display().to_string()).unwrap_or_else(|| "anneal".into());
    let script = format!("#!/bin/sh\n# Installed by `anneal course hooks`: runs the current stage's tests and records the result.\nif [ -x '{exe}' ]; then exec '{exe}' course hook pre-push; else exec anneal course hook pre-push; fi\n");
    let path = hooks.join("pre-push");
    fs::write(&path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// A solution diff of one file is hunks of lines starting with ' ' (kept), '-' (removed) and '+' (added), separated by ` ⋯` lines. Applies each hunk
/// whose old lines (kept and removed) are found, in order, in `text`; returns the new text and the hunks that could not be applied.
fn apply_hunks(text: &str, diff: &[String]) -> (String, usize, Vec<Vec<String>>) {
    let mut hunks: Vec<Vec<&str>> = vec![Vec::new()];
    for l in diff {
        if l.trim() == "⋯" {
            hunks.push(Vec::new());
        } else {
            hunks.last_mut().unwrap().push(l);
        }
    }
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let (mut applied, mut rejected) = (0, Vec::new());
    let mut from = 0;
    for h in hunks.into_iter().filter(|h| !h.is_empty()) {
        let old: Vec<&str> = h.iter().filter(|l| l.starts_with(' ') || l.starts_with('-')).map(|l| &l[1..]).collect();
        let new: Vec<String> = h.iter().filter(|l| l.starts_with(' ') || l.starts_with('+')).map(|l| l[1..].to_owned()).collect();
        let at = if old.is_empty() {
            None
        } else {
            (from..lines.len().saturating_sub(old.len() - 1)).find(|&i| old.iter().enumerate().all(|(k, o)| lines[i + k] == *o))
        };
        match at {
            Some(i) => {
                let n = new.len();
                lines.splice(i..i + old.len(), new);
                from = i + n;
                applied += 1;
            }
            None => rejected.push(h.iter().map(|l| (*l).to_owned()).collect()),
        }
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') || text.is_empty() {
        out.push('\n');
    }
    (out, applied, rejected)
}

/// `anneal course adopt`: our solution of a stage, applied to the learner's files.
fn adopt(stage: Option<&str>, yes: bool) -> anyhow::Result<ExitCode> {
    use std::io::{BufRead, IsTerminal, Write};
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let progress = load_progress(&repo);
    let target = match stage {
        Some(id) => course.stage(id)?,
        None => current(&course, &progress).context("every stage is done")?,
    };
    if target.def.kind == "boss" || target.def.kind == "challenge" {
        bail!("{} is a {}: there is no solution to adopt, only tests to pass", target.def.id, target.def.kind);
    }
    println!("Adopting our solution of {} · {} replaces the parts of your files that this stage is about. The stage is marked as helped (assisted) in the app.", target.def.id, target.def.title);
    if !yes {
        if !std::io::stdin().is_terminal() {
            bail!("not a terminal: add --yes to adopt without asking");
        }
        print!("Type adopt to go on: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if line.trim() != "adopt" {
            println!("Nothing changed.");
            return Ok(ExitCode::FAILURE);
        }
    }
    let files = course_sync::open_solution(&course.meta.id, &target.def.id)?;
    let (mut applied, mut rejected_total) = (0, 0);
    let mut rej = String::new();
    for (path, diff) in &files {
        let file = repo.join(path);
        let Ok(text) = fs::read_to_string(&file) else {
            println!("  {path}: not in your repo yet (it arrives with its module); skipped");
            continue;
        };
        let (new, ok, rejected) = apply_hunks(&text, diff);
        if ok > 0 {
            fs::write(&file, new)?;
        }
        applied += ok;
        rejected_total += rejected.len();
        println!("  {path}: {ok} part{} applied{}", if ok == 1 { "" } else { "s" }, if rejected.is_empty() { String::new() } else { format!(", {} did not fit", rejected.len()) });
        for h in rejected {
            rej.push_str(&format!("--- {path}\n{}\n\n", h.join("\n")));
        }
    }
    if !rej.is_empty() {
        let dir = repo.join(STATE_DIR).join("adopt");
        fs::create_dir_all(&dir)?;
        let out = dir.join(format!("{}.txt", target.def.id));
        fs::write(&out, rej)?;
        println!("\n{rejected_total} part(s) did not fit your code: they are in {} (lines starting + are ours, - are the starter code, the rest is context). Merge them by hand.", out.display());
    }
    println!("\n{applied} part(s) applied. Run `anneal course test {}`.", target.def.id);
    Ok(ExitCode::SUCCESS)
}

/// `anneal course reset`: forgets which stages passed, for one module or for the whole course. The learner's code stays as it is.
fn reset(module: Option<&str>, all: bool, yes: bool) -> anyhow::Result<ExitCode> {
    use std::io::{BufRead, IsTerminal, Write};
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let mut progress = load_progress(&repo);
    let (ids, what): (Vec<String>, String) = match (module, all) {
        (Some(code), false) => {
            let ids: Vec<String> = course.stages.iter().filter(|s| s.module == code).map(|s| s.def.id.clone()).collect();
            if ids.is_empty() {
                bail!("no module {code:?}; `anneal course status` lists them");
            }
            (ids, format!("module {code}"))
        }
        (None, true) => (course.stages.iter().map(|s| s.def.id.clone()).collect(), "the whole course".to_owned()),
        _ => bail!("say what to reset: `--module 1a` or `--all`"),
    };
    let done = ids.iter().filter(|id| progress.passed.contains_key(*id)).count();
    println!("This forgets {done} passed stage{} of {what}. Your code is not touched, and files already unlocked stay.", if done == 1 { "" } else { "s" });
    if !yes {
        if !std::io::stdin().is_terminal() {
            bail!("not a terminal: add --yes to reset without asking");
        }
        print!("Type reset to go on: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        if line.trim() != "reset" {
            println!("Nothing changed.");
            return Ok(ExitCode::FAILURE);
        }
    }
    for id in &ids {
        progress.passed.remove(id);
    }
    save_progress(&repo, &progress)?;
    println!("Reset {what}.");
    let scope = match module {
        Some(code) => serde_json::json!({ "module": code }),
        None => serde_json::json!({ "all": true }),
    };
    match course_sync::reset_remote(&course.meta.id, &scope) {
        Some(Ok(_)) => println!("The web app's progress is reset too."),
        Some(Err(e)) => println!("The web app's progress was not reset: {e}"),
        None => println!("Not signed in, so the web app's progress is unchanged (`anneal course login <url>`)."),
    }
    Ok(ExitCode::SUCCESS)
}

/// A text bar of `width` cells, `done` of `total` filled.
fn bar(done: usize, total: usize, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else if done >= total {
        width
    } else {
        // proportional, but a started bar shows something and an unfinished one is never full
        (done * width / total).max(usize::from(done > 0)).min(width.saturating_sub(1))
    };
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// The first line `tool --version` prints, if the tool runs.
fn tool_version(tool: &str) -> Option<String> {
    let out = Command::new(tool).arg("--version").stdin(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").trim().to_owned())
}

/// The total size of the files under `dir`.
fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries.flatten().map(|e| match e.metadata() {
        Ok(m) if m.is_dir() => dir_size(&e.path()),
        Ok(m) => m.len(),
        Err(_) => 0,
    }).sum()
}

/// `anneal course doctor`: what is set up and what is not, with the fix for each problem.
fn doctor() -> anyhow::Result<ExitCode> {
    use crate::term::{green, red};
    let mut bad = 0;
    let mut line = |ok: Option<bool>, what: &str, detail: String| {
        let mark = match ok {
            Some(true) => green("✓"),
            Some(false) => {
                bad += 1;
                red("✗")
            }
            None => "!".to_owned(),
        };
        println!("  {mark} {what:<14} {detail}");
    };
    println!("anneal {}\n", env!("CARGO_PKG_VERSION"));
    // (tool, required, how to get it); curl is only needed to report runs to the web app
    for (tool, required, fix) in [("rustc", true, "install Rust: https://rustup.rs"), ("cargo", true, "install Rust: https://rustup.rs"), ("git", true, "install git"), ("curl", false, "install curl (needed only to report runs)")] {
        match tool_version(tool) {
            Some(v) => line(Some(true), tool, v),
            None => line(if required { Some(false) } else { None }, tool, format!("not found: {fix}")),
        }
    }
    match find_repo() {
        Err(_) => line(None, "repo", "not inside a course repo (run this from the folder `anneal course init` made)".into()),
        Ok(repo) => {
            let course = learner_course(&repo)?;
            let progress = load_progress(&repo);
            line(Some(true), "repo", format!("{} · {} of {} stages done", course.meta.title, progress.passed.len(), course.stages.len()));
            let hook = fs::read_to_string(repo.join(".git/hooks/pre-push")).unwrap_or_default();
            if hook.contains("anneal") {
                line(Some(true), "git hook", "pre-push runs the tests".into());
            } else {
                line(Some(false), "git hook", "missing: run `anneal course hooks`".into());
            }
            let queued = course_sync::queued_runs(&repo);
            if queued > 0 {
                line(None, "queued runs", format!("{queued} waiting: `anneal course sync`"));
            }
            let target = dir_size(&repo.join("target"));
            let gb = target as f64 / 1e9;
            if gb > 8.0 {
                line(None, "build dir", format!("target/ is {gb:.1} GB: `cargo clean` frees it"));
            } else {
                line(Some(true), "build dir", format!("target/ is {gb:.1} GB"));
            }
        }
    }
    match course_sync::session_status() {
        None => line(None, "web app", "not signed in (optional): `anneal course login <url>` to report runs".into()),
        Some((url, Ok(st))) if st < 400 => line(Some(true), "web app", format!("signed in to {url}")),
        Some((url, Ok(st))) => line(Some(false), "web app", format!("{url} answered {st}: `anneal course login {url}` again")),
        Some((url, Err(e))) => line(None, "web app", format!("{url} is not reachable ({e}); runs are queued until it is")),
    }
    Ok(if bad == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn status(json: bool) -> anyhow::Result<ExitCode> {
    use crate::term::{bold, dim, green};
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let progress = load_progress(&repo);
    let cur = current(&course, &progress).map(|s| s.def.id.clone());
    // Challenges are extra practice: they have their own tally and never count towards the course.
    fn core(s: &Stage) -> bool {
        s.def.kind != "challenge"
    }
    let total = course.stages.iter().filter(|s| core(s)).count();
    let done = course.stages.iter().filter(|s| core(s)).filter(|s| progress.passed.contains_key(&s.def.id)).count();
    let challenges = course.stages.iter().filter(|s| s.def.kind == "challenge").count();
    let challenges_done = course.stages.iter().filter(|s| s.def.kind == "challenge" && progress.passed.contains_key(&s.def.id)).count();
    let mut modules: Vec<(String, String, Vec<&Stage>)> = Vec::new();
    for s in &course.stages {
        match modules.last_mut() {
            Some(m) if m.0 == s.module => m.2.push(s),
            _ => modules.push((s.module.clone(), s.module_title.clone(), vec![s])),
        }
    }
    if json {
        let mods: Vec<serde_json::Value> = modules
            .iter()
            .map(|(code, title, stages)| {
                let d = stages.iter().filter(|s| core(s)).filter(|s| progress.passed.contains_key(&s.def.id)).count();
                serde_json::json!({
                    "code": code, "title": title, "done": d, "total": stages.iter().filter(|s| core(s)).count(),
                    "stages": stages.iter().map(|s| serde_json::json!({
                        "id": s.def.id, "title": s.def.title, "difficulty": s.def.difficulty, "kind": s.def.kind,
                        "state": if progress.passed.contains_key(&s.def.id) { "done" } else if Some(&s.def.id) == cur.as_ref() { "current" } else { "todo" },
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "course": course.meta.id, "title": course.meta.title, "done": done, "total": total, "challenges": challenges, "challenges_done": challenges_done, "current": cur, "modules": mods }))?);
        return Ok(ExitCode::SUCCESS);
    }
    println!("{}   {}", bold(&course.meta.title), dim(&format!("{done} of {total} stages · {}%", (done * 100).checked_div(total).unwrap_or(0))));
    println!("{}", bar(done, total, 40));
    if challenges > 0 {
        println!("{}", dim(&format!("{challenges_done} of {challenges} challenges (extra practice, not part of the course)")));
    }
    let strip: Vec<(String, usize, usize)> = modules.iter().map(|(code, _, stages)| (code.clone(), stages.iter().filter(|s| core(s)).filter(|s| progress.passed.contains_key(&s.def.id)).count(), stages.iter().filter(|s| core(s)).count())).collect();
    let here = cur.as_deref().and_then(|id| course.stage(id).ok()).map(|s| s.module.clone());
    println!("\n{}", crate::ui::roadmap(&strip, here.as_deref(), term::text_width()));
    for (code, title, stages) in &modules {
        let n = stages.iter().filter(|s| core(s)).count();
        let d = stages.iter().filter(|s| core(s)).filter(|s| progress.passed.contains_key(&s.def.id)).count();
        let header = format!("{code}  {title}  ({d}/{n})  {}", bar(d, n, 10));
        println!("\n{}", if d == n { green(&header) } else { header });
        for s in stages {
            let mark = if progress.passed.contains_key(&s.def.id) {
                green("✓")
            } else if Some(&s.def.id) == cur.as_ref() {
                bold("→")
            } else {
                " ".to_owned()
            };
            let tag = if s.def.kind == "challenge" { dim("  challenge") } else { String::new() };
            println!("  {mark} {:<8} {:<9} {}{tag}", s.def.id, s.def.difficulty, s.def.title);
        }
    }
    match cur.as_deref().and_then(|id| course.stage(id).ok()) {
        Some(s) => println!("\nNext up: {} · {} ({}). `anneal course show` to read it, `anneal course test` to run it.", s.def.id, s.def.title, s.def.difficulty),
        None => println!("\nEvery stage is done."),
    }
    Ok(ExitCode::SUCCESS)
}

fn show(stage: Option<&str>, hint: Option<usize>, no_pager: bool) -> anyhow::Result<ExitCode> {
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let progress = load_progress(&repo);
    let s = match stage {
        Some(id) => course.stage(id)?,
        None => current(&course, &progress).context("every stage is done")?,
    };
    match hint {
        Some(n) => print_hint(&course.meta.id, s, n, no_pager)?,
        None => print_stage(&repo, &course, &progress, s, no_pager),
    }
    Ok(ExitCode::SUCCESS)
}

/// One hint, opened through the server so that the website knows the stage was helped.
fn print_hint(course_id: &str, s: &Stage, n: usize, no_pager: bool) -> anyhow::Result<()> {
    let (title, md, opened) = course_sync::open_hint(course_id, &s.def.id, n)?;
    if opened {
        eprintln!("Hint {n} opened. When stage {} passes, the website will count it as passed with help.", s.def.id);
    }
    let text = format!("{}\n\n{}", term::bold(&format!("Stage {} · hint {n}: {title}", s.def.id)), render::render(&md, term::text_width(), term::color_on()));
    term::page(&text, no_pager);
    Ok(())
}

/// Stages that count towards the course (challenges are extra practice): (passed, all).
fn core_counts(course: &Course, progress: &Progress) -> (usize, usize) {
    let core = course.stages.iter().filter(|s| s.def.kind != "challenge");
    (core.clone().filter(|s| progress.passed.contains_key(&s.def.id)).count(), core.count())
}

/// What the stage banner shows about `s`: its place in its module and in the course.
fn stage_info<'a>(course: &Course, progress: &Progress, s: &'a Stage) -> crate::ui::StageInfo<'a> {
    let in_module: Vec<&Stage> = course.stages.iter().filter(|x| x.module == s.module && x.def.kind != "challenge").collect();
    let of = in_module.len().max(1);
    let at = in_module.iter().position(|x| x.def.id == s.def.id).map_or(of, |i| i + 1);
    let (course_done, course_total) = core_counts(course, progress);
    crate::ui::StageInfo { module: &s.module, module_title: &s.module_title, id: &s.def.id, title: &s.def.title, difficulty: difficulty_label(&s.def.difficulty), kind: &s.def.kind, at, of, course_done, course_total }
}

/// The results of the learner's recent test runs, by test name. It lives inside `.git`, so it is never committed or reported.
fn last_run_path(repo: &Path) -> PathBuf {
    repo.join(".git").join("anneal-last-run.json")
}

fn load_last_run(repo: &Path) -> BTreeMap<String, bool> {
    fs::read_to_string(last_run_path(repo)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Remembers this run's results. Best effort: a repo without a `.git` directory (or a read-only one) just forgets.
fn save_last_run(repo: &Path, report: &RunReport) {
    let mut last = load_last_run(repo);
    for t in &report.tests {
        last.insert(short_test_name(&t.name), t.ok);
    }
    if let Ok(text) = serde_json::to_string(&last) {
        let _ = fs::write(last_run_path(repo), text);
    }
}

fn short_test_name(full: &str) -> String {
    full.rsplit("::").next().unwrap_or(full).to_owned()
}

/// `s2c_05_a_scan_that_runs...` as `a scan that runs...`: the stage prefix (`s2c_05_`) dropped, underscores as spaces.
fn readable_test_name(name: &str) -> String {
    let parts: Vec<&str> = name.split('_').collect();
    let skip = if parts.len() > 2 && parts[0].starts_with('s') && parts[0].len() <= 4 && parts[1].len() <= 3 && parts[1].chars().all(|c| c.is_ascii_alphanumeric()) { 2 } else { 0 };
    parts[skip..].join(" ")
}

/// The names of the `#[test]` functions of `tests/<bin>.rs` in the learner's repo whose name contains one of `filters` (all of them
/// when there are none), in file order. Read from the source, so it also names tests that never printed a result.
fn test_fn_names(repo: &Path, bin: &str, filters: &[&str]) -> Vec<String> {
    let Ok(text) = fs::read_to_string(repo.join("tests").join(format!("{bin}.rs"))) else { return Vec::new() };
    let mut names: Vec<String> = Vec::new();
    let mut after_attr = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("#[test]") {
            after_attr = true;
        } else if after_attr && let Some(rest) = t.strip_prefix("fn ") {
            let name = rest.split('(').next().unwrap_or("").trim().to_owned();
            if (filters.is_empty() || filters.iter().any(|f| name.contains(f))) && !names.contains(&name) {
                names.push(name);
            }
            after_attr = false;
        } else if !t.starts_with("#[") && !t.is_empty() {
            after_attr = false;
        }
    }
    names
}

/// The names of a stage's test functions, in file order, read from the test files in the learner's repo.
fn stage_test_names(repo: &Path, s: &Stage) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for entry in &s.def.tests {
        let (bin, filter) = split_entry(entry);
        let filters: Vec<&str> = filter.into_iter().collect();
        for n in test_fn_names(repo, bin, &filters) {
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    names
}

/// `anneal course tests`: the test cases of a stage, before you run them.
fn tests_command(stage: Option<&str>) -> anyhow::Result<ExitCode> {
    use crate::term::{bold, dim, green, red};
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let progress = load_progress(&repo);
    let s = match stage {
        Some(id) => course.stage(id)?,
        None => current(&course, &progress).context("every stage is done")?,
    };
    println!("{}\n", crate::ui::stage_banner(&stage_info(&course, &progress, s)));
    let names = stage_test_names(&repo, s);
    if names.is_empty() {
        println!("The test files for this stage are not in your repo yet. They arrive with the module: `anneal course update`.");
        return Ok(ExitCode::SUCCESS);
    }
    let last = load_last_run(&repo);
    let passing = names.iter().filter(|n| last.get(*n) == Some(&true)).count();
    println!("{}  {}", bold(&format!("{} test cases to pass", names.len())), dim(&format!("{passing} passing in your last run")));
    let room = term::text_width().saturating_sub(6);
    for n in &names {
        let mark = match last.get(n) {
            Some(true) => green("✓"),
            Some(false) => red("✗"),
            None => dim("·"),
        };
        println!("  {mark} {}", crate::ui::clip(&readable_test_name(n), room));
    }
    println!("\n{}", dim("✓ passed · ✗ failed · · not run yet"));
    println!("Run them:  anneal course test       One of them:  anneal course test --filter <words of its name, with _>");
    let hints = render::split_hints(&s.readme).1.len();
    if hints > 0 {
        println!("Stuck?     anneal course show {} --hint 1   ({hints} hint{}; the website counts the stage as passed with help)", s.def.id, if hints == 1 { "" } else { "s" });
    }
    Ok(ExitCode::SUCCESS)
}

fn difficulty_label(d: &str) -> &'static str {
    match d {
        "very-easy" => "Very easy · under 5 minutes",
        "easy" => "Easy · 5-10 minutes",
        "medium" => "Medium · 30 minutes to 1 hour",
        _ => "Hard · more than 1 hour",
    }
}

/// The files of the repo that still have a stub marked with this stage's id (`todo!("1a-01: ...")`, `// TODO(2c-03): ...`), with how many
/// places in each: where the learner works on the stage. Files they have already filled in no longer show.
fn stage_files(repo: &Path, id: &str) -> Vec<(String, usize)> {
    fn walk(dir: &Path, root: &Path, id: &str, out: &mut Vec<(String, usize)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            if path.is_dir() {
                walk(&path, root, id, out);
            } else if path.extension().is_some_and(|x| x == "rs") {
                let Ok(text) = fs::read_to_string(&path) else { continue };
                let (a, b) = (format!("{id}:"), format!("TODO({id})"));
                let n = text.lines().filter(|l| l.contains(&a) || l.contains(&b)).count();
                if n > 0 {
                    out.push((path.strip_prefix(root).unwrap_or(&path).display().to_string(), n));
                }
            }
        }
    }
    let mut out = vec![];
    walk(&repo.join("src"), repo, id, &mut out);
    out
}

fn print_stage(repo: &Path, course: &Course, progress: &Progress, s: &Stage, no_pager: bool) {
    let (body, hints) = render::split_hints(&s.readme);
    let (color, width) = (term::color_on(), term::text_width());
    let mut text = format!("{}\n\n", crate::ui::stage_banner(&stage_info(course, progress, s)));
    let files = stage_files(repo, &s.def.id);
    if !files.is_empty() {
        let list = files.iter().map(|(f, n)| format!("  {f}  ({n} place{})", if *n == 1 { "" } else { "s" })).collect::<Vec<_>>().join("\n");
        text.push_str(&format!("{}\n{list}\n{}\n\n", term::bold("Where to work"), term::dim(&format!("each place is a stub marked {}: find them with  grep -rn '{}:' src", s.def.id, s.def.id))));
    }
    text.push_str(&render::render(body, width, color));
    text.push('\n');
    text.push_str(&render::render("Your way: the tests call only the public items named above. How you build the inside is up to you; change the given structs and helpers if you want a different design.", width, color));
    if !s.resources.is_empty() {
        text.push_str(&format!("\n{}\n{}\n", term::bold("Module resources"), s.resources.trim_end()));
    }
    text.push_str(&format!("\n{} {}\n{}\n", term::bold("Tests:"), s.def.tests.iter().map(|t| format!("cargo test --test {}", t.replace("::", " -- "))).collect::<Vec<_>>().join("   "), term::dim(&format!("`anneal course tests {}` lists the test cases; `anneal course test` runs them", s.def.id))));
    if !hints.is_empty() {
        let titles = hints.iter().enumerate().map(|(i, (t, _))| format!("{}. {t}", i + 1)).collect::<Vec<_>>().join("  ");
        text.push_str(&format!("\n{} {titles}\n{}\n", term::bold("Hints:"), term::dim(&format!("open one with `anneal course show {} --hint 1`; the website then counts the stage as passed with help once it passes", s.def.id))));
    }
    term::page(&text, no_pager);
}

fn next() -> anyhow::Result<ExitCode> {
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let progress = load_progress(&repo);
    maybe_unlock(&repo, &course, &progress)?;
    match current(&course, &progress) {
        Some(s) => {
            print_stage(&repo, &course, &progress, s, false);
            Ok(ExitCode::SUCCESS)
        }
        None => {
            println!("That was the last stage. Nice work.");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `anneal course test`: once, or (with `--watch`) again after every change to the learner's files until the stage passes.
fn test_command(stage: Option<&str>, opts: TestOpts, filter: Option<&str>, watch: bool) -> anyhow::Result<ExitCode> {
    if !watch {
        return test(stage, opts, filter);
    }
    let repo = find_repo()?;
    loop {
        if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            print!("\x1b[2J\x1b[H");
        }
        let code = test(stage, opts, filter)?;
        if code == ExitCode::SUCCESS && filter.is_none() {
            return Ok(code);
        }
        println!("\n{}", crate::term::dim("Watching src/ and tests/ for changes (Ctrl-C to stop)..."));
        let before = newest_change(&repo);
        while newest_change(&repo) == before {
            std::thread::sleep(Duration::from_millis(400));
        }
    }
}

/// The latest modification time of anything the learner edits.
fn newest_change(repo: &Path) -> Option<SystemTime> {
    fn walk(dir: &Path, newest: &mut Option<SystemTime>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, newest);
            } else if let Ok(m) = e.metadata().and_then(|m| m.modified())
                && newest.is_none_or(|n| m > n)
            {
                *newest = Some(m);
            }
        }
    }
    let mut newest = None;
    for d in ["src", "tests"] {
        walk(&repo.join(d), &mut newest);
    }
    newest
}

/// What `anneal course test` was asked to do.
#[derive(Clone, Copy)]
struct TestOpts {
    all: bool,
    only: bool,
    verbose: bool,
    /// Called by the pre-push hook: a failure blocks the push only if the repo's config says so.
    hook: bool,
}

/// The exit code for a stage that did not pass: 1 for failing tests, 2 when nothing could run (a compile error or a timeout). The
/// pre-push hook is the exception: it exits 0 unless the repo asked to block pushes.
fn failure_code(opts: TestOpts, block_on_fail: bool, nothing_ran: bool) -> ExitCode {
    if opts.hook {
        if block_on_fail { ExitCode::FAILURE } else { ExitCode::SUCCESS }
    } else if nothing_ran {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

fn test(stage: Option<&str>, opts: TestOpts, filter: Option<&str>) -> anyhow::Result<ExitCode> {
    use crate::term::{bold, dim, green, red};
    let TestOpts { all, only, verbose, .. } = opts;
    crate::term::enable_progress();
    let repo = find_repo()?;
    let course = learner_course(&repo)?;
    let mut progress = load_progress(&repo);
    let cfg: LocalConfig = read_toml(&state_path(&repo, "course.toml"))?;
    let target = match stage {
        Some(id) => course.stage(id)?,
        None => match current(&course, &progress) {
            Some(s) => s,
            None => {
                println!("Every stage is done.");
                return Ok(ExitCode::SUCCESS);
            }
        },
    };
    let (done, total) = core_counts(&course, &progress);
    println!("{}  {}\n", bold(&format!("Stage {} · {}", target.def.id, target.def.title)), dim(&format!("({done} of {total} stages done)")));
    let started = Instant::now();
    let entries: Vec<String> = match filter {
        // the stage's test files, but only the tests with this text in their name
        Some(f) => {
            let mut bins: Vec<String> = target.def.tests.iter().map(|t| t.split("::").next().unwrap_or(t).to_owned()).collect();
            bins.sort();
            bins.dedup();
            bins.into_iter().map(|b| format!("{b}::{f}")).collect()
        }
        None => target.def.tests.clone(),
    };
    if filter.is_none() {
        // the stage page in the browser shows "Running…" from here until the report below
        course_sync::report_start(&course.meta.id, &target.def.id);
    }
    let report = run_entries(&repo, &entries, None, Duration::from_secs(180));
    print_report(&report, verbose);
    save_last_run(&repo, &report);
    let ok = report.passed();
    if filter.is_some() {
        return Ok(if ok { ExitCode::SUCCESS } else { failure_code(opts, cfg.block_on_fail, report.tests.is_empty()) });
    }
    {
        let tests: Vec<(String, bool, String)> = report.tests.iter().map(|t| (t.name.clone(), t.ok, t.detail.lines().filter(|l| !l.trim().is_empty()).take(6).collect::<Vec<_>>().join("\n"))).collect();
        course_sync::report_run(&repo, &course.meta.id, &target.def.id, &tests, report.problem.as_deref(), &head_commit(&repo), started.elapsed().as_millis() as u64);
    }
    if !ok {
        println!("\n{} `anneal course show {}` has the task and hints; `-v` shows every failure in full.", red("Not yet."), target.def.id);
        let failing: Vec<String> = report.tests.iter().filter(|t| !t.ok).map(|t| short_test_name(&t.name)).collect();
        // the first one in the order of the test file: the stage builds up, so that is the one to fix first
        let first = stage_test_names(&repo, target).into_iter().find(|n| failing.contains(n)).or_else(|| failing.iter().min().cloned());
        if let Some(first) = first {
            println!("  {} {}", dim("next to fix:"), readable_test_name(&first));
            let hints = render::split_hints(&target.readme).1.len();
            if hints > 0 {
                println!("  {} anneal course show {} --hint 1   {}", dim("stuck?      "), target.def.id, dim("(counts the stage as helped)"));
            }
        }
        return Ok(failure_code(opts, cfg.block_on_fail, report.tests.is_empty()));
    }
    let ms = started.elapsed().as_millis() as u64;
    // Regression: everything that already passed must still pass.
    let earlier: Vec<String> = if only {
        Vec::new()
    } else {
        course.stages.iter().filter(|s| s.rank < target.rank && (progress.passed.contains_key(&s.def.id) || (all && s.def.kind != "challenge"))).flat_map(|s| s.def.tests.clone()).collect()
    };
    if !earlier.is_empty() {
        let mut uniq = earlier;
        uniq.sort();
        uniq.dedup();
        println!("\n{}", dim("Checking the stages you already passed..."));
        let reg = run_entries(&repo, &uniq, None, Duration::from_secs(300));
        let broken: Vec<&TestOutcome> = reg.tests.iter().filter(|t| !t.ok).collect();
        if let Some(p) = &reg.problem {
            println!("\nRegression run failed: {p}");
            return Ok(failure_code(opts, cfg.block_on_fail, true));
        }
        if !broken.is_empty() {
            println!("\nThis stage passes, but you broke earlier stages:");
            for t in broken {
                println!("  {} {}", red("✗"), t.name);
            }
            return Ok(failure_code(opts, cfg.block_on_fail, false));
        }
        println!("{} earlier tests still pass.", reg.tests.len());
    }
    if !progress.passed.contains_key(&target.def.id) {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        progress.passed.insert(target.def.id.clone(), Passed { at, commit: head_commit(&repo), ms });
        save_progress(&repo, &progress)?;
        let (course_done, course_total) = core_counts(&course, &progress);
        let module_core: Vec<&Stage> = course.stages.iter().filter(|s| s.module == target.module && s.def.kind != "challenge").collect();
        let module_done = module_core.iter().filter(|s| progress.passed.contains_key(&s.def.id)).count();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let last_day = progress.passed.values().filter(|p| p.at + 86_400 >= now).count();
        println!("\n{}", crate::ui::stage_complete(&crate::ui::StageDone { id: &target.def.id, title: &target.def.title, course_done, course_total, took_ms: ms, last_day, module_done, module_total: module_core.len() }));
        if !module_core.is_empty() && module_done == module_core.len() && target.def.kind != "challenge" {
            println!("\n{}", crate::ui::module_complete(&target.module, &target.module_title, module_core.len()));
        }
        let before_module = target.module.clone();
        maybe_unlock(&repo, &course, &progress)?;
        match current(&course, &progress) {
            Some(n) => {
                if n.module != before_module {
                    println!("{}", bold(&format!("Module {} · {} is done. Next module: {} · {}.", before_module, target.module_title, n.module, n.module_title)));
                }
                println!("Next: {} · {}  (`anneal course next`)", n.def.id, n.def.title)
            }
            None => println!("\n{}", crate::ui::course_complete(&course.meta.title)),
        }
    } else {
        println!("\n{}", green(&format!("✓ Stage {} passes (already recorded).", target.def.id)));
    }
    Ok(ExitCode::SUCCESS)
}

/// The tests of one run: the passing ones a line each, the failing ones grouped by their message (a stage whose stub panics with the same
/// `todo!` in twelve tests shows that message once, with the twelve names under it), then a count. `verbose` shows every failure in full.
fn print_report(r: &RunReport, verbose: bool) {
    use crate::term::{clean_detail, dim, green, reason, red};
    let mut tests: Vec<&TestOutcome> = r.tests.iter().collect();
    tests.sort_by(|a, b| a.name.cmp(&b.name));
    let short = |t: &TestOutcome| t.name.rsplit("::").next().unwrap_or(&t.name).to_owned();
    for t in tests.iter().filter(|t| t.ok) {
        println!("  {} {}", green("✓"), short(t));
    }
    // failing tests grouped by what they say: (message lines, test names)
    let mut groups: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    for t in tests.iter().filter(|t| !t.ok) {
        let lines = clean_detail(&t.detail);
        let why = if verbose { t.name.clone() } else { reason(&lines) };
        match groups.iter_mut().find(|g| !why.is_empty() && g.0 == why) {
            Some(g) => g.2.push(short(t)),
            None => groups.push((why, lines, vec![short(t)])),
        }
    }
    let mut hidden = 0;
    for (k, (_, lines, names)) in groups.iter().enumerate() {
        if names.len() == 1 {
            println!("  {} {}", red("✗"), names[0]);
        } else {
            println!("  {} {} tests fail the same way:", red("✗"), names.len());
            names.iter().for_each(|n| println!("      {}", dim(n)));
        }
        if verbose || k < 4 {
            let shown = if verbose { lines.len() } else { 6 };
            lines.iter().take(shown).for_each(|l| println!("        {l}"));
        } else {
            hidden += 1;
        }
    }
    if hidden > 0 {
        println!("        {}", dim(&format!("({hidden} more groups of failures without their messages: run with -v to see them)")));
    }
    if let Some(p) = &r.problem {
        println!("\n{p}");
    }
    let total = tests.len();
    let passed = tests.iter().filter(|t| t.ok).count();
    if total > 0 {
        let line = format!("{passed} of {total} tests passed");
        println!("\n{}", if passed == total { green(&line) } else { red(&line) });
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Authoring: lint (docs/COURSE_STANDARDS.md)

/// The bullet count of each `### Tests` block in a stage's markdown (outside code fences).
fn tests_bullets(md: &str) -> Vec<usize> {
    let (mut in_tests, mut fence, mut n) = (false, false, 0);
    let mut blocks = Vec::new();
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        if fence {
            continue;
        }
        if let Some(h) = line.strip_prefix("## ").or_else(|| line.strip_prefix("### ")) {
            if in_tests {
                blocks.push(n);
            }
            n = 0;
            in_tests = h.trim().to_lowercase() == "tests";
            continue;
        }
        if in_tests && (line.starts_with("- ") || line.starts_with("* ")) {
            n += 1;
        }
    }
    if in_tests {
        blocks.push(n);
    }
    blocks
}

fn lint(course_id: &str, courses: &Path, all: bool) -> anyhow::Result<ExitCode> {
    use anneal_content::course::Course as Def;
    let root = courses.join(course_id);
    let def = if all { Def::load_all(&root) } else { Def::load(&root) }.map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut problems = Vec::new();
    let mut checked = 0;
    for m in &def.modules {
        let core_stages = m.stages.iter().filter(|s| s.kind != "challenge").count();
        if !(6..=10).contains(&core_stages) {
            println!("note  {}: {} stages (6 to 10 is the sweet spot; not a hard limit)", m.code, core_stages);
        }
        for st in &m.stages {
            if st.kind == "boss" {
                continue;
            }
            if st.kind == "challenge" {
                // a challenge is a spec and tests: no toolbox, hints or solution, but it still says what it practises
                checked += 1;
                if st.learn.is_empty() {
                    problems.push(format!("{}: `learn` needs at least 1 entry (what this challenge practises)", st.id));
                }
                for (needed, what) in [("invariants", "what must hold after every step"), ("relations-between-input-and-output", "how the answer must change with the input"), ("examples", "worked cases")] {
                    if !st.sections.iter().any(|x| x.id == needed) {
                        problems.push(format!("{}: a challenge page needs a `## {}` section ({what})", st.id, if needed == "relations-between-input-and-output" { "Relations between input and output" } else if needed == "invariants" { "Invariants" } else { "Examples" }));
                    }
                }
                continue;
            }
            checked += 1;
            let id = &st.id;
            if st.learn.len() < 2 {
                problems.push(format!("{id}: `learn` needs at least 2 entries (what this stage teaches)"));
            }
            if st.concepts.len() + st.concepts_optional.len() < 2 {
                problems.push(format!("{id}: link at least 2 concept articles (`concepts = [...]` for what the stage needs, `concepts_optional = [...]` for Rust and systems ideas the learner can pull in if they want): it is a learning track, and a stage usually leans on two or three ideas"));
            }
            if !st.sections.iter().any(|s| s.id == "performance") {
                problems.push(format!("{id}: add a `## Performance` section"));
            }
            if st.hints.len() < 2 {
                problems.push(format!("{id}: {} hint(s); write at least 2 (design, then the trap, then the invariant)", st.hints.len()));
            }
            let blocks: Vec<usize> = std::iter::once(st.intro.as_str()).chain(st.sections.iter().map(|s| s.md.as_str())).flat_map(tests_bullets).collect();
            let total: usize = blocks.iter().sum();
            if blocks.iter().any(|&b| b > 4) || total > 8 {
                problems.push(format!("{id}: Tests text has {total} bullets ({blocks:?}); summarise the key behaviours: at most 4 per Tests block and 8 per stage (every test is listed by name in Last run)"));
            }
        }
    }
    // Concepts teach code you can use: each one linked from a checked stage needs an "In real code" section with two or more runnable
    // examples (#[test] functions in ```rust test fences, which tools/course_snippets.py compiles and runs) and a "Where it is used" part.
    let mut seen = std::collections::BTreeSet::new();
    for m in &def.modules {
        for st in m.stages.iter().filter(|st| st.kind != "boss") {
            for id in st.concepts.iter().chain(&st.concepts_optional) {
                if !seen.insert(id.clone()) || id.starts_with("errors-") {
                    // the "errors you will meet" pages show compiler output instead of runnable examples
                    continue;
                }
                let Some(k) = def.concept(id) else { continue };
                match k.sections.iter().find(|x| x.id == "in-real-code") {
                    None => problems.push(format!("concept {id}: add a `## In real code` section (the API, examples, where it is used)")),
                    Some(x) => {
                        // Examples are counted as #[test] functions inside ```rust test fences: one fence may hold a whole
                        // implementation with several tests, which is the best kind of example.
                        let tests: usize = x.md.split("```rust test").skip(1).map(|b| b.split("```").next().unwrap_or("").matches("#[test]").count()).sum();
                        if tests < 2 {
                            problems.push(format!("concept {id}: `In real code` needs at least 2 runnable examples (#[test] functions in ```rust test fences)"));
                        }
                        if !x.md.contains("### In the exercises") {
                            problems.push(format!("concept {id}: `In real code` needs a `### In the exercises` part: which stage uses which tool, and how"));
                        }
                        if !x.md.contains("Where it is used") {
                            problems.push(format!("concept {id}: `In real code` needs a `### Where it is used` part (beyond the exercises)"));
                        }
                    }
                }
            }
        }
    }
    for p in &problems {
        println!("FAIL  {p}");
    }
    println!("\n{checked} stages checked, {} problems", problems.len());
    Ok(if problems.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

// ---------------------------------------------------------------------------------------------------------------
// Authoring: solutions

/// Renders every text file of the reference for each stage and uploads (or writes) the per-stage diffs.
fn solutions(course_id: &str, courses: &Path, out: Option<PathBuf>, only: &[String]) -> anyhow::Result<ExitCode> {
    let root = courses.join(course_id);
    let course = Course::load(&root)?;
    let ranks = course.ranks();
    let reference = root.join("reference");
    if !reference.is_dir() {
        bail!("{} has no reference/ (the solution tree the stage markers live in)", root.display());
    }
    fn files(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), "target" | ".git" | ".DS_Store") {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                files(base, &p, out)?;
            } else if p.extension().is_some_and(|x| x == "rs") && !p.strip_prefix(base)?.starts_with("tests") {
                out.push(p.strip_prefix(base)?.to_path_buf());
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    files(&reference, &reference, &mut paths)?;
    let texts: Vec<(String, String)> =
        paths.iter().filter_map(|p| fs::read_to_string(reference.join(p)).ok().filter(|t| t.contains("@begin ")).map(|t| (p.display().to_string(), t))).collect();
    let render = |cutoff: usize| -> anyhow::Result<BTreeMap<String, String>> {
        texts.iter().map(|(name, t)| Ok((name.clone(), render_text(t, &ranks, cutoff, name)?))).collect()
    };
    let mut diffs = course_sync::solution_diffs(&ranks, &render)?;
    // a challenge has no published solution: the point is to solve it yourself
    for st in course.stages.iter().filter(|s| s.def.kind == "challenge") {
        diffs.remove(&st.def.id);
    }
    match out {
        Some(path) => {
            fs::write(&path, serde_json::to_string_pretty(&diffs)? + "\n")?;
            println!("Wrote {} stage solutions to {}", diffs.values().filter(|v| !v.is_empty()).count(), path.display());
        }
        None => course_sync::push_solutions(&course.meta.id, &diffs, if only.is_empty() { None } else { Some(only) })?,
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------------------------------------------
// Authoring: verify

fn verify(course_id: &str, courses: &Path, only: Option<&str>, from: Option<&str>, work: Option<PathBuf>) -> anyhow::Result<ExitCode> {
    let root = courses.join(course_id);
    let course = Course::load(&root)?;
    let ranks = course.ranks();
    let reference = root.join("reference");
    if !reference.is_dir() {
        bail!("{} has no reference/ (it is kept out of the public repo; see docs/BUSTUB.md §7)", root.display());
    }
    let work = work.unwrap_or_else(|| std::env::temp_dir().join(format!("anneal-course-{course_id}")));
    let target = work.join("target");
    let repo = work.join("repo");
    fs::create_dir_all(&repo)?;
    let first = match from {
        Some(id) => course.stage(id)?.rank,
        None => 1,
    };
    let mut problems = 0;
    for s in &course.stages {
        if s.rank < first {
            continue;
        }
        if let Some(id) = only
            && s.def.id != id
        {
            continue;
        }
        let t0 = Instant::now();
        // Before: the stage's tests must fail (unless a boss).
        render_tree(&reference, &repo, &ranks, s.rank - 1)?;
        let before = run_entries(&repo, &s.def.tests, Some(&target), Duration::from_secs(120));
        // After: they must pass, and so must everything before.
        render_tree(&reference, &repo, &ranks, s.rank)?;
        let after = run_entries(&repo, &s.def.tests, Some(&target), Duration::from_secs(120));
        let mut notes: Vec<String> = Vec::new();
        if !after.passed() {
            notes.push(format!(
                "tests fail with the solution: {}",
                after.problem.clone().unwrap_or_else(|| after.tests.iter().filter(|t| !t.ok).map(|t| t.name.clone()).collect::<Vec<_>>().join(", "))
            ));
        }
        if s.def.kind != "boss" && after.tests.len() < 5 {
            notes.push(format!("{} test(s); a stage needs at least 5, each checking a distinct behaviour (docs/COURSE_STANDARDS.md)", after.tests.len()));
        }
        if !s.def.retest && before.problem.is_none() && !before.tests.is_empty() && before.tests.iter().all(|t| t.ok) {
            notes.push("all tests already pass before the stage (the stub isn't doing its job)".into());
        }
        if before.problem.as_deref().is_some_and(|p| p.contains("didn't compile")) {
            notes.push("the stub state doesn't compile".into());
        }
        // A stage re-runs the earlier stages of its own module; the module's last stage (its boss) re-runs everything before it.
        let module_of = |id: &str| id.split('-').next().unwrap_or("").to_owned();
        let last_of_module = course.stages.iter().rev().filter(|x| x.def.kind != "challenge").find(|x| module_of(&x.def.id) == module_of(&s.def.id)).is_some_and(|x| x.rank == s.rank);
        // a challenge stands alone: it is not a step on the way, so there is nothing before it to keep green
        let mut earlier: Vec<String> = course
            .stages
            .iter()
            .filter(|x| s.def.kind != "challenge" && x.rank < s.rank && (last_of_module || module_of(&x.def.id) == module_of(&s.def.id)))
            .flat_map(|x| x.def.tests.clone())
            .collect();
        earlier.sort();
        earlier.dedup();
        if notes.is_empty() && !earlier.is_empty() {
            let reg = run_entries(&repo, &earlier, Some(&target), Duration::from_secs(300));
            if !reg.passed() {
                notes.push(format!(
                    "earlier tests break: {}",
                    reg.problem.clone().unwrap_or_else(|| reg.tests.iter().filter(|t| !t.ok).map(|t| t.name.clone()).collect::<Vec<_>>().join(", "))
                ));
            }
        }
        if notes.is_empty() {
            println!("ok    {:<8} {:<5} {:>3} tests  {:>5.1}s  {}", s.def.id, s.def.kind, after.tests.len(), t0.elapsed().as_secs_f64(), s.def.title);
        } else {
            problems += 1;
            println!("FAIL  {:<8} {}", s.def.id, s.def.title);
            for n in notes {
                println!("        {n}");
            }
        }
    }
    if (only.is_none() && from.is_none()) || only == Some("unlock") {
        problems += verify_unlock_states(&course, &reference, &repo, &work, &target, &ranks)?;
    }
    println!("\n{} stages, {problems} problems", course.stages.len());
    Ok(if problems == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Every module's unlock state must compile (and so must its tests): the repo a learner has when they reach module k, with the
/// stubs of module k, shows only the files of modules 1..=k. Catches a file that is revealed later than something that names it.
fn verify_unlock_states(course: &Course, reference: &Path, full: &Path, work: &Path, target: &Path, ranks: &HashMap<String, usize>) -> anyhow::Result<usize> {
    let map = files_map_for(course, reference)?;
    let order = module_order(course);
    let shown = work.join("unlock-repo");
    let mut problems = 0;
    for (k, code) in order.iter().enumerate() {
        let cutoff = if k == 0 { 0 } else { course.stages.iter().filter(|s| order[..k].contains(&s.module)).map(|s| s.rank).max().unwrap_or(0) };
        render_tree(reference, full, ranks, cutoff)?;
        let files: Vec<PathBuf> = list_files(full)?.into_iter().filter(|p| !p.starts_with("target")).collect();
        let vis = course_unlock::visible(&files, &map, &order[..=k], &|rel| fs::read_to_string(full.join(rel)).map(|t| course_unlock::only_declares(&t)).unwrap_or(true));
        if shown.exists() {
            fs::remove_dir_all(&shown)?;
        }
        for rel in &vis {
            let dest = shown.join(rel);
            fs::create_dir_all(dest.parent().unwrap())?;
            let raw = fs::read(full.join(rel))?;
            if course_unlock::is_module_file(rel) {
                let text = String::from_utf8_lossy(&raw).into_owned();
                fs::write(&dest, course_unlock::filter_mod(&text, rel.parent().unwrap_or(Path::new("")), &vis))?;
            } else {
                fs::write(&dest, raw)?;
            }
        }
        let t0 = Instant::now();
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&shown).args(["test", "--no-run"]).env("CARGO_TARGET_DIR", target).env("CARGO_TERM_COLOR", "never");
        let (_, stderr, status) = run_with_timeout(cmd, Duration::from_secs(300)).map_err(|e| anyhow::anyhow!("{e}"))?;
        if status == Some(0) {
            println!("ok    unlock {:<4} {:>3} files visible  {:>5.1}s", code, vis.len(), t0.elapsed().as_secs_f64());
        } else {
            problems += 1;
            println!("FAIL  unlock {code}: the repo a learner has on reaching this module doesn't compile");
            for l in stderr.lines().filter(|l| l.starts_with("error")).take(6) {
                println!("        {l}");
            }
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod adopt_tests {
    use super::apply_hunks;

    fn d(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn a_hunk_replaces_the_starter_lines_it_finds() {
        let text = "fn a() {\n    todo!()\n}\nfn b() {}\n";
        let (out, ok, rej) = apply_hunks(text, &d(&[" fn a() {", "-    todo!()", "+    1 + 1", " }"]));
        assert_eq!(out, "fn a() {\n    1 + 1\n}\nfn b() {}\n");
        assert_eq!((ok, rej.len()), (1, 0));
    }

    #[test]
    fn a_hunk_that_does_not_fit_is_set_aside_and_the_file_is_untouched() {
        let text = "fn a() {\n    my own code\n}\n";
        let (out, ok, rej) = apply_hunks(text, &d(&[" fn a() {", "-    todo!()", "+    1 + 1", " }"]));
        assert_eq!(out, text);
        assert_eq!((ok, rej.len()), (0, 1));
    }

    #[test]
    fn several_hunks_apply_in_order_and_each_is_judged_alone() {
        let text = "a\nTODO1\nb\nc\nd\ne\nf\nTODO2\ng\n";
        let diff = d(&[" a", "-TODO1", "+one", " b", " ⋯", " f", "-TODO2", "+two", " g"]);
        let (out, ok, rej) = apply_hunks(text, &diff);
        assert_eq!(out, "a\none\nb\nc\nd\ne\nf\ntwo\ng\n");
        assert_eq!((ok, rej.len()), (2, 0));
        let (out, ok, rej) = apply_hunks("a\nmine\nb\nc\nd\ne\nf\nTODO2\ng\n", &diff);
        assert_eq!(out, "a\nmine\nb\nc\nd\ne\nf\ntwo\ng\n", "the first hunk does not fit, the second still does");
        assert_eq!((ok, rej.len()), (1, 1));
    }
}

#[cfg(test)]
mod bar_tests {
    use super::bar;

    #[test]
    fn a_progress_bar_is_never_full_until_done_and_never_empty_once_started() {
        assert_eq!(bar(0, 10, 10), "░░░░░░░░░░");
        assert_eq!(bar(1, 100, 10), "█░░░░░░░░░");
        assert_eq!(bar(99, 100, 10), "█████████░");
        assert_eq!(bar(10, 10, 10), "██████████");
        assert_eq!(bar(0, 0, 4), "░░░░");
    }
}

#[cfg(test)]
mod learner_screens {
    use super::*;

    #[test]
    fn a_test_name_reads_as_a_sentence() {
        assert_eq!(readable_test_name("s2c_05_a_scan_that_runs_while_writers_work"), "a scan that runs while writers work");
        assert_eq!(readable_test_name("s1a_01_new_creates_the_db_file"), "new creates the db file");
        assert_eq!(readable_test_name("sr_01_bytes_round_trip"), "bytes round trip");
        assert_eq!(readable_test_name("s2c_c1_a_challenge_test"), "a challenge test");
        // not a stage prefix: left alone
        assert_eq!(readable_test_name("plain_name_here"), "plain name here");
        assert_eq!(readable_test_name("short"), "short");
    }

    #[test]
    fn a_process_past_its_limit_is_stopped_and_what_it_printed_is_kept() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo 'test a ... ok'; echo 'test b ... FAILED'; exec sleep 30"]);
        let started = Instant::now();
        let ran = run_until(cmd, Duration::from_millis(600)).unwrap();
        assert!(ran.timed_out, "it ran for 30 s against a 0.6 s limit");
        assert!(started.elapsed() < Duration::from_secs(10), "it was killed, not waited for");
        assert!(ran.stdout.contains("test a ... ok") && ran.stdout.contains("test b ... FAILED"), "what it printed before the limit is kept: {:?}", ran.stdout);
        let quick = run_until({ let mut c = Command::new("sh"); c.args(["-c", "echo done"]); c }, Duration::from_secs(10)).unwrap();
        assert!(!quick.timed_out && quick.code == Some(0) && quick.stdout.trim() == "done", "a process that ends in time is not marked");
        let mut slow = Command::new("sh");
        slow.args(["-c", "exec sleep 30"]);
        assert!(run_with_timeout(slow, Duration::from_millis(300)).unwrap_err().to_string().starts_with("Time limit exceeded"), "the strict form says so");
    }

    #[test]
    fn tests_that_never_finished_become_time_limit_failures() {
        let mut tests = vec![
            TestOutcome { name: "earlier".into(), ok: true, detail: String::new() },
            TestOutcome { name: "s9z_01_done".into(), ok: true, detail: String::new() },
            TestOutcome { name: "s9z_01_failed".into(), ok: false, detail: "boom".into() },
        ];
        let n = mark_unfinished(&mut tests, 1, vec!["s9z_01_done".into(), "s9z_01_failed".into(), "s9z_01_hangs".into(), "earlier".into()], 180);
        assert_eq!(n, 2, "the one that hangs, and `earlier`, which is not among the outcomes from index 1");
        let hung = tests.iter().find(|t| t.name == "s9z_01_hangs").unwrap();
        assert!(!hung.ok && hung.detail.starts_with("Time limit exceeded") && hung.detail.contains("180 s"), "{hung:?}");
        assert!(tests.iter().find(|t| t.name == "s9z_01_failed").unwrap().detail == "boom", "a failure that was reported keeps its own detail");
    }

    #[test]
    fn test_function_names_are_filtered_and_kept_in_file_order() {
        let dir = std::env::temp_dir().join(format!("anneal-fn-names-{}", std::process::id()));
        fs::create_dir_all(dir.join("tests")).unwrap();
        fs::write(dir.join("tests/stages_9y.rs"), "#[test]\nfn s9y_01_a() {}\n\n#[test]\n#[ignore]\nfn s9y_02_b() {}\n\nfn helper() {}\n\n#[test]\nfn s9y_01_c() {}\n").unwrap();
        assert_eq!(test_fn_names(&dir, "stages_9y", &[]), ["s9y_01_a", "s9y_02_b", "s9y_01_c"], "no filter: every test, in file order");
        assert_eq!(test_fn_names(&dir, "stages_9y", &["s9y_01"]), ["s9y_01_a", "s9y_01_c"], "a filter keeps the matching names");
        assert!(test_fn_names(&dir, "no_such_file", &[]).is_empty(), "a missing file names nothing");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stages_tests_are_read_from_the_test_file_in_order() {
        let dir = std::env::temp_dir().join(format!("anneal-tests-listing-{}", std::process::id()));
        fs::create_dir_all(dir.join("tests")).unwrap();
        fs::write(
            dir.join("tests/stages_9z.rs"),
            "use x;\n\n#[test]\nfn s9z_01_first_thing() {}\n\n/// docs\n#[test]\n#[should_panic]\nfn s9z_01_second_thing() {}\n\nfn helper_not_a_test() {}\n\n#[test]\nfn s9z_02_other_stage() {}\n",
        )
        .unwrap();
        let stage = |tests: &[&str]| Stage {
            def: StageToml { id: "9z-01".into(), title: "t".into(), kind: "build".into(), difficulty: "easy".into(), tests: tests.iter().map(|t| (*t).to_owned()).collect(), retest: false },
            module: "9z".into(),
            optional: false,
            module_title: "m".into(),
            readme: String::new(),
            resources: String::new(),
            rank: 0,
        };
        assert_eq!(stage_test_names(&dir, &stage(&["stages_9z::s9z_01"])), ["s9z_01_first_thing", "s9z_01_second_thing"]);
        assert_eq!(stage_test_names(&dir, &stage(&["stages_9z"])).len(), 3);
        assert!(stage_test_names(&dir, &stage(&["stages_missing"])).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
