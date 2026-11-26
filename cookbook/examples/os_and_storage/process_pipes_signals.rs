//! RECIPE 174 (os): child processes, pipes, exit status vs signals, timeouts, and async-signal-safe signal handlers
//! LEVEL: intermediate
//! Use in: task runners, CI agents, supervisors, CLIs that shell out, daemons that need clean shutdown.   Run: cargo run --release --example process_pipes_signals
#![allow(dead_code)]
//!
//! WHAT: `std::process::Command` spawns children (fork+exec on Unix). Each of stdin/stdout/stderr can be
//! inherited, piped, or nulled. `output()` runs to completion and captures; `spawn()` gives a `Child` you
//! drive yourself. An `ExitStatus` is either "exited with code N" or "killed by signal S"
//! (`ExitStatusExt::signal()`); `code()` is `None` in the second case, so never `unwrap` it.
//!
//! PIPE DEADLOCKS: a pipe has a small kernel buffer (64 KiB on Linux, 16-64 KiB on macOS). If you write a
//! large input to a child's stdin from the same thread that should be draining its stdout, the child blocks
//! writing stdout (full), you block writing stdin (full): deadlock. Fix: write stdin from another thread
//! (or use async IO), read stdout on this one, and DROP stdin so the child sees EOF.
//!
//! TIMEOUTS: std has no `wait_timeout`; poll `try_wait()` with a short sleep, then `kill()` + `wait()`
//! (always reap, or you leak a zombie).
//!
//! SIGNALS: a handler runs asynchronously, interrupting any code, possibly inside `malloc`. So it may only
//! call async-signal-safe functions: set an atomic, write to a pipe, `_exit`. No allocation, no locks,
//! no `println!`. The pattern here: handler sets `AtomicBool`/`AtomicI32`; the main loop polls it.
//! We install without SA_RESTART so blocking syscalls return EINTR and the loop notices promptly.
//!
//! TOKIO ALTERNATIVE: `tokio::signal::unix::signal(SignalKind::terminate())` returns a stream; internally
//! tokio installs a tiny handler that writes to a self-pipe, and a task wakes up and delivers events, so you
//! can `select!` on `sig.recv()` and never write unsafe code:
//!     let mut term = signal(SignalKind::terminate())?;
//!     tokio::select! { _ = term.recv() => shutdown().await, _ = work() => {} }
//! and `tokio::signal::ctrl_c().await` for SIGINT. `tokio::process::Command` mirrors std's API with async
//! `wait()` and async pipes (no thread needed to avoid the deadlock).
//!
//! COMMON USES: build systems and CI runners spawning and timing out jobs; process supervisors restarting
//! children; shells' pipelines; servers draining connections on SIGTERM (Kubernetes sends SIGTERM, waits
//! a grace period, then SIGKILL); wrapping external tools (git, ffmpeg) and capturing their output.
//! Read: https://doc.rust-lang.org/std/process/index.html and https://man7.org/linux/man-pages/man7/signal-safety.7.html
use std::io::{Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
static LAST_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// The handler: ONLY atomics. Atomic stores are async-signal-safe (lock-free).
extern "C" fn on_signal(sig: libc::c_int) {
    LAST_SIGNAL.store(sig, Ordering::SeqCst);
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn install_handler(sig: libc::c_int) {
    // SAFETY: we zero a plain-old-data `sigaction`, fill the fields we need, and pass valid pointers.
    // `on_signal` is `extern "C"` and async-signal-safe (atomic stores only).
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0; // no SA_RESTART: blocking calls return EINTR
        let rc = libc::sigaction(sig, &sa, std::ptr::null_mut());
        assert_eq!(rc, 0, "sigaction failed");
    }
}

/// Run `cmd` to completion or kill it after `limit`. `None` = timed out (and killed + reaped).
fn run_with_timeout(cmd: &mut Command, limit: Duration) -> std::io::Result<Option<ExitStatus>> {
    let mut child = cmd.spawn()?;
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?; // reap: avoid a zombie
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn sh(script: &str) -> Command {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(script);
    c
}

fn main() -> std::io::Result<()> {
    // 1. capture output
    let out = Command::new("echo").arg("hello").output()?;
    assert!(out.status.success());
    assert_eq!(out.stdout, b"hello\n");
    println!(
        "1. output() captured {:?}",
        String::from_utf8_lossy(&out.stdout)
    );

    // 2. exit codes and stderr
    let out = sh("echo oops >&2; exit 3").output()?;
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(out.stderr, b"oops\n");
    assert!(out.stdout.is_empty());
    println!(
        "2. exit code {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    // 3. killed by signal: code() is None, signal() is Some
    let st = sh("kill -TERM $$").status()?;
    assert_eq!(st.code(), None);
    assert_eq!(st.signal(), Some(libc::SIGTERM));
    assert!(!st.success());
    println!("3. child died from signal {:?}", st.signal());

    // 4. env and working dir
    let dir = std::env::temp_dir().join(format!("cb_r174_{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let out = sh("echo \"$GREETING\"").env("GREETING", "howdy").output()?;
    assert_eq!(out.stdout, b"howdy\n");
    let out = sh("echo \"[$GREETING]\"").env_clear().output()?; // env_clear: child sees nothing
    assert_eq!(out.stdout, b"[]\n");
    let out = sh("pwd").current_dir(&dir).output()?;
    let printed = String::from_utf8(out.stdout).unwrap();
    // macOS temp dirs are symlinks (/var -> /private/var), so compare canonical paths
    assert_eq!(
        std::fs::canonicalize(printed.trim())?,
        std::fs::canonicalize(&dir)?
    );
    println!("4. env + current_dir honoured ({})", printed.trim());

    // 5. kill a long-running child
    let mut child = Command::new("sleep").arg("30").spawn()?;
    assert!(child.try_wait()?.is_none(), "still running");
    child.kill()?;
    let st = child.wait()?;
    assert_eq!(st.signal(), Some(libc::SIGKILL));
    println!("5. killed sleep: signal {:?}", st.signal());

    // 6. a pipeline wired by hand: printf | sort
    let mut a = Command::new("printf")
        .arg("b\\na\\nc\\n")
        .stdout(Stdio::piped())
        .spawn()?;
    let b = Command::new("sort")
        .stdin(Stdio::from(a.stdout.take().unwrap())) // a's stdout fd becomes b's stdin
        .stdout(Stdio::piped())
        .spawn()?;
    let out = b.wait_with_output()?;
    assert!(a.wait()?.success());
    assert_eq!(out.stdout, b"a\nb\nc\n");
    println!(
        "6. pipeline printf | sort -> {:?}",
        String::from_utf8_lossy(&out.stdout)
    );

    // 7. avoid the deadlock: 4 MiB through `cat`, stdin written from a thread
    let input: Vec<u8> = (0..4 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let mut cat = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut stdin = cat.stdin.take().unwrap();
    let mut stdout = cat.stdout.take().unwrap();
    let to_write = input.clone();
    let writer = thread::spawn(move || {
        stdin.write_all(&to_write).unwrap();
        // `stdin` dropped here => EOF for cat
    });
    let mut echoed = Vec::new();
    stdout.read_to_end(&mut echoed)?; // drains concurrently with the writer
    writer.join().unwrap();
    assert!(cat.wait()?.success());
    assert_eq!(echoed, input);
    println!(
        "7. {} bytes round-tripped through cat without deadlock",
        echoed.len()
    );

    // 8. timeouts via try_wait polling
    let t = Instant::now();
    let r = run_with_timeout(Command::new("sleep").arg("30"), Duration::from_millis(150))?;
    assert!(r.is_none(), "should time out");
    assert!(t.elapsed() < Duration::from_secs(5));
    let r = run_with_timeout(&mut sh("exit 7"), Duration::from_secs(10))?;
    assert_eq!(r.and_then(|s| s.code()), Some(7));
    println!(
        "8. slow child killed after {:?}; fast child finished",
        t.elapsed()
    );

    // 9. signals: SIGINT then SIGTERM delivered to ourselves; handler only sets atomics
    install_handler(libc::SIGINT);
    install_handler(libc::SIGTERM);
    assert!(!SHUTDOWN.load(Ordering::SeqCst));

    // a worker loop that does "work" until told to stop, signalled from another thread
    let worker = thread::spawn(|| {
        let mut ticks = 0u64;
        while !SHUTDOWN.load(Ordering::SeqCst) {
            ticks += 1;
            thread::sleep(Duration::from_millis(2));
            assert!(ticks < 5_000, "shutdown signal never observed");
        }
        // graceful part: flush, close connections, etc.
        ticks
    });
    thread::sleep(Duration::from_millis(30));
    // SAFETY: kill(2) on our own pid with a signal we have a handler for is sound.
    assert_eq!(unsafe { libc::kill(libc::getpid(), libc::SIGTERM) }, 0);
    let ticks = worker.join().unwrap();
    assert_eq!(LAST_SIGNAL.load(Ordering::SeqCst), libc::SIGTERM);
    println!("9. worker saw SIGTERM after {ticks} ticks and exited gracefully");

    // second signal via raise (delivered synchronously to this thread)
    SHUTDOWN.store(false, Ordering::SeqCst);
    // SAFETY: raise(3) with a handled signal just runs our handler on this thread.
    assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
    assert!(SHUTDOWN.load(Ordering::SeqCst));
    assert_eq!(LAST_SIGNAL.load(Ordering::SeqCst), libc::SIGINT);
    println!("   raise(SIGINT) set the flag; main loop would now shut down");

    // a child does NOT inherit our handler (exec resets handlers) so it dies by default action
    let st = sh("kill -INT $$").status()?;
    assert_eq!(st.signal(), Some(libc::SIGINT));

    // restore defaults so later code behaves normally
    // SAFETY: SIG_DFL is always a valid disposition.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }
    std::fs::remove_dir_all(&dir)?;
    println!("all process/signal assertions passed");
    Ok(())
}
