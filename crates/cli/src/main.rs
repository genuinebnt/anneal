mod course;
mod course_sync;
mod course_unlock;
mod embedded;
mod ui;
mod update;
mod render;
mod term;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anneal_content::{Catalog, Loaded, Mode, Status};
use anneal_runner::{Outcome, RunResult, Runner, RunnerConfig, Sandbox, Submission, Suite};
use anyhow::{Context, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "anneal",
    version,
    about = "Tools for the anneal Rust interview-prep platform"
)]
struct Cli {
    /// The content directory.
    #[arg(long, global = true, default_value = "content")]
    content: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check every track and problem and list the issues found.
    Validate,
    /// Prompt for a login passphrase and print the ANNEAL_PASSPHRASE_HASH value for it.
    Passphrase,
    /// Print a shell completion script: `anneal completions zsh > ~/.zfunc/_anneal` (bash, zsh, fish, elvish, powershell).
    Completions {
        shell: clap_complete::Shell,
    },
    /// CodeCrafters-style courses (docs/BUSTUB.md): create a repo, run a stage's tests, and the git hooks.
    #[command(subcommand)]
    Course(course::CourseCmd),
    /// Replace this anneal with the build the app serves, when they differ. Asks the app you signed in to (or `--from`).
    SelfUpdate {
        /// The app's address, e.g. https://anneal.example.dev. Default: the one you signed in to.
        #[arg(long)]
        from: Option<String>,
        /// Only say whether a newer build exists.
        #[arg(long)]
        check: bool,
    },
    /// List tracks, or the problems in one track.
    List { track: Option<String> },
    /// Check every ready problem end to end: the reference solution passes every test
    /// without breaking a rule, the starter doesn't pass, a write-it starter compiles, there are
    /// at least 5 visible and 8 hidden tests, and every `wrong/<name>.rs` (at least one) compiles but fails Submit.
    Verify {
        /// Only this track (code or folder name), e.g. d1.
        track: Option<String>,
        /// Problems run at once.
        #[arg(long, default_value_t = 6)]
        jobs: usize,
    },
    /// Run a problem's tests against the starter, the reference solution, or your own file.
    Run {
        /// Problem id, e.g. d9-network-delay-time.
        problem: String,
        /// Your src/lib.rs. Defaults to the problem's starter.
        #[arg(long, conflicts_with = "solution")]
        code: Option<PathBuf>,
        /// Run the reference solution.
        #[arg(long)]
        solution: bool,
        /// Include hidden tests, like Submit.
        #[arg(long)]
        submit: bool,
        /// Run inside the Docker sandbox instead of host cargo.
        #[arg(long)]
        docker: bool,
        #[arg(long, default_value = "anneal-runner:1.98")]
        image: String,
        /// Docker context for the sandbox; empty means the current context.
        #[arg(long, default_value = anneal_runner::DEFAULT_DOCKER_CONTEXT)]
        docker_context: String,
        /// Skip clippy.
        #[arg(long)]
        no_clippy: bool,
        /// Print the full result as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    term::quiet_broken_pipe();
    let cli = Cli::parse();
    if let Command::Passphrase = cli.command {
        return passphrase();
    }
    if let Command::Completions { shell } = cli.command {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        clap_complete::generate(shell, &mut cmd, "anneal", &mut std::io::stdout());
        return Ok(ExitCode::SUCCESS);
    }
    // Courses run on the learner's machine, without the content directory.
    let cli = match cli.command {
        Command::Course(cmd) => return course::run(cmd),
        Command::SelfUpdate { from, check } => return update::self_update(from, check),
        _ => cli,
    };
    let Loaded { catalog, issues } = Catalog::load(&cli.content)
        .with_context(|| format!("loading {}", cli.content.display()))?;
    match cli.command {
        Command::Passphrase | Command::Completions { .. } | Command::Course(_) | Command::SelfUpdate { .. } => unreachable!("handled before loading content"),
        Command::Validate => {
            let problems: usize = catalog.tracks.iter().map(|t| t.problems.len()).sum();
            let mut issues: Vec<String> = issues.iter().map(ToString::to_string).collect();
            issues.extend(unknown_crates(&catalog));
            for issue in &issues {
                println!("{issue}");
            }
            println!(
                "{} tracks, {problems} problems, {} issues",
                catalog.tracks.len(),
                issues.len()
            );
            Ok(if issues.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Command::List { track } => {
            match track {
                None => {
                    for t in &catalog.tracks {
                        let ready = t
                            .problems
                            .iter()
                            .filter(|p| p.meta.status == anneal_content::Status::Ready)
                            .count();
                        println!(
                            "{:<4} {:<32} {:>3} problems ({ready} ready)",
                            t.code,
                            t.name,
                            t.problems.len()
                        );
                    }
                }
                Some(code) => {
                    let Some(t) = catalog.track(&code) else {
                        bail!("no track {code:?}")
                    };
                    for p in &t.problems {
                        println!(
                            "{:>3}  {:<40} {:?} {:?} {:?}  {}",
                            p.meta.order,
                            p.id,
                            p.meta.mode,
                            p.meta.level,
                            p.meta.status,
                            p.meta.title
                        );
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Verify { track, jobs } => verify(&catalog, track.as_deref(), jobs).await,
        Command::Run {
            problem,
            code,
            solution,
            submit,
            docker,
            image,
            docker_context,
            no_clippy,
            json,
        } => {
            let Some((_, p)) = catalog.problem(&problem) else {
                bail!("no problem {problem:?}; try `anneal list <track>`")
            };
            let lib_rs = match (&code, solution) {
                (Some(path), _) => std::fs::read_to_string(path)
                    .with_context(|| format!("reading {}", path.display()))?,
                (None, true) => p
                    .files
                    .solution
                    .clone()
                    .context("this problem has no solution.rs")?,
                (None, false) => p
                    .files
                    .starter
                    .clone()
                    .context("this problem has no starter.rs")?,
            };
            let visible = p
                .files
                .visible_tests
                .as_deref()
                .context("this problem has no tests/visible.rs")?;
            let hidden = if submit {
                Some(
                    p.files
                        .hidden_tests
                        .as_deref()
                        .context("this problem has no tests/hidden.rs")?,
                )
            } else {
                None
            };
            let sandbox = if docker {
                Sandbox::docker(image).in_context(&docker_context)
            } else {
                Sandbox::Host
            };
            let mut config = RunnerConfig::new(sandbox, anneal_runner::default_work_root());
            config.clippy = !no_clippy;
            let result = Runner::new(config)
                .run(
                    &p.id,
                    &Submission {
                        lib_rs: &lib_rs,
                        visible_tests: visible,
                        hidden_tests: hidden,
                        crates: &p.meta.crates,
                        perf: runner_perf(p.meta.perf),
                    },
                )
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print_result(&result);
            }
            Ok(if result.status == anneal_runner::RunStatus::Passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
    }
}

struct Case {
    id: String,
    mode: Mode,
    language: anneal_content::Language,
    starter: String,
    solution: String,
    visible: String,
    hidden: String,
    wrong: Vec<(String, String)>,
    rules: Option<anneal_content::Rules>,
    crates: Vec<String>,
    perf: anneal_runner::Perf,
}

async fn verify(catalog: &Catalog, track: Option<&str>, jobs: usize) -> anyhow::Result<ExitCode> {
    let mut config = RunnerConfig::new(Sandbox::Host, anneal_runner::default_work_root());
    config.clippy = false;
    let runner = Arc::new(Runner::new(config));
    let slots = Arc::new(tokio::sync::Semaphore::new(jobs.max(1)));
    let mut tasks = tokio::task::JoinSet::new();
    for t in &catalog.tracks {
        if track.is_some_and(|c| !t.code.eq_ignore_ascii_case(c) && t.slug != c) {
            continue;
        }
        // DSA problems are LeetCode links: there's nothing to run.
        for p in t.problems.iter().filter(|p| p.meta.status == Status::Ready && p.dsa.is_none()) {
            let f = &p.files;
            let case = Case {
                id: p.id.clone(),
                mode: p.meta.mode,
                language: p.meta.language,
                starter: f.starter.clone().unwrap_or_default(),
                solution: f.solution.clone().unwrap_or_default(),
                visible: f.visible_tests.clone().unwrap_or_default(),
                hidden: f.hidden_tests.clone().unwrap_or_default(),
                wrong: f.wrong.clone(),
                rules: p.meta.rules.clone(),
                crates: p.meta.crates.clone(),
                perf: runner_perf(p.meta.perf),
            };
            let (runner, slots) = (runner.clone(), slots.clone());
            tasks.spawn(async move {
                let _permit = slots.acquire_owned().await;
                let issues = verify_one(&runner, &case).await;
                (case.id, issues)
            });
        }
    }
    let mut results = Vec::new();
    while let Some(r) = tasks.join_next().await {
        results.push(r?);
    }
    results.sort();
    let failed = results.iter().filter(|(_, i)| !i.is_empty()).count();
    for (id, issues) in &results {
        if issues.is_empty() {
            println!("ok    {id}");
        } else {
            println!("FAIL  {id}");
            for i in issues {
                println!("        {i}");
            }
        }
    }
    println!("{} problems verified, {failed} failed", results.len());
    Ok(if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

async fn verify_one(runner: &Runner, c: &Case) -> Vec<String> {
    let mut issues = Vec::new();
    let marker = if c.language == anneal_content::Language::Python { "\ndef test_" } else { "#[test]" };
    let count = |src: &str| format!("\n{src}").matches(marker).count();
    // The bar from the test-hardening pass (HANDOFF §6.1): enough visible tests to explain the problem, enough
    // hidden ones to check it, and at least one wrong solution the tests are shown to reject.
    if count(&c.visible) < 5 {
        issues.push(format!("only {} visible tests; want at least 5", count(&c.visible)));
    }
    if count(&c.hidden) < 8 {
        issues.push(format!("only {} hidden tests; want at least 8", count(&c.hidden)));
    }
    if c.wrong.is_empty() {
        issues.push(format!("no wrong/<name>.{}; add a plausible wrong solution the tests reject", c.language.ext()));
    }
    let broken = |code: &str| c.rules.as_ref().map(|r| anneal_rules::check(code, &c.starter, r)).unwrap_or_default();

    match submit(runner, c, &c.solution).await {
        Ok(r) => {
            if r.status != anneal_runner::RunStatus::Passed {
                issues.push(format!("solution: {:?}, {}/{} passing{}", r.status, r.passed, r.total, first_problem(&r)));
            }
            for v in broken(&c.solution) {
                issues.push(format!("solution breaks rule {}: {}", v.rule, v.message));
            }
        }
        Err(e) => issues.push(format!("solution: runner error {e}")),
    }
    match submit(runner, c, &c.starter).await {
        Ok(r) => {
            if r.status == anneal_runner::RunStatus::Passed && broken(&c.starter).is_empty() {
                issues.push("starter already passes every test without breaking a rule".into());
            }
            if c.mode == Mode::Write && r.status == anneal_runner::RunStatus::CompileError {
                issues.push(format!("write-it starter doesn't compile{}", first_problem(&r)));
            }
        }
        Err(e) => issues.push(format!("starter: runner error {e}")),
    }
    // A wrong solution must be caught by a failing or timed-out test (or a broken rule), not by the compiler.
    for (name, code) in &c.wrong {
        match submit(runner, c, code).await {
            Ok(r) if r.status == anneal_runner::RunStatus::CompileError => {
                issues.push(format!("wrong/{name}.{} doesn't compile{}", c.language.ext(), first_problem(&r)));
            }
            Ok(r) if r.status == anneal_runner::RunStatus::Passed && broken(code).is_empty() => {
                issues.push(format!("wrong/{name}.{} passes every test; add a test that catches it", c.language.ext()));
            }
            Ok(_) => {}
            Err(e) => issues.push(format!("wrong/{name}.{}: runner error {e}", c.language.ext())),
        }
    }
    issues
}

async fn submit(runner: &Runner, c: &Case, code: &str) -> Result<RunResult, anneal_runner::RunnerError> {
    if c.language == anneal_content::Language::Python {
        return runner.run_python(&anneal_runner::PySubmission { code, visible_tests: &c.visible, hidden_tests: Some(&c.hidden) }).await;
    }
    runner.run(&c.id, &Submission { lib_rs: code, visible_tests: &c.visible, hidden_tests: Some(&c.hidden), crates: &c.crates, perf: c.perf }).await
}

/// The first compiler error or failing test, for a one-line report.
fn first_problem(r: &RunResult) -> String {
    if let Some(d) = r.diagnostics.iter().find(|d| d.is_error()) {
        return format!(" · {}", d.rendered.lines().take(3).collect::<Vec<_>>().join(" | "));
    }
    r.tests
        .iter()
        .find(|t| t.outcome != Outcome::Passed)
        .map(|t| match &t.check {
            Some(c) => format!(" · {} expected {} got {}", t.name, c.expected, c.got),
            None => format!(" · {} {}", t.name, t.panic.as_deref().unwrap_or("")),
        })
        .unwrap_or_default()
}

fn print_result(r: &RunResult) {
    for d in &r.diagnostics {
        println!("{}", d.rendered.trim_end());
    }
    for t in &r.tests {
        let mark = match t.outcome {
            Outcome::Passed => "ok",
            Outcome::Failed => "FAILED",
            Outcome::Ignored => "ignored",
            Outcome::TimedOut => "TIMED OUT",
        };
        let suite = if t.suite == Suite::Hidden {
            "hidden::"
        } else {
            ""
        };
        println!("  {mark:<9} {suite}{}", t.name);
        if let Some(c) = &t.check {
            println!(
                "            input    {}\n            expected {}\n            got      {}",
                c.input, c.expected, c.got
            );
        } else if let Some(p) = &t.panic {
            println!("            {p}");
        }
    }
    println!(
        "{:?} · {} / {} passing · {} ms",
        r.status, r.passed, r.total, r.duration_ms
    );
}

/// Hashes a passphrase with argon2id for `ANNEAL_PASSPHRASE_HASH`.
fn passphrase() -> anyhow::Result<ExitCode> {
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
    use std::io::IsTerminal;
    // Prompt without echo on a terminal; read one line when piped (e.g. from a secrets manager).
    let first = if std::io::stdin().is_terminal() {
        let first = rpassword::prompt_password("New passphrase: ")?;
        if rpassword::prompt_password("Again: ")? != first {
            anyhow::bail!("the passphrases don't match");
        }
        first
    } else {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        line.trim_end_matches(['\r', '\n']).to_owned()
    };
    if first.chars().count() < 12 {
        anyhow::bail!("use at least 12 characters");
    }
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default()
        .hash_password(first.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing failed: {e}"))?;
    println!("ANNEAL_PASSPHRASE_HASH='{hash}'");
    Ok(ExitCode::SUCCESS)
}

/// Problems that ask for crates outside the sandbox's crate set.
fn unknown_crates(catalog: &Catalog) -> Vec<String> {
    let known = anneal_runner::known_crates();
    catalog
        .tracks
        .iter()
        .flat_map(|t| &t.problems)
        .flat_map(|p| {
            p.meta
                .crates
                .iter()
                .filter(|c| !known.contains(&c.as_str()))
                .map(move |c| format!("{}: crate {c:?} isn't in docker/deps/Cargo.toml", p.dir.join("problem.toml").display()))
        })
        .collect()
}

/// A problem's `[perf]` as the runner takes it; all off when there's none.
fn runner_perf(perf: Option<anneal_content::Perf>) -> anneal_runner::Perf {
    let p = perf.unwrap_or_default();
    anneal_runner::Perf { release: p.release, asm: p.asm, count_allocs: p.count_allocs }
}
