//! RECIPE 1009 (std): panics (payloads, catch_unwind, hooks, drop order), ending a process (exit, abort, ExitCode) and
//! running child processes with std::process::Command.
//! LEVEL: intermediate
//! Use in: worker isolation, test harnesses, CLI exit codes, running external tools with pipes and timeouts.   Run: cargo run --release --example std_panic_process_exit_catch
//! COMMON USES: keeping a server alive when one request handler panics, crash reporting hooks, `git`/`ffmpeg`
//! wrappers, health-check commands, supervising a subprocess.
//! METHODS: (everyday first) std::panic, std::assert, std::unreachable, Option::expect, Result::unwrap, std::process::Command::new, Command::arg,
//! METHODS: Command::args, Command::output, Command::status, Command::spawn, Command::stdin, Command::stdout, Command::stderr, Command::env,
//! METHODS: Command::envs, Command::current_dir, Stdio::piped, Stdio::null, Child::wait, Child::wait_with_output, Child::try_wait, Child::kill,
//! METHODS: Child::id, ExitStatus::success, ExitStatus::code, std::os::unix::process::ExitStatusExt::signal, std::process::ExitCode,
//! METHODS: std::process::Termination, std::process::exit, std::process::abort, std::process::id, std::panic::catch_unwind,
//! METHODS: std::panic::AssertUnwindSafe, std::panic::resume_unwind, std::panic::panic_any, std::panic::set_hook, std::panic::take_hook,
//! METHODS: PanicHookInfo::payload, PanicHookInfo::payload_as_str, PanicHookInfo::location, std::panic::Location::caller, std::thread::panicking,
//! METHODS: JoinHandle::join, Command::get_program, Command::get_args, Mutex::is_poisoned, PoisonError::into_inner, Mutex::clear_poison
//! DOCS: https://doc.rust-lang.org/std/panic/index.html
//! DEFINED IN: panic!/assert!/unreachable! and `Location` are core; catch_unwind, hooks, resume_unwind, panic_any, process
//! (Command, exit, abort, ExitCode, Termination), thread and Mutex are std. `AssertUnwindSafe` is core (`core::panic`).
//! Also: https://doc.rust-lang.org/std/process/index.html
//! NOT COVERED (and why): always_abort, update_hook, ExitStatus::exit_ok (unstable), the unix `pre_exec` (unsafe), uid/gid/
//! process_group extensions, io::pipe (see the io pages), `#[panic_handler]` (no_std only), panic=abort cannot be
//! demonstrated inside one build profile (described below).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   a bug / broken invariant                  -> panic! / assert! / unreachable! / expect("why it holds")
//!   an expected failure (input, I/O)          -> Result
//!   isolate a panic in a worker               -> thread::spawn(..).join()  or catch_unwind
//!   re-raise after cleanup                    -> resume_unwind(payload)
//!   read the message from a payload           -> downcast_ref::<&str>() then ::<String>()
//!   log every panic once                      -> set_hook (process-wide)
//!   set the exit status                       -> return ExitCode from main (destructors run)
//!   quit from deep inside (no destructors)    -> process::exit(code)
//!   die immediately                           -> process::abort()
//!   run and capture output                    -> Command::output()
//!   run, inherit the terminal                 -> Command::status()
//!   stream or feed a child                    -> spawn() with Stdio::piped(), then wait()
//!
//! PANIC STRATEGY: by default a panic UNWINDS the thread, running destructors. With `panic = "abort"` in a Cargo profile
//! the process aborts after the hook runs: no destructors, catch_unwind catches nothing, a panicking thread takes the whole
//! process down. So a library must not rely on catch_unwind for correctness.
//!
//! Read: https://doc.rust-lang.org/std/panic/index.html , https://doc.rust-lang.org/std/process/index.html ,
//!       https://doc.rust-lang.org/nomicon/unwinding.html
//! TRY NEXT:
//!   - [S] Remove the status check from `pipe_through`, predict which assert in `section_pipes` fails, then run it.
//!   - [S] Add `panic = "abort"` to the release profile in a scratch copy. Which asserts are never reached?
//!   - [M] Write `run_with_timeout` using a waiter thread and a channel instead of polling try_wait; compare the designs.
//!   - [M] Feed 10 MB to `cat` writing stdin on the SAME thread and explain the hang using pipe buffer sizes.
//!   - [A] PROJECT: a job runner: commands from std_env_args_vars's parser, timeouts from this page, bounded output capture
//!         (std_io_read_write_seek_traits), and a typed JobError (std_any_error_marker_traits).
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::panic::{self, AssertUnwindSafe, Location};
use std::process::{Command, ExitCode, Stdio, Termination};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const COVERED: &[&str] = &[
    "panic",
    "assert",
    "unreachable",
    "expect",
    "unwrap",
    "new",
    "arg",
    "args",
    "output",
    "status",
    "spawn",
    "stdin",
    "stdout",
    "stderr",
    "env",
    "envs",
    "current_dir",
    "piped",
    "null",
    "wait",
    "wait_with_output",
    "try_wait",
    "kill",
    "id",
    "success",
    "code",
    "signal",
    "ExitCode",
    "Termination",
    "exit",
    "abort",
    "catch_unwind",
    "AssertUnwindSafe",
    "resume_unwind",
    "panic_any",
    "set_hook",
    "take_hook",
    "payload",
    "payload_as_str",
    "location",
    "caller",
    "panicking",
    "join",
    "get_program",
    "get_args",
    "is_poisoned",
    "into_inner",
    "clear_poison",
];

