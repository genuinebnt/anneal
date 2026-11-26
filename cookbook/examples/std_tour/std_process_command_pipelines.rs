//! RECIPE 1066 (std_tour): `std::process` by use case: Command, pipes, pipelines, exit statuses, timeouts, signals.
//! LEVEL: intermediate
//! Use in: running git/ffmpeg/compilers, worker subprocesses, CI runners, supervisors.   Run: cargo run --release --example std_process_command_pipelines
//! COMMON USES: capture a tool's output (`output()`), stream data through a filter, build `a | b` without a shell, enforce a timeout, report exit codes.
//! METHODS: Command::new, Command::arg, Command::args, Command::env, Command::envs, Command::env_remove, Command::env_clear, Command::current_dir,
//! Command::stdin, Command::stdout, Command::stderr, Command::spawn, Command::output, Command::status, Command::get_program, Command::get_args,
//! Command::get_envs, Command::get_current_dir, Stdio::piped, Stdio::null, Stdio::inherit, Stdio::from, Child::id, Child::wait, Child::try_wait,
//! Child::kill, Child::wait_with_output, Child::stdin, Child::stdout, Child::stderr, ExitStatus::success, ExitStatus::code, Output::status,
//! std::process::id, std::process::exit, std::process::ExitCode, std::io::pipe, std::env::current_exe,
//! unix: ExitStatusExt::signal, CommandExt::exec, CommandExt::process_group, CommandExt::arg0
//! DOCS: https://doc.rust-lang.org/std/process/index.html
//! DEFINED IN: std only: `std::process`, `std::os::unix::process`, `std::io::pipe`, `std::env`. Processes need an OS; core and alloc have nothing here.
//! NOT COVERED (and why): `CommandExt::pre_exec` (unsafe: only async-signal-safe calls are allowed between fork and exec), `uid`/`gid`/`groups`,
//! Windows `CommandExt` (`creation_flags`, `raw_arg`), `ExitStatus::exit_ok` (unstable), `std::process::abort` (kills this process), async `tokio::process`.
//! Portability: every child is a re-exec of THIS binary (`current_exe` + `--child <mode>`), so no external tool is needed. The shell and signal
//! demonstrations are `cfg(unix)`. Only the macOS build was run when this was written.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   run and capture                            -> `cmd.output()?` then CHECK `out.status.success()` (non-zero exit is NOT an `Err`)
//!   run with the terminal attached             -> `cmd.status()?`
//!   pass user input safely                     -> `.arg(input)`: one argument, no shell, no word splitting. Never `sh -c format!(..)`
//!   input that starts with `-`                 -> put `--` before it (if the tool supports it): `arg` does not stop option parsing
//!   stream into stdin                          -> `Stdio::piped()`, write from ANOTHER THREAD, drop the handle to send EOF
//!   `a | b`                                    -> `b.stdin(Stdio::from(a_child.stdout.take().unwrap()))`
//!   merge stdout and stderr in order           -> one `std::io::pipe()`, writer cloned into both; then drop the `Command`
//!   timeout                                    -> poll `try_wait` until a deadline, then `kill()` AND `wait()`
//!   never leak a child                         -> a guard whose `Drop` kills and waits (`Child` itself does neither)
//!   clean environment                          -> `env_clear()` then add what is needed
//!   exit with a status from `main`             -> return `ExitCode` (destructors run); `process::exit` skips them
//!
//! PITFALLS: writing all of stdin before reading stdout deadlocks once both pipe buffers fill; relative program paths combined with `current_dir` are
//! platform specific; on Windows, arguments are re-joined into one command line, and `.bat` files have special rules (see the module docs);
//! unix keeps only the low 8 bits of an exit code; Rust binaries ignore SIGPIPE, but `Command` resets it to the default in children.
//! TRY NEXT:
//!   - [S] In section 3 remove the writer thread and write 4 MiB before reading. Predict what hangs, then run it with a shell `timeout` guard.
//!   - [S] In section 6, delete `drop(cmd)`. Why does `read_to_string` now never return?
//!   - [M] Extend `run_with_timeout` to capture stdout without deadlocking (reader thread) and return partial output on timeout.
//!   - [M] Add a concurrency limit: run 20 children with at most 4 alive, using `try_wait` or one thread per slot.
//!   - [A] PROJECT: combine with `std_sync_primitives_use_cases` and `std_signals_env_exit_hooks_ctrlc`: a supervisor that restarts a crashing child with backoff and forwards SIGTERM.
//! Read: https://doc.rust-lang.org/std/process/struct.Command.html ; https://doc.rust-lang.org/std/process/struct.Child.html
#![allow(dead_code)]

