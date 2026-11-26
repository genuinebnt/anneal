//! RECIPE 1071 (std_tour): Unix signals with `libc`, Ctrl-C and SIGTERM shutdown, SIGPIPE, environment variables, exit hooks, and which destructors run.
//! LEVEL: advanced
//! Use in: services that must drain on deploy (SIGTERM), CLIs that clean up on Ctrl-C, supervisors, anything embedded next to C code.
//! Run: cargo run --release --example std_signals_env_exit_hooks_ctrlc
//! COMMON USES: set a flag on SIGTERM and finish in-flight work; wake a blocked thread with a self-pipe; force quit on a second Ctrl-C; read configuration from the environment.
//! METHODS: libc::sigaction, libc::sigemptyset, libc::sigaddset, libc::sigismember, libc::sigpending, libc::pthread_sigmask, libc::raise, libc::kill,
//! libc::write, libc::fcntl, libc::_exit, libc::atexit, std::io::pipe, AsRawFd::as_raw_fd, ExitStatusExt::signal, std::process::exit,
//! std::process::abort, std::env::var, std::env::var_os, std::env::vars_os, std::env::set_var, std::env::remove_var, std::env::VarError
//! DOCS: https://doc.rust-lang.org/std/process/fn.exit.html
//! DEFINED IN: std has NO signal API: everything signal related comes from the `libc` crate (bindings to the platform C library).
//!   std: `std::env`, `std::process`, `std::io::pipe`, `std::os::fd`, `std::os::unix`. core: the atomics used inside handlers.
//! NOT COVERED (and why): the `signal-hook` and `ctrlc` crates and `tokio::signal` (what production code should normally use; they implement the same
//! self-pipe idea); signalfd (Linux only) and `sigwait` threads; realtime signals; `siginfo_t` handlers; SIGCHLD reaping; Windows console control events.
//! PLATFORM: unix only; other targets print a notice. Written and run on macOS; the Linux path was NOT run.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   shut down cleanly on SIGTERM / Ctrl-C        -> handler stores to an atomic flag (or writes a byte to a self-pipe); normal code does the work
//!   wake a thread blocked in I/O                 -> self-pipe: handler `write`s, the thread `read`s
//!   force quit on the second Ctrl-C              -> handler: if already requested, `_exit(130)`
//!   handle a closed pipe                         -> nothing to install: Rust ignores SIGPIPE, so writes return `BrokenPipe`
//!   defer a signal over a critical section       -> block it with `pthread_sigmask`, unblock afterwards
//!   read an env var                              -> `env::var` (Err: NotPresent / NotUnicode) or `env::var_os`
//!   give a child different variables             -> `Command::env`, not `set_var`
//!   run cleanup reliably                         -> return from `main` so destructors run; NOTHING runs on SIGKILL or power loss
//!
//! WHAT RUNS WHEN (asserted in section 6; "std stdout" is a partial line still in std's buffer)
//!   how the process ends     | local destructors | atexit handlers | std stdout flushed | user BufWriter flushed
//!   return from main         | yes               | yes             | yes                | yes (its Drop)
//!   std::process::exit       | NO                | yes             | yes (1.98.1 impl)  | NO
//!   libc::_exit              | NO                | NO              | NO                 | NO
//!   std::process::abort      | NO                | NO              | NO                 | NO
//!   unhandled SIGTERM        | NO                | NO              | NO                 | NO
//!
//! HANDLER RULES: only async-signal-safe operations. NOT allowed: allocating, locking, `println!`, formatting, panicking. Allowed here: lock-free
//! atomics, `write`, `_exit`. Preserve `errno`. Dispositions are process wide, so a library should not silently install them.
//! TRY NEXT:
//!   - [S] Add `eprintln!` to the handler and describe the deadlock it allows (which lock, held by whom?). Do not rely on a test to find it.
//!   - [S] In section 3 raise the signal three times while blocked. Predict the count after unblocking, for a standard signal.
//!   - [M] Handle SIGHUP as "reload config": the waiter thread re-reads a file and swaps an `Arc`.
//!   - [M] Add a shutdown deadline: if cleanup takes longer than N ms after the first signal, a watchdog thread calls `process::exit(1)`.
//!   - [A] PROJECT: combine with `std_net_tcp_udp_ip_addr` and `std_sync_primitives_use_cases`: a loopback server that stops accepting on SIGTERM, drains, and exits 0, tested by a parent process.
//! Read: POSIX async-signal-safe list https://man7.org/linux/man-pages/man7/signal-safety.7.html ; sigaction https://man7.org/linux/man-pages/man2/sigaction.2.html ; https://doc.rust-lang.org/std/env/fn.set_var.html (Safety section)
#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(not(unix))]
fn main() {
    println!(
        "std_signals_env_exit_hooks_ctrlc: unix signals are not available on this platform; nothing to run."
    );
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    unix::main()
}