const MODE: &str = "STD_PANIC_COOKBOOK_MODE";

fn message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic>")
}

// ---- 1. Everyday core: Result for failures, panic for bugs -----------------------------------------------------
/// Untrusted input: Result.
fn parse_percent(s: &str) -> Result<u8, String> {
    match s.parse::<u8>() {
        Ok(p) if p <= 100 => Ok(p),
        _ => Err(format!("not a percentage: {s:?}")),
    }
}
/// A caller bug: panic. #[track_caller] makes the panic point at the CALLER's line.
#[track_caller]
fn percent_of(total: u64, percent: u8) -> u64 {
    assert!(
        percent <= 100,
        "percent {percent} out of range: validate with parse_percent first"
    );
    total * u64::from(percent) / 100
}
#[track_caller]
fn where_called() -> &'static Location<'static> {
    Location::caller()
}

fn section_payloads() {
    assert_eq!(parse_percent("40"), Ok(40));
    assert!(parse_percent("400").is_err());
    assert_eq!(percent_of(200, 50), 100);
    let (here, line) = (where_called(), line!());
    assert_eq!((here.line(), here.file()), (line, file!()));

    let p = panic::catch_unwind(|| percent_of(1, std::hint::black_box(101))).unwrap_err();
    assert!(message(&*p).starts_with("percent 101 out of range"));
    let p = panic::catch_unwind(|| panic!("static text"))
        .map(|(): ()| ())
        .unwrap_err();
    assert_eq!(p.downcast_ref::<&str>(), Some(&"static text"));
    let n = std::hint::black_box(7);
    let p = panic::catch_unwind(|| -> () { panic!("formatted {n}") }).unwrap_err();
    assert_eq!(
        p.downcast_ref::<String>().map(String::as_str),
        Some("formatted 7")
    );
    // expect puts your sentence first, then the error's Debug.
    let p =
        panic::catch_unwind(|| "x".parse::<u8>().expect("port was validated earlier")).unwrap_err();
    assert!(message(&*p).starts_with("port was validated earlier: ParseIntError"));
    let p = panic::catch_unwind(|| None::<u8>.expect("slot filled in new()")).unwrap_err();
    assert_eq!(message(&*p), "slot filled in new()");
    let p = panic::catch_unwind(|| Err::<(), u8>(3).unwrap()).unwrap_err();
    assert!(message(&*p).contains("`Result::unwrap()` on an `Err` value: 3"));
    let p = panic::catch_unwind(|| -> () { unreachable!("state {}", std::hint::black_box(9)) })
        .unwrap_err();
    assert_eq!(
        message(&*p),
        "internal error: entered unreachable code: state 9"
    );
    // panic_any: any Send + 'static value as the payload.
    #[derive(Debug, PartialEq)]
    struct Cancelled(u32);
    let p = panic::catch_unwind(|| -> () { panic::panic_any(Cancelled(5)) }).unwrap_err();
    assert_eq!(p.downcast_ref::<Cancelled>(), Some(&Cancelled(5)));
    println!("1. payloads ok");
}

// ---- 2. catch_unwind, AssertUnwindSafe, resume_unwind ----------------------------------------------------------
fn handle(requests: &[i32]) -> (Vec<Result<i32, String>>, u32) {
    let mut processed = 0;
    let out = requests
        .iter()
        .map(|&r| {
            // The closure mutates a captured &mut, which is not UnwindSafe: we assert that a half-finished update
            // cannot leave `processed` inconsistent (it is updated only after the work succeeded).
            panic::catch_unwind(AssertUnwindSafe(|| {
                let v = 100 / r; // panics for r == 0
                processed += 1;
                v
            }))
            .map_err(|p| message(&*p).to_string())
        })
        .collect();
    (out, processed)
}