use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

// ================================================================ child side
struct PrintOnDrop;
impl Drop for PrintOnDrop {
    fn drop(&mut self) {
        println!("destructor ran");
    }
}

/// What the re-executed binary does. Each mode is a tiny stand-in for an external tool.
fn child_main(mode: &str, rest: &[String]) -> io::Result<ExitCode> {
    let out = io::stdout();
    let mut out = out.lock();
    match mode {
        "args" => {
            for a in rest {
                writeln!(out, "{a:?}")?;
            }
        }
        "env" => {
            for name in rest {
                writeln!(
                    out,
                    "{name}={}",
                    std::env::var(name).unwrap_or_else(|_| "<unset>".into())
                )?;
            }
        }
        "cwd" => writeln!(out, "{}", std::env::current_dir()?.display())?,
        "upper" => {
            for line in io::stdin().lock().lines() {
                writeln!(out, "{}", line?.to_uppercase())?;
            }
        }
        "gen" => {
            let n: u32 = rest[0].parse().unwrap();
            for i in 0..n {
                // a producer whose consumer has gone away should stop quietly
                match writeln!(out, "item-{i}") {
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                        return Ok(ExitCode::SUCCESS);
                    }
                    r => r?,
                }
            }
        }
        "grep" => {
            let mut found = false;
            for line in io::stdin().lock().lines() {
                let line = line?;
                if line.ends_with(&rest[0]) {
                    found = true;
                    writeln!(out, "{line}")?;
                }
            }
            return Ok(ExitCode::from(u8::from(!found))); // grep's convention: 1 = no match
        }
        "count" => writeln!(out, "{}", io::stdin().lock().lines().count())?,
        "fail" => {
            writeln!(out, "partial output")?;
            eprintln!("error: something broke");
            out.flush()?;
            std::process::exit(rest[0].parse().unwrap());
        }
        "interleave" => {
            for i in 0..3 {
                writeln!(out, "out {i}")?;
                out.flush()?;
                eprintln!("err {i}");
            }
        }
        "flood" => {
            // lots of data on BOTH streams: a parent that drains only one would deadlock
            let block = "x".repeat(1023);
            for _ in 0..1024 {
                writeln!(out, "{block}")?;
                eprintln!("{block}");
            }
        }
        "hang" => {
            writeln!(out, "ready")?;
            out.flush()?;
            thread::sleep(Duration::from_secs(60)); // safety net: even a leaked child ends eventually
        }
        "drop-return" => {
            let _guard = PrintOnDrop;
            return Ok(ExitCode::from(3));
        }
        "drop-exit" => {
            let _guard = PrintOnDrop;
            out.flush()?;
            std::process::exit(3);
        }
        "pid" => writeln!(out, "{}", std::process::id())?,
        #[cfg(unix)]
        "exec" => {
            use std::os::unix::process::CommandExt;
            writeln!(out, "{}", std::process::id())?;
            out.flush()?; // exec replaces the process image: unflushed user-space buffers are lost
            let err = me("pid").exec(); // only returns on failure
            return Err(err);
        }
        #[cfg(unix)]
        "arg0" => writeln!(out, "{}", std::env::args().next().unwrap_or_default())?,
        other => {
            eprintln!("unknown child mode {other}");
            return Ok(ExitCode::from(64));
        }
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

// ================================================================ parent side
/// A command that re-runs this very executable in child mode.
fn me(mode: &str) -> Command {
    let mut c = Command::new(std::env::current_exe().expect("path of this binary"));
    c.arg("--child").arg(mode);
    c
}

fn stdout_of(c: &mut Command) -> String {
    let o = c.output().expect("spawn");
    assert!(
        o.status.success(),
        "{:?}: {}",
        o.status,
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).expect("utf-8")
}

/// `Child` has no Drop behaviour: without this, an early `?` leaves the process running (and later a zombie).
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill(); // Ok if it already exited
        let _ = self.0.wait(); // reap
    }
}