#[cfg(unix)]
mod unix {
    use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
    use std::mem;
    use std::os::fd::AsRawFd;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, ExitCode, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering::SeqCst};

    fn cvt(ret: libc::c_int) -> io::Result<libc::c_int> {
        if ret == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret)
        }
    }

    // ============================================================ handler state: statics, because a handler gets only the signal number
    static SHUTDOWN: AtomicBool = AtomicBool::new(false);
    static USR1_COUNT: AtomicUsize = AtomicUsize::new(0);
    static WAKE_FD: AtomicI32 = AtomicI32::new(-1);
    static FORCE_ON_SECOND: AtomicBool = AtomicBool::new(false);

    fn errno_ptr() -> *mut libc::c_int {
        #[cfg(any(target_os = "linux", target_os = "emscripten"))]
        // SAFETY: returns the address of this thread's errno; it has no preconditions.
        return unsafe { libc::__errno_location() };
        #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
        // SAFETY: as above.
        return unsafe { libc::__error() };
        #[cfg(not(any(
            target_os = "linux",
            target_os = "emscripten",
            target_vendor = "apple",
            target_os = "freebsd"
        )))]
        return std::ptr::null_mut();
    }

    extern "C" fn on_signal(sig: libc::c_int) {
        // The handler may interrupt code between a failing call and its errno check: save errno and restore it.
        let errno = errno_ptr();
        // SAFETY: when non-null, `errno` is this thread's valid errno slot.
        let saved = (!errno.is_null()).then(|| unsafe { *errno });
        if sig == libc::SIGUSR1 {
            USR1_COUNT.fetch_add(1, SeqCst);
        } else if SHUTDOWN.swap(true, SeqCst) && FORCE_ON_SECOND.load(SeqCst) {
            // SAFETY: _exit is async-signal-safe and does not return; it skips all user-space cleanup on purpose.
            unsafe { libc::_exit(130) };
        }
        let fd = WAKE_FD.load(SeqCst);
        if fd >= 0 {
            let byte = sig as u8;
            // SAFETY: write is async-signal-safe; the pointer is valid for 1 byte. `fd` is the non-blocking write end of
            // the self-pipe, which outlives the installed handlers. A full pipe returns EAGAIN, which is harmless
            // because a wake-up is then already pending.
            let _ = unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
        }
        if let Some(v) = saved {
            // SAFETY: as for the read above.
            unsafe { *errno = v };
        }
    }

    /// Installs `on_signal` and restores the previous disposition on drop.
    struct Installed {
        sig: libc::c_int,
        old: libc::sigaction,
    }
    fn set_action(sig: libc::c_int, new: &libc::sigaction) -> io::Result<libc::sigaction> {
        // SAFETY: an all-zero sigaction is a valid value of this plain C struct, used here only as an output buffer.
        let mut old: libc::sigaction = unsafe { mem::zeroed() };
        // SAFETY: both pointers refer to live sigaction values; the kernel reads `new` and writes `old`.
        cvt(unsafe { libc::sigaction(sig, new, &mut old) })?;
        Ok(old)
    }
    fn install(sig: libc::c_int) -> io::Result<Installed> {
        // SAFETY: zero is valid for every field (no flags, no handler yet); the fields that matter are set below.
        let mut sa: libc::sigaction = unsafe { mem::zeroed() };
        sa.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        sa.sa_flags = libc::SA_RESTART; // restart interrupted system calls where the OS supports it (not all calls restart)
        // SAFETY: `sa_mask` is a valid sigset_t to initialise.
        cvt(unsafe { libc::sigemptyset(&mut sa.sa_mask) })?;
        // The handler obeys the async-signal-safety rules listed in the header.
        Ok(Installed {
            sig,
            old: set_action(sig, &sa)?,
        })
    }
    impl Drop for Installed {
        fn drop(&mut self) {
            let _ = set_action(self.sig, &self.old);
        }
    }

    fn current_handler(sig: libc::c_int) -> io::Result<libc::sighandler_t> {
        // SAFETY: zeroed sigaction as an output buffer, as above.
        let mut old: libc::sigaction = unsafe { mem::zeroed() };
        // SAFETY: a null `new` pointer means "query only"; `old` is valid for writes.
        cvt(unsafe { libc::sigaction(sig, std::ptr::null(), &mut old) })?;
        Ok(old.sa_sigaction)
    }

    fn raise(sig: libc::c_int) {
        // SAFETY: raise takes an integer. Every signal raised in this file has a handler installed or is blocked.
        assert_eq!(unsafe { libc::raise(sig) }, 0);
    }

    // ============================================================ 1. install, raise, observe
    fn s01_flag() -> io::Result<()> {
        assert_eq!(
            current_handler(libc::SIGTERM)?,
            libc::SIG_DFL,
            "assumes the launcher did not change SIGTERM"
        );
        {
            let _term = install(libc::SIGTERM)?;
            let _int = install(libc::SIGINT)?;
            for sig in [libc::SIGTERM, libc::SIGINT] {
                SHUTDOWN.store(false, SeqCst);
                let mut work_done = 0;
                // the main loop polls the flag between units of work, and finishes the current unit
                for job in 0..100 {
                    if SHUTDOWN.load(SeqCst) {
                        break;
                    }
                    if job == 3 {
                        raise(sig); // POSIX: raise returns only after the handler has returned
                    }
                    work_done += 1;
                }
                assert_eq!(work_done, 4, "job 3 completed, job 4 never started");
            }
        }
        assert_eq!(
            current_handler(libc::SIGTERM)?,
            libc::SIG_DFL,
            "restored by Drop"
        );
        // SIGKILL and SIGSTOP cannot be caught
        let err = install(libc::SIGKILL).err().expect("must fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        SHUTDOWN.store(false, SeqCst);
        println!("1. handler + flag ok");
        Ok(())
    }

    // ============================================================ 2. self-pipe: turn a signal into readable bytes
    struct SelfPipe {
        reader: io::PipeReader,
        _writer: io::PipeWriter,
    }
    impl SelfPipe {
        fn new() -> io::Result<SelfPipe> {
            let (reader, writer) = io::pipe()?;
            let fd = writer.as_raw_fd();
            // SAFETY: fcntl on a descriptor we own, with integer arguments.
            let flags = cvt(unsafe { libc::fcntl(fd, libc::F_GETFL) })?;
            // SAFETY: as above. Non-blocking, so the handler can never stall on a full pipe.
            cvt(unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) })?;
            WAKE_FD.store(fd, SeqCst);
            Ok(SelfPipe {
                reader,
                _writer: writer,
            })
        }
        /// Blocks until a signal arrives. `read_exact` retries if the read itself is interrupted.
        fn wait(&mut self) -> io::Result<libc::c_int> {
            let mut b = [0u8; 1];
            self.reader.read_exact(&mut b)?;
            Ok(libc::c_int::from(b[0]))
        }
    }
    impl Drop for SelfPipe {
        fn drop(&mut self) {
            // disconnect BEFORE the fields close the descriptor, so the handler cannot write to a recycled fd number
            // (declare the SelfPipe before the handlers, so that they are uninstalled first)
            WAKE_FD.store(-1, SeqCst);
        }
    }

    fn s02_self_pipe() -> io::Result<()> {
        let mut pipe = SelfPipe::new()?;
        let _term = install(libc::SIGTERM)?;
        let _usr = install(libc::SIGUSR1)?;
        let before = USR1_COUNT.load(SeqCst);
        let seen = std::thread::scope(|s| -> io::Result<Vec<libc::c_int>> {
            let waiter = s.spawn(|| -> io::Result<Vec<libc::c_int>> {
                let mut seen = Vec::new();
                loop {
                    // ordinary code: it may allocate, lock, log
                    let sig = pipe.wait()?;
                    seen.push(sig);
                    if sig == libc::SIGTERM {
                        return Ok(seen);
                    }
                }
            });
            raise(libc::SIGUSR1); // runs the handler on THIS thread; the other thread is woken through the pipe
            raise(libc::SIGUSR1);
            raise(libc::SIGTERM);
            waiter.join().expect("waiter")
        })?;
        assert_eq!(seen, [libc::SIGUSR1, libc::SIGUSR1, libc::SIGTERM]);
        assert_eq!(USR1_COUNT.load(SeqCst) - before, 2);
        SHUTDOWN.store(false, SeqCst);
        println!("2. self-pipe ok");
        Ok(())
    }

    // ============================================================ 3. blocking: defer delivery over a critical section
    fn set_of(sig: libc::c_int) -> libc::sigset_t {
        // SAFETY: zeroed storage that sigemptyset immediately initialises.
        let mut set: libc::sigset_t = unsafe { mem::zeroed() };
        // SAFETY: `set` is valid for writes and `sig` is a real signal number.
        unsafe {
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, sig);
        }
        set
    }
    fn change_mask(how: libc::c_int, set: &libc::sigset_t) {
        // SAFETY: `set` is initialised; a null old-set pointer is allowed. Affects only the calling thread.
        assert_eq!(
            unsafe { libc::pthread_sigmask(how, set, std::ptr::null_mut()) },
            0
        );
    }
    fn is_pending(sig: libc::c_int) -> bool {
        // SAFETY: zeroed storage as the output of sigpending.
        let mut p: libc::sigset_t = unsafe { mem::zeroed() };
        // SAFETY: `p` is valid for writes, then initialised for sigismember.
        unsafe { libc::sigpending(&mut p) == 0 && libc::sigismember(&p, sig) == 1 }
    }

    fn s03_blocking() -> io::Result<()> {
        let _usr = install(libc::SIGUSR1)?;
        let set = set_of(libc::SIGUSR1);
        let before = USR1_COUNT.load(SeqCst);
        change_mask(libc::SIG_BLOCK, &set);
        raise(libc::SIGUSR1);
        raise(libc::SIGUSR1);
        assert_eq!(USR1_COUNT.load(SeqCst), before, "held back");
        assert!(is_pending(libc::SIGUSR1));
        change_mask(libc::SIG_UNBLOCK, &set); // POSIX: a pending unblocked signal is delivered before this returns
        let delivered = USR1_COUNT.load(SeqCst) - before;
        assert!(!is_pending(libc::SIGUSR1));
        // two raises while blocked: standard signals are a pending BIT, not a queue. POSIX leaves it to the
        // implementation; Linux and macOS deliver once. Never count events by counting signals.
        #[cfg(any(target_os = "linux", target_vendor = "apple"))]
        assert_eq!(delivered, 1);
        assert!(delivered >= 1);
        println!("3. blocking / pending ok");
        Ok(())
    }

    // ============================================================ 4. SIGPIPE
    fn s04_sigpipe() -> io::Result<()> {
        assert_eq!(
            current_handler(libc::SIGPIPE)?,
            libc::SIG_IGN,
            "the Rust runtime ignores SIGPIPE before main"
        );
        let (reader, mut writer) = io::pipe()?;
        drop(reader);
        // with the C default (SIG_DFL) this write would kill the process; ignored, it is an ordinary error (EPIPE)
        let err = writer.write(b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(err.raw_os_error(), Some(libc::EPIPE));
        // Embedding Rust in a C program (staticlib/cdylib) does not run this startup code: the host's disposition applies.
        println!("4. sigpipe ok");
        Ok(())
    }

    // ============================================================ child process modes
    struct DropLog(&'static str);
    impl Drop for DropLog {
        fn drop(&mut self) {
            println!("drop:{}", self.0);
        }
    }

    extern "C" fn at_exit_hook() {
        // Runs inside C's exit(): avoid Rust facilities that may already be shut down, and NEVER panic here.
        const MSG: &[u8] = b"atexit;";
        // SAFETY: MSG is valid for MSG.len() bytes and fd 1 is the process's stdout descriptor.
        let _ = unsafe { libc::write(1, MSG.as_ptr().cast(), MSG.len()) };
    }

    fn child(mode: &str) -> io::Result<ExitCode> {
        let say = |s: &str| -> io::Result<()> {
            let mut o = io::stdout().lock();
            writeln!(o, "{s}")?;
            o.flush()
        };
        match mode {
            // a well behaved service: SIGTERM or SIGINT, drain, destructors, exit 0
            "serve" | "serve-force" => {
                FORCE_ON_SECOND.store(mode == "serve-force", SeqCst);
                let mut pipe = SelfPipe::new()?;
                let _term = install(libc::SIGTERM)?;
                let _int = install(libc::SIGINT)?;
                let _res = DropLog("connection-pool");
                say("ready")?;
                let sig = pipe.wait()?;
                say(&format!("draining after {sig}"))?;
                if mode == "serve-force" {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(60)); // a stuck drain; the second signal ends it
                    }
                }
                say("drained")?;
                Ok(ExitCode::SUCCESS)
            }
            // no handler at all
            "unhandled" => {
                let _res = DropLog("never");
                print!("partial;");
                say("ready")?;
                std::thread::sleep(std::time::Duration::from_secs(60));
                Ok(ExitCode::FAILURE)
            }
            "end-return" | "end-exit" | "end-_exit" | "end-abort" => {
                // SAFETY: at_exit_hook is a valid extern "C" fn with no arguments, lives for the whole program,
                // does not unwind, and only calls write.
                assert_eq!(unsafe { libc::atexit(at_exit_hook) }, 0);
                let _local = DropLog("local");
                let mut user_buffer = BufWriter::new(io::stdout());
                write!(user_buffer, "user-buffer;")?; // stays inside the user's BufWriter
                print!("std-buffer;"); // no newline: stays inside std's own stdout buffer
                match mode {
                    "end-return" => Ok(ExitCode::SUCCESS),
                    "end-exit" => std::process::exit(0),
                    // SAFETY: _exit takes an integer and does not return.
                    "end-_exit" => unsafe { libc::_exit(0) },
                    _ => std::process::abort(),
                }
            }
            "env" => {
                // set_var / remove_var are unsafe since edition 2024: they race with any other thread that reads the
                // environment, including inside C libraries (for example during name resolution).
                // SAFETY: this child has just started and has spawned no threads.
                unsafe {
                    std::env::set_var("RECIPE_SET", "yes");
                    std::env::remove_var("RECIPE_REMOVE");
                }
                let show = |k: &str| match std::env::var(k) {
                    Ok(v) => format!("{k}={v}"),
                    Err(std::env::VarError::NotPresent) => format!("{k}:absent"),
                    Err(std::env::VarError::NotUnicode(raw)) => {
                        format!("{k}:not-unicode:{}", raw.len())
                    }
                };
                for k in [
                    "RECIPE_SET",
                    "RECIPE_REMOVE",
                    "RECIPE_EMPTY",
                    "RECIPE_BYTES",
                ] {
                    println!("{}", show(k));
                }
                assert_eq!(
                    std::env::var_os("RECIPE_BYTES").map(|v| v.len()),
                    Some(2),
                    "var_os keeps the raw bytes"
                );
                // vars_os never fails; vars() panics if ANY variable is not unicode
                println!(
                    "count={}",
                    std::env::vars_os()
                        .filter(|(k, _)| k.to_string_lossy().starts_with("RECIPE_"))
                        .count()
                );
                Ok(ExitCode::SUCCESS)
            }
            _ => Ok(ExitCode::from(64)),
        }
    }

    // ============================================================ parent side
    fn me(mode: &str) -> Command {
        let mut c = Command::new(std::env::current_exe().expect("own path"));
        c.arg("--child")
            .arg(mode)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        c
    }

    struct Child(std::process::Child, BufReader<std::process::ChildStdout>);
    impl Child {
        fn start(mode: &str) -> io::Result<Child> {
            let mut c = me(mode).spawn()?;
            let out = BufReader::new(c.stdout.take().expect("piped"));
            Ok(Child(c, out))
        }
        fn expect(&mut self, want: &str) -> io::Result<()> {
            let mut line = String::new();
            self.1.read_line(&mut line)?;
            assert!(
                line.trim_end().ends_with(want),
                "wanted {want:?}, got {line:?}"
            );
            Ok(())
        }
        fn signal(&self, sig: libc::c_int) -> io::Result<()> {
            // SAFETY: integer arguments only. The pid belongs to our child, which has not been waited on yet, so it
            // cannot have been reused for another process.
            cvt(unsafe { libc::kill(self.0.id() as libc::pid_t, sig) })?;
            Ok(())
        }
        fn finish(mut self) -> io::Result<(std::process::ExitStatus, String)> {
            let mut rest = String::new();
            self.1.read_to_string(&mut rest)?;
            Ok((self.0.wait()?, rest)) // Drop still runs afterwards: kill on an already reaped child is a no-op Ok
        }
    }
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    // ============================================================ 5. real signals between processes
    fn s05_processes() -> io::Result<()> {
        for sig in [libc::SIGTERM, libc::SIGINT] {
            let mut c = Child::start("serve")?;
            c.expect("ready")?; // synchronise: signal only after the handlers are installed
            c.signal(sig)?;
            c.expect(&format!("draining after {sig}"))?;
            let (status, rest) = c.finish()?;
            assert_eq!((status.code(), status.signal()), (Some(0), None));
            assert_eq!(
                rest, "drained\ndrop:connection-pool\n",
                "graceful: destructors ran"
            );
        }
        let mut c = Child::start("serve-force")?;
        c.expect("ready")?;
        c.signal(libc::SIGINT)?;
        c.expect(&format!("draining after {}", libc::SIGINT))?;
        c.signal(libc::SIGINT)?;
        let (status, rest) = c.finish()?;
        assert_eq!(
            status.code(),
            Some(130),
            "second Ctrl-C forces the exit (a normal exit with code 130)"
        );
        assert_eq!(rest, "", "_exit skipped the destructor");

        let mut c = Child::start("unhandled")?;
        c.expect("partial;ready")?;
        c.signal(libc::SIGTERM)?;
        let (status, rest) = c.finish()?;
        assert_eq!(
            (status.code(), status.signal()),
            (None, Some(libc::SIGTERM)),
            "died BY the signal: no exit code"
        );
        assert_eq!(rest, "", "no destructor output");
        println!("5. graceful, forced and unhandled termination ok");
        Ok(())
    }

    // ============================================================ 6. what runs at each kind of exit
    fn s06_exit_matrix() -> io::Result<()> {
        let run = |mode: &str| -> io::Result<(Option<i32>, Option<i32>, String)> {
            let o = me(mode).output()?;
            Ok((
                o.status.code(),
                o.status.signal(),
                String::from_utf8_lossy(&o.stdout).into_owned(),
            ))
        };
        let (code, _, out) = run("end-return")?;
        assert_eq!(code, Some(0));
        // locals drop in reverse order: user_buffer flushes into std's buffer, then DropLog prints, then exit hooks run
        for part in ["std-buffer;", "user-buffer;", "drop:local", "atexit;"] {
            assert!(out.contains(part), "{part} missing in {out:?}");
        }
        let (code, _, out) = run("end-exit")?;
        assert_eq!(code, Some(0));
        // std's own buffer is flushed by process::exit in this toolchain (it calls the runtime cleanup first). The docs
        // promise only that atexit handlers run and destructors do not, so flush explicitly before exit in real code.
        assert_eq!(out, "std-buffer;atexit;");
        let (code, _, out) = run("end-_exit")?;
        assert_eq!((code, out.as_str()), (Some(0), ""));
        let (code, signal, out) = run("end-abort")?;
        assert_eq!(
            (code, signal, out.as_str()),
            (None, Some(libc::SIGABRT), "")
        );
        println!("6. exit matrix ok");
        Ok(())
    }

    // ============================================================ 7. environment
    fn s07_env() -> io::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let o = me("env")
            .env("RECIPE_REMOVE", "x")
            .env("RECIPE_EMPTY", "")
            .env("RECIPE_BYTES", std::ffi::OsStr::from_bytes(&[b'a', 0xFF]))
            .env_remove("RECIPE_SET")
            .output()?;
        assert!(o.status.success());
        assert_eq!(
            String::from_utf8_lossy(&o.stdout),
            "RECIPE_SET=yes\nRECIPE_REMOVE:absent\nRECIPE_EMPTY=\nRECIPE_BYTES:not-unicode:2\ncount=3\n"
        );
        // empty and unset are different states
        assert!(matches!(
            std::env::var("RECIPE_SURELY_UNSET_1071"),
            Err(std::env::VarError::NotPresent)
        ));
        println!("7. environment ok");
        Ok(())
    }

    pub fn main() -> ExitCode {
        let mut args = std::env::args().skip(1);
        if args.next().as_deref() == Some("--child") {
            return child(&args.next().unwrap_or_default()).unwrap_or_else(|e| {
                eprintln!("child: {e}");
                ExitCode::FAILURE
            });
        }
        let all = || -> io::Result<()> {
            s01_flag()?;
            s02_self_pipe()?;
            s03_blocking()?;
            s04_sigpipe()?;
            s05_processes()?;
            s06_exit_matrix()?;
            s07_env()
        };
        match all() {
            Ok(()) => {
                println!("all signal sections passed");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        }
    }
}
