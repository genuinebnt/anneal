//! Runs one cargo command, on the host or inside a throwaway container, with a time limit.

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::{RunnerError, Sandbox};

pub(crate) struct Captured {
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    /// The process's exit code; `None` when it timed out or was killed by a signal.
    pub exit_code: Option<i32>,
}

static CONTAINER_SEQ: AtomicU64 = AtomicU64::new(0);

/// What to start: cargo itself, or a binary cargo already built into the target directory.
#[derive(Clone, Copy)]
pub(crate) enum Program<'a> {
    Cargo,
    /// e.g. `debug/scratch`, relative to the target directory.
    Built(&'a str),
    /// `python3`, for practice problems.
    Python,
}

pub(crate) async fn cargo(
    sandbox: &Sandbox,
    work: &Path,
    target: &Path,
    args: &[&str],
    limit: Duration,
) -> Result<Captured, RunnerError> {
    run(sandbox, work, target, Program::Cargo, args, limit).await
}

pub(crate) async fn run(
    sandbox: &Sandbox,
    work: &Path,
    target: &Path,
    program: Program<'_>,
    args: &[&str],
    limit: Duration,
) -> Result<Captured, RunnerError> {
    let mut container = None;
    let mut cmd = match sandbox {
        Sandbox::Host => {
            let mut c = match program {
                Program::Cargo => Command::new("cargo"),
                Program::Built(rel) => Command::new(target.join(rel)),
                Program::Python => Command::new("python3"),
            };
            c.args(args)
                .current_dir(work)
                .env("CARGO_TARGET_DIR", target)
                .env("CARGO_TERM_COLOR", "never")
                // No incremental compilation: rustc's copies from its incremental directory have failed ("unable to
                // copy …/incremental/…: No such file or directory"), failing a run for nothing the learner wrote.
                .env("CARGO_INCREMENTAL", "0")
                .env("RUSTC_BOOTSTRAP", "1")
                // Own process group, so a timeout can kill the test binary cargo started too.
                .process_group(0);
            c
        }
        Sandbox::Docker {
            image,
            memory,
            cpus,
            context,
        } => {
            let meta = std::fs::metadata(work).map_err(|e| RunnerError::io("stat work dir", e))?;
            let name = format!(
                "anneal-{}-{}",
                std::process::id(),
                CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed)
            );
            let mut c = docker(context.as_deref());
            c.args(["run", "--rm", "--name", &name])
                .args([
                    "--network",
                    "none",
                    "--read-only",
                    "--tmpfs",
                    "/tmp:rw,exec,size=512m",
                ])
                .args([
                    "--memory",
                    memory,
                    "--memory-swap",
                    memory,
                    "--cpus",
                    cpus,
                    "--pids-limit",
                    "256",
                ])
                .args(["--cap-drop", "ALL", "--security-opt", "no-new-privileges"])
                .args(["--user", &format!("{}:{}", meta.uid(), meta.gid())])
                .args([
                    "-v",
                    &format!("{}:/work", work.display()),
                    "-v",
                    &format!("{}:/target", target.display()),
                ])
                .args([
                    "-e",
                    "CARGO_TARGET_DIR=/target",
                    "-e",
                    // as for host runs; overrides the image's setting
                    "CARGO_INCREMENTAL=0",
                    "-e",
                    "PYTHONDONTWRITEBYTECODE=1",
                    "-e",
                    "PYTHONUNBUFFERED=1",
                    "-w",
                    "/work",
                    image,
                ])
                .arg(match program {
                    Program::Cargo => "cargo".to_owned(),
                    Program::Built(rel) => format!("/target/{rel}"),
                    Program::Python => "python3".to_owned(),
                })
                .args(args);
            container = Some((name, context.clone()));
            c
        }
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| RunnerError::io("start cargo", e))?;
    let mut out = child.stdout.take().expect("stdout is piped");
    let mut err = child.stderr.take().expect("stderr is piped");
    // Read both pipes while waiting, so a chatty build can't fill one and deadlock.
    let read_out = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf).await;
        buf
    });
    let read_err = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf).await;
        buf
    });

    let mut exit_code = None;
    let timed_out = match tokio::time::timeout(limit, child.wait()).await {
        Ok(status) => {
            exit_code = status.map_err(|e| RunnerError::io("wait for cargo", e))?.code();
            false
        }
        Err(_) => {
            // Killing `docker run` doesn't stop the container, so stop it by name.
            if let Some((name, context)) = &container {
                let _ = docker(context.as_deref())
                    .args(["kill", name])
                    .output()
                    .await;
            }
            if let (Sandbox::Host, Some(pid)) = (sandbox, child.id()) {
                // `--` so the negative pid means the process group; procps-ng 4 ignores `kill -KILL -<pgid>`.
                let _ = Command::new("kill")
                    .args(["-s", "KILL", "--", &format!("-{pid}")])
                    .output()
                    .await;
            }
            let _ = child.kill().await;
            true
        }
    };
    let stdout = read_out.await.unwrap_or_default();
    let stderr = read_err.await.unwrap_or_default();
    Ok(Captured {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out,
        exit_code,
    })
}

/// `docker`, pinned to a context when one is given.
fn docker(context: Option<&str>) -> Command {
    let mut c = Command::new("docker");
    if let Some(ctx) = context {
        c.args(["--context", ctx]);
    }
    c
}