#[derive(Debug)]
enum Outcome {
    Exited(ExitStatus),
    TimedOut(ExitStatus),
}

fn run_with_timeout(child: &mut Child, limit: Duration) -> io::Result<Outcome> {
    let deadline = Instant::now() + limit;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Outcome::Exited(status));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            return Ok(Outcome::TimedOut(child.wait()?)); // always reap after kill
        }
        thread::sleep(pause);
        pause = (pause * 2).min(Duration::from_millis(20));
    }
}

// ------------------------------------------------------------ 1. output, status, exit codes
fn s01_output_status() {
    let o = me("args").args(["a", "b"]).output().unwrap();
    assert!(o.status.success());
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(o.stdout, b"\"a\"\n\"b\"\n");
    assert!(o.stderr.is_empty());

    // A failing child is still Ok(output). Only a failure to run it is Err.
    let o = me("fail").arg("7").output().unwrap();
    assert!(!o.status.success());
    assert_eq!(o.status.code(), Some(7));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "partial output\n");
    assert!(String::from_utf8_lossy(&o.stderr).contains("something broke"));

    let err = Command::new("definitely-not-a-real-program-1066")
        .output()
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);

    // status(): same, but streams are inherited by default; silence them explicitly here
    let st = me("fail")
        .arg("2")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(st.code(), Some(2));
    // inherit states the default of status/spawn explicitly; it also overrides output()'s default of capturing
    let o = me("gen")
        .arg("0")
        .stderr(Stdio::inherit())
        .output()
        .unwrap();
    assert!(
        o.status.success() && o.stderr.is_empty(),
        "an inherited stream is never captured"
    );

    // output() gives the child a closed stdin by default, so a reading child sees EOF instead of hanging
    assert_eq!(stdout_of(&mut me("count")), "0\n");
    // output() drains stdout and stderr together, so 1 MiB on each does not deadlock
    let o = me("flood").output().unwrap();
    assert!(o.status.success());
    assert_eq!((o.stdout.len(), o.stderr.len()), (1024 * 1024, 1024 * 1024));

    // a typed wrapper: the shape most services want
    fn run_checked(c: &mut Command) -> Result<Vec<u8>, String> {
        let o = c
            .output()
            .map_err(|e| format!("cannot run {:?}: {e}", c.get_program()))?;
        if o.status.success() {
            Ok(o.stdout)
        } else {
            Err(format!(
                "{}: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            ))
        }
    }
    let msg = run_checked(me("fail").arg("5")).unwrap_err();
    assert!(
        msg.contains('5') && msg.ends_with("error: something broke"),
        "{msg}"
    );
    println!("1. output / status ok");
}

// ------------------------------------------------------------ 2. Arguments, environment, working directory
fn s02_args_env_cwd() {
    // Each `arg` arrives as exactly one argument: spaces, quotes, `;`, `$()` and `*` have no special meaning.
    let hostile = [
        "two words",
        "x; echo INJECTED",
        "$(whoami)",
        "*",
        "",
        "--looks-like-a-flag",
        "quote\"s",
    ];
    let got = stdout_of(me("args").args(hostile));
    let want: String = hostile.iter().map(|a| format!("{a:?}\n")).collect();
    assert_eq!(got, want);

    #[cfg(unix)]
    {
        let user_input = "x; echo INJECTED";
        // WRONG: user data spliced into shell SOURCE CODE runs as code
        let bad = stdout_of(
            Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("echo {user_input}")),
        );
        assert_eq!(bad, "x\nINJECTED\n");
        // If a shell is truly needed: fixed script, data as positional parameters, expanded inside quotes
        let ok = stdout_of(Command::new("/bin/sh").args([
            "-c",
            "printf '%s\\n' \"$1\"",
            "sh",
            user_input,
        ]));
        assert_eq!(ok, "x; echo INJECTED\n");
    }

    // env adds to the inherited environment; env_remove and env_clear subtract. Prefer this to std::env::set_var
    // (unsafe since edition 2024 because other threads may be reading the environment).
    let got = stdout_of(
        me("env")
            .args(["APP_MODE", "APP_EXTRA"])
            .env("APP_MODE", "test")
            .envs([("APP_EXTRA", "1")]),
    );
    assert_eq!(got, "APP_MODE=test\nAPP_EXTRA=1\n");
    let got = stdout_of(
        me("env")
            .arg("APP_MODE")
            .env("APP_MODE", "x")
            .env_remove("APP_MODE"),
    );
    assert_eq!(got, "APP_MODE=<unset>\n");
    assert!(
        std::env::var_os("CARGO_MANIFEST_DIR").is_some() || std::env::var_os("PATH").is_some(),
        "need some inherited variable"
    );
    #[cfg(unix)]
    {
        assert_eq!(
            stdout_of(me("env").arg("PATH").env_clear()),
            "PATH=<unset>\n"
        );
        assert_eq!(
            stdout_of(me("env").arg("PATH").env_clear().env("PATH", "/bin")),
            "PATH=/bin\n"
        );
    }

    let dir = std::env::temp_dir().canonicalize().unwrap();
    let got = stdout_of(me("cwd").current_dir(&dir));
    assert_eq!(
        std::path::Path::new(got.trim()).canonicalize().unwrap(),
        dir
    );
    let err = me("cwd")
        .current_dir(dir.join("no-such-dir-1066"))
        .output()
        .unwrap_err();
    assert_eq!(
        err.kind(),
        io::ErrorKind::NotFound,
        "a bad cwd fails at spawn"
    );

    // Command builders can be unit tested without running anything
    let mut c = Command::new("git");
    c.args(["log", "-n", "1"])
        .env("GIT_PAGER", "cat")
        .env_remove("GIT_DIR")
        .current_dir("/repo");
    assert_eq!(c.get_program(), "git");
    assert_eq!(c.get_args().collect::<Vec<_>>(), ["log", "-n", "1"]);
    let envs: Vec<_> = c
        .get_envs()
        .map(|(k, v)| (k.to_str().unwrap(), v.map(|v| v.to_str().unwrap())))
        .collect();
    assert_eq!(envs, [("GIT_DIR", None), ("GIT_PAGER", Some("cat"))]); // None = explicitly removed
    assert_eq!(c.get_current_dir(), Some(std::path::Path::new("/repo")));
    println!("2. args / env / cwd ok");
}