fn section_catch() {
    let (out, n) = handle(&[5, 0, 20]);
    assert_eq!(
        out,
        [Ok(20), Err("attempt to divide by zero".to_string()), Ok(5)]
    );
    assert_eq!(n, 2);
    // resume_unwind continues the SAME panic without running the hook again.
    let outer = panic::catch_unwind(|| {
        let inner = panic::catch_unwind(|| -> () { panic!("original") });
        if let Err(p) = inner {
            panic::resume_unwind(p);
        }
    });
    assert_eq!(message(&*outer.unwrap_err()), "original");
    println!("2. catch_unwind ok");
}

// ---- 3. Panic hooks --------------------------------------------------------------------------------------------
fn section_hooks() {
    // The hook is PROCESS-WIDE and runs before unwinding, even for panics that are later caught.
    let seen: Arc<Mutex<Vec<(String, u32, bool)>>> = Arc::default();
    let sink = seen.clone();
    let previous = panic::take_hook(); // take_hook also unregisters: the default hook is back until set_hook
    panic::set_hook(Box::new(move |info| {
        // No asserts in here: a panic inside the hook aborts the process.
        let msg = info.payload_as_str().unwrap_or("?").to_string(); // since 1.91; older: payload().downcast_ref
        let text_payload = info.payload().downcast_ref::<&str>().is_some()
            || info.payload().downcast_ref::<String>().is_some();
        let line = info.location().map_or(0, Location::line);
        sink.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((msg, line, std::thread::panicking() && text_payload));
    }));
    let expected_line = line!() + 1;
    let _ = panic::catch_unwind(|| panic!("captured"));
    let _ = panic::catch_unwind(|| panic::resume_unwind(Box::new("silent"))); // no hook call
    panic::set_hook(previous);
    assert_eq!(
        *seen.lock().unwrap(),
        [("captured".to_string(), expected_line, true)]
    );
    assert!(!std::thread::panicking());
    println!("3. hooks ok");
}

// ---- 4. Drop order during unwinding, threads, poisoning --------------------------------------------------------
struct Noisy(&'static str, Rc<RefCell<Vec<String>>>);
impl Drop for Noisy {
    fn drop(&mut self) {
        self.1
            .borrow_mut()
            .push(format!("{} panicking={}", self.0, std::thread::panicking()));
    }
}

fn section_unwinding() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let _calm = Noisy("calm", log.clone());
    }
    let l = log.clone();
    let r = panic::catch_unwind(AssertUnwindSafe(move || {
        let _first = Noisy("first", l.clone());
        let _second = Noisy("second", l.clone());
        fn deeper(l: Rc<RefCell<Vec<String>>>) {
            let _inner = Noisy("inner", l);
            panic!("boom");
        }
        deeper(l.clone());
    }));
    assert!(r.is_err());
    // Innermost frame first; within a frame, reverse declaration order.
    assert_eq!(
        *log.borrow(),
        [
            "calm panicking=false",
            "inner panicking=true",
            "second panicking=true",
            "first panicking=true"
        ]
    );
    // A Drop that panics DURING unwinding aborts the process, so Drop impls must not panic.

    // A panic in a spawned thread ends only that thread; join hands you the payload.
    let h = std::thread::spawn(|| -> u32 { panic!("worker died") });
    assert_eq!(message(&*h.join().unwrap_err()), "worker died");
    // A panic while holding a MutexGuard poisons the mutex: later lock() returns Err.
    let m = Arc::new(Mutex::new(vec![1]));
    let m2 = m.clone();
    let _ = std::thread::spawn(move || {
        let mut g = m2.lock().unwrap();
        g.push(2);
        panic!("mid-update");
    })
    .join();
    assert!(m.is_poisoned());
    let data = m
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert_eq!(data, [1, 2]); // the partial update is visible: repair it, then clear the flag
    m.clear_poison(); // since 1.77
    assert!(m.lock().is_ok());
    println!("4. unwinding, threads, poisoning ok");
}

// ---- 5. Exit codes, observed from a parent ---------------------------------------------------------------------
struct PrintOnDrop;
impl Drop for PrintOnDrop {
    fn drop(&mut self) {
        println!("dropped");
    }
}
/// Termination (stable since 1.61) can be implemented for your own outcome type; `main` or a helper may return it.
enum Outcome {
    Done,
    Usage,
}
impl Termination for Outcome {
    fn report(self) -> ExitCode {
        match self {
            Outcome::Done => ExitCode::SUCCESS,
            Outcome::Usage => ExitCode::from(2), // ExitCode::from(u8)
        }
    }
}