// ------------------------------------------------------------ 3. Feeding stdin while reading stdout
fn filter_through(mut cmd: Command, input: Vec<u8>) -> io::Result<Vec<u8>> {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    // The writer must run concurrently with the reader. Otherwise: the child blocks writing stdout (pipe full),
    // so it stops reading stdin, so the parent blocks writing stdin. Neither ever proceeds.
    let writer = thread::spawn(move || stdin.write_all(&input)); // `stdin` drops here: the child sees EOF
    let output = child.wait_with_output()?; // drains stdout/stderr, then reaps
    writer.join().expect("writer thread")?;
    assert!(output.status.success());
    Ok(output.stdout)
}

fn s03_stdin_stdout() {
    let input = "hello pipe\n".repeat(400_000).into_bytes(); // ~4.4 MB, far beyond any pipe buffer
    let out = filter_through(me("upper"), input.clone()).unwrap();
    assert_eq!(out.len(), input.len());
    assert!(out.eq_ignore_ascii_case(&input) && out.starts_with(b"HELLO PIPE\n"));

    // interactive request/response: one line in, one line out. Works only because the child answers line by line AND flushes
    // (Rust's stdout flushes on newline even into a pipe; many C programs switch to block buffering and would stall here).
    let mut child = KillOnDrop(
        me("upper")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut tx = child.0.stdin.take().unwrap();
    let mut rx = BufReader::new(child.0.stdout.take().unwrap()).lines();
    for word in ["one", "two", "three"] {
        writeln!(tx, "{word}").unwrap();
        assert_eq!(rx.next().unwrap().unwrap(), word.to_uppercase());
    }
    drop(tx); // EOF
    assert!(rx.next().is_none());
    assert!(child.0.wait().unwrap().success());

    // wait() closes the child's stdin first, so a child waiting for EOF can finish
    let mut child = me("count")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    assert!(child.stdin.is_some());
    assert!(child.wait().unwrap().success());
    println!("3. stdin + stdout without deadlock ok");
}

// ------------------------------------------------------------ 4. Pipelines without a shell
fn s04_pipeline() {
    // gen 100000 | grep 7 | count
    let mut producer = me("gen")
        .arg("100000")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut filter = me("grep")
        .arg("7")
        .stdin(Stdio::from(producer.stdout.take().unwrap()))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let last = me("count")
        .stdin(Stdio::from(filter.stdout.take().unwrap()))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let out = last.wait_with_output().unwrap();
    // every stage's status matters (a shell without `pipefail` reports only the last one)
    let statuses = [producer.wait().unwrap(), filter.wait().unwrap(), out.status];
    assert!(statuses.iter().all(ExitStatus::success), "{statuses:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "10000\n"); // numbers below 100000 ending in 7

    // early exit downstream: the producer gets a broken pipe and must stop instead of producing a billion lines
    let mut producer = me("gen")
        .arg("1000000000")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut first = String::new();
        BufReader::new(producer.stdout.take().unwrap())
            .read_line(&mut first)
            .unwrap();
        assert_eq!(first, "item-0\n");
    } // the read end closes here
    match run_with_timeout(&mut producer, Duration::from_secs(30)).unwrap() {
        Outcome::Exited(st) => assert!(st.success(), "child handled BrokenPipe: {st:?}"),
        Outcome::TimedOut(_) => panic!("producer ignored the closed pipe"),
    }
    println!("4. pipelines ok");
}

// ------------------------------------------------------------ 5. Timeouts, kill, zombies
fn s05_timeouts() {
    let mut quick = me("gen").arg("1").stdout(Stdio::null()).spawn().unwrap();
    assert!(quick.id() != std::process::id());
    assert!(
        matches!(run_with_timeout(&mut quick, Duration::from_secs(30)).unwrap(), Outcome::Exited(s) if s.success())
    );
    // the status is cached: wait / try_wait keep returning it, and kill on a reaped child is Ok
    assert!(quick.wait().unwrap().success());
    assert!(quick.try_wait().unwrap().is_some_and(|s| s.success()));
    assert!(quick.kill().is_ok());

    let mut hung = me("hang").stdout(Stdio::piped()).spawn().unwrap();
    let mut ready = String::new();
    BufReader::new(hung.stdout.as_mut().unwrap())
        .read_line(&mut ready)
        .unwrap(); // synchronise on output, not on a sleep
    assert_eq!(ready, "ready\n");
    assert!(hung.try_wait().unwrap().is_none(), "try_wait never blocks");
    match run_with_timeout(&mut hung, Duration::from_millis(50)).unwrap() {
        Outcome::TimedOut(status) => {
            assert!(!status.success());
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(
                    status.code(),
                    None,
                    "killed by a signal: there is no exit code"
                );
                assert_eq!(
                    status.signal(),
                    Some(9),
                    "kill() sends SIGKILL: no cleanup runs in the child"
                );
            }
        }
        other => panic!("{other:?}"),
    }
    drop(KillOnDrop(
        me("hang").stdout(Stdio::null()).spawn().unwrap(),
    )); // returns promptly: killed, not waited out
    println!("5. timeouts / kill ok");
}

// ------------------------------------------------------------ 6. One pipe for both streams
fn s06_merged_output() {
    let (mut reader, writer) = io::pipe().unwrap(); // 1.87
    let mut cmd = me("interleave");
    cmd.stdout(writer.try_clone().unwrap()).stderr(writer);
    let mut child = cmd.spawn().unwrap();
    drop(cmd); // the Command still owns both write ends; EOF cannot arrive while they are open
    let mut all = String::new();
    reader.read_to_string(&mut all).unwrap();
    assert!(child.wait().unwrap().success());
    // two separate captures would lose the relative order; one pipe keeps the order of the child's writes
    assert_eq!(all, "out 0\nerr 0\nout 1\nerr 1\nout 2\nerr 2\n");

    // redirect to a file: a File converts into Stdio
    let path = std::env::temp_dir().join(format!("std1066-{}.log", std::process::id()));
    let file = std::fs::File::create_new(&path).unwrap();
    let st = me("gen").arg("3").stdout(file).status().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(st.success());
    assert_eq!(text, "item-0\nitem-1\nitem-2\n");
    println!("6. merged and redirected output ok");
}

// ------------------------------------------------------------ 7. Exiting: ExitCode vs process::exit
fn s07_exit() {
    let o = me("drop-return").output().unwrap();
    assert_eq!(
        (o.status.code(), o.stdout.as_slice()),
        (Some(3), b"destructor ran\n".as_slice())
    );
    let o = me("drop-exit").output().unwrap();
    assert_eq!(
        (o.status.code(), o.stdout.as_slice()),
        (Some(3), b"".as_slice()),
        "process::exit runs no destructors on any stack frame"
    );
    #[cfg(unix)]
    {
        // wait() reports only the low 8 bits: exit(256) looks like success
        assert_eq!(
            me("fail").arg("259").output().unwrap().status.code(),
            Some(3)
        );
        assert!(me("fail").arg("256").output().unwrap().status.success());
    }
    assert_eq!(me("no-such-mode").output().unwrap().status.code(), Some(64));
    println!("7. exit semantics ok");
}