/// What the re-spawned copy of this binary does. `scratch` is a file path for the flush demonstration.
fn child(mode: &str) -> ExitCode {
    let _guard = PrintOnDrop;
    match mode {
        "code" => Outcome::Usage.report(), // returns normally: destructors run
        "ok" => Outcome::Done.report(),
        "exit" => std::process::exit(7),
        "abort" => std::process::abort(),
        "panic" => panic!("child panic"),
        "unflushed" => {
            let path = std::env::var_os("SCRATCH").expect("SCRATCH");
            let mut w = io::BufWriter::new(std::fs::File::create(path).expect("create"));
            w.write_all(b"important").expect("buffered write");
            std::process::exit(0) // BufWriter's destructor never runs
        }
        _ => ExitCode::FAILURE,
    }
}

fn run_self(mode: &str) -> std::process::Output {
    Command::new(std::env::current_exe().expect("exe"))
        .env(MODE, mode)
        .env("RUST_BACKTRACE", "0")
        .output()
        .expect("spawn self")
}

fn section_exit() {
    let o = run_self("ok");
    assert_eq!(
        (o.status.code(), o.stdout.as_slice()),
        (Some(0), &b"dropped\n"[..])
    );
    let o = run_self("code");
    assert_eq!(
        (o.status.code(), o.stdout.as_slice()),
        (Some(2), &b"dropped\n"[..])
    );
    let o = run_self("exit");
    assert_eq!((o.status.code(), o.stdout.len()), (Some(7), 0)); // no destructor ran
    let o = run_self("panic");
    assert_eq!(o.status.code(), Some(101)); // uncaught panic on the main thread
    assert_eq!(o.stdout, b"dropped\n");
    assert!(String::from_utf8_lossy(&o.stderr).contains("child panic"));
    let o = run_self("abort");
    assert!(!o.status.success() && o.stdout.is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // Killed by a signal: there is no exit code.
        assert_eq!((o.status.code(), o.status.signal()), (None, Some(6))); // SIGABRT
    }
    let path = std::env::temp_dir().join(format!("cookbook-unflushed-{}", std::process::id()));
    let st = Command::new(std::env::current_exe().expect("exe"))
        .env(MODE, "unflushed")
        .env("SCRATCH", &path)
        .status()
        .expect("spawn");
    assert!(st.success(), "the child 'succeeded'");
    assert_eq!(
        std::fs::read(&path).expect("file exists"),
        b"",
        "...and the data is gone"
    );
    std::fs::remove_file(&path).expect("cleanup");
    // main() -> Result<(), E>: Ok exits 0; Err prints `Error: {e:?}` to stderr and exits with failure.
    let _code: ExitCode = Ok::<(), String>(()).report(); // Result<(), E: Debug> implements Termination too
    println!("5. exit codes ok");
}

// ---- 6. Running programs ---------------------------------------------------------------------------------------
#[cfg(unix)]
fn sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(script);
    c
}

#[cfg(unix)]
fn section_command() -> io::Result<()> {
    let out = sh("echo out; echo err >&2; exit 3").output()?; // Ok even though the program failed
    assert_eq!((out.status.code(), out.status.success()), (Some(3), false));
    assert_eq!(
        (out.stdout.as_slice(), out.stderr.as_slice()),
        (&b"out\n"[..], &b"err\n"[..])
    );
    // Err means the program could not be STARTED.
    let missing = Command::new("/definitely/not/here").output().unwrap_err();
    assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    // status(): stdio is inherited unless redirected.
    assert!(
        sh("exit 0")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
    );

    let dir = std::env::temp_dir().canonicalize()?;
    let out = sh("echo \"$A-$B\"; pwd -P")
        .env("A", "1")
        .envs([("B", "2")])
        .current_dir(&dir)
        .output()?;
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("1-2\n{}\n", dir.display())
    );
    // Pass data as ARGUMENTS, never by pasting it into the script text.
    let hostile = "x; echo INJECTED";
    let out = sh("printf '%s' \"$1\"").args(["sh", hostile]).output()?;
    assert_eq!(out.stdout, hostile.as_bytes());
    let mut c = Command::new("echo");
    c.args(["a", "b"]);
    assert_eq!(
        (c.get_program(), c.get_args().count()),
        (std::ffi::OsStr::new("echo"), 2)
    );
    println!("6. Command basics ok");
    Ok(())
}