// ------------------------------------------------------------ 8. Unix extensions
#[cfg(unix)]
fn s08_unix() {
    use std::os::unix::process::CommandExt;
    // exec replaces the current process: same pid, no return
    let out = stdout_of(&mut me("exec"));
    let pids: Vec<&str> = out.lines().collect();
    assert_eq!(pids.len(), 2);
    assert_eq!(pids[0], pids[1]);
    // arg0 sets argv[0] independently of the program path
    assert_eq!(stdout_of(me("arg0").arg0("worker-7")), "worker-7\n");
    // process_group(0): the child leads a new group, so terminal Ctrl-C (sent to the foreground group) does not reach it,
    // and a supervisor can signal the child together with its descendants via the negative pid.
    let child = KillOnDrop(
        me("hang")
            .stdout(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let pid = child.0.id() as libc::pid_t;
    // SAFETY: getpgid takes a plain integer and has no memory-safety preconditions; `pid` is our own unreaped child,
    // so the id cannot have been recycled.
    let pgid = unsafe { libc::getpgid(pid) };
    assert_eq!(pgid, pid);
    println!("8. unix exec / arg0 / process_group ok");
}
#[cfg(not(unix))]
fn s08_unix() {
    println!("8. unix extensions skipped on this platform");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        return child_main(&args[2], &args[3..]).unwrap_or_else(|e| {
            eprintln!("child error: {e}");
            ExitCode::FAILURE
        });
    }
    s01_output_status();
    s02_args_env_cwd();
    s03_stdin_stdout();
    s04_pipeline();
    s05_timeouts();
    s06_merged_output();
    s07_exit();
    s08_unix();
    println!("all process sections passed");
    ExitCode::SUCCESS
}