// ---- 7. Pipes and timeouts -------------------------------------------------------------------------------------
#[cfg(unix)]
fn pipe_through(program: &str, input: Vec<u8>) -> io::Result<Vec<u8>> {
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped");
    // Write from another thread: if the child fills its stdout pipe while we are still writing, a single thread deadlocks.
    let writer = std::thread::spawn(move || {
        let r = stdin.write_all(&input);
        drop(stdin); // closing stdin is how the child sees end-of-file
        r
    });
    let out = child.wait_with_output()?; // reads stdout to the end, then reaps the child
    let written = writer.join().expect("writer thread");
    // A program that ran and failed is still `Ok` from `wait_with_output`: check the status yourself.
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "{program} failed: {}",
            out.status
        )));
    }
    written?; // a child that stops reading early shows up here as a broken pipe
    Ok(out.stdout)
}

/// None means the deadline passed and the child was killed. Do not give `cmd` a piped stdout or stderr: nothing
/// here reads it, so a child that fills the pipe waits until it is killed.
fn run_with_timeout(
    mut cmd: Command,
    limit: Duration,
) -> io::Result<Option<std::process::ExitStatus>> {
    let mut child = cmd.spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= limit {
            child.kill()?; // SIGKILL on unix; grandchildren are NOT killed
            child.wait()?; // reap it: std never waits for you, not even on drop
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn section_pipes() -> io::Result<()> {
    let big: Vec<u8> = (0..1_000_000u32).map(|i| b'a' + (i % 26) as u8).collect();
    assert!(pipe_through("cat", big.clone())? == big); // far larger than a pipe buffer
    assert!(
        pipe_through("false", Vec::new()).is_err(),
        "a failed program is not a result"
    );
    // Streaming: read the pipe yourself, then wait.
    let mut child = sh("printf 'a\\nb\\n'")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    assert!(child.id() != std::process::id());
    let mut text = String::new();
    child
        .stdout
        .take()
        .expect("piped")
        .read_to_string(&mut text)?;
    let status = child.wait()?;
    assert!(status.success() && text == "a\nb\n");
    assert!(child.wait()?.success()); // waiting again returns the same status

    let mut slow = Command::new("sleep");
    slow.arg("30").stdin(Stdio::null()).stdout(Stdio::null());
    let started = Instant::now();
    assert!(run_with_timeout(slow, Duration::from_millis(50))?.is_none());
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "we did not wait for the 30 s sleep"
    );
    let quick = sh("exit 0");
    assert!(run_with_timeout(quick, Duration::from_secs(20))?.is_some_and(|s| s.success()));
    println!("7. pipes and timeouts ok");
    Ok(())
}

// ---- 8. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Libraries return Result; panic only for bugs (clippy::unwrap_used / expect_used if the team wants it enforced).
    // 2. expect messages state why the value MUST be there, not what went wrong.
    // 3. catch_unwind belongs at isolation boundaries (request, job, FFI edge), not as try/catch.
    // 4. Never unwind into C: catch at every `extern "C"` entry point.
    // 5. Install one panic hook at start-up that logs location and message; libraries must not install hooks.
    // 6. Return ExitCode from main; call process::exit only after buffers are flushed and guards dropped (section 5).
    let mut w = io::BufWriter::new(Vec::new());
    w.write_all(b"x").unwrap();
    assert!(w.get_ref().is_empty());
    w.flush().unwrap(); // flush explicitly: errors inside Drop are silently ignored
    assert_eq!(w.get_ref(), b"x");
    // 7. Check status.success() after output(): a failing program is still Ok(..).
    // 8. Always wait() on spawned children, or they stay zombies until the parent exits.
    // 9. Put a timeout on every external command.
    // 10. Avoid `sh -c` with interpolated input; pass arguments directly.
    println!("8. production tips ok");
}

fn main() -> ExitCode {
    if let Ok(mode) = std::env::var(MODE) {
        return child(&mode);
    }
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    // Silence the panics this page triggers on purpose; restore the default hook afterwards.
    panic::set_hook(Box::new(|_| {}));
    let sections = panic::catch_unwind(|| {
        section_payloads();
        section_catch();
        section_hooks(); // swaps in its own hook and puts ours back
        section_unwinding();
    });
    let _quiet = panic::take_hook(); // default hook again: real failures below are printed
    if let Err(p) = sections {
        // the silent hook also hid a real failure: say what it was
        panic!("a section failed: {}", message(&*p));
    }
    section_exit();
    #[cfg(unix)]
    {
        section_command().expect("command section");
        section_pipes().expect("pipe section");
    }
    section_production_tips();
    println!(
        "covered {} panic/process names; all asserts passed",
        COVERED.len()
    );
    ExitCode::SUCCESS
}
