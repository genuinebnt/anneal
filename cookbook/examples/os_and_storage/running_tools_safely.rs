//! RECIPE (processes): run an outside tool with limits.
//! LEVEL: intermediate
//! Use in: any program that starts another program.   Run: cargo run --example running_tools_safely
//! COMMON USES: converters, linters, version control, archive and media tools.
//!
//! No shell, a program named by its full path, arguments that cannot turn into options, a chosen environment and
//! folder, a cap on output, one time limit, and the whole process group killed when the run ends.
//!
//! **THIS IS NOT A SANDBOX.** The tool runs as you. It can read and write your files, use the network, take memory,
//! processor time and disk, and start more processes. A child or grandchild can **leave the group**, by starting its
//! own session or by joining another group. The program still kills its direct child, but a grandchild that left
//! survives. The group is cleanup, not containment. Open files that are not marked close-on-exec pass to the tool.
//! To run code you don't trust, use an isolation boundary made for it, and still apply these limits.
//! Unix only (Linux and macOS). Run and checked on macOS. The checks start this same program as the "tool".
//! Read: https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.process_group
#![allow(dead_code)]

use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- arguments

/// A value from outside, used as an argument that is **not** an option, such as a file name. Refuses what a program
/// would read as an option. What else a value can mean depends on the tool: read its manual.
fn operand(value: &OsStr) -> Option<&OsStr> {
    let bytes = value.as_bytes();
    let refused = bytes.is_empty() || bytes[0] == b'-' || bytes.contains(&0);
    (!refused).then_some(value)
}

// ---------------------------------------------------------------- running

struct Tool {
    /// a full path: nothing is searched for
    program: PathBuf,
    args: Vec<OsString>,
    /// the whole environment of the tool
    env: Vec<(OsString, OsString)>,
    folder: PathBuf,
    /// for each of standard output and standard error
    max_output: usize,
    time_limit: Duration,
}

#[derive(Debug)]
struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
enum Failed {
    RelativePath,
    Start(io::Error),
    TooMuchOutput,
    TooSlow,
    /// killing the group did not close the pipes: something left the group and still holds them
    Escaped,
    Io(io::Error),
}

/// Owns the child. When the run ends, even by a panic, the group and the child are killed and the child is reaped.
struct Group {
    child: Child,
    reaped: bool,
}

impl Group {
    /// Kills every process still in the group, and the child itself in case it left the group. It must run
    /// **before** the child is reaped: until then the child's number cannot be given to a new process, so neither
    /// signal can hit a stranger.
    fn kill(&self) {
        let id = self.child.id() as libc::pid_t;
        if !self.reaped && id > 1 {
            // SAFETY: kill has no memory effects; a negative id means the process group with that number
            unsafe { libc::kill(-id, libc::SIGKILL) };
            // the child may have moved to another group; without this, `wait` could block with no limit
            // SAFETY: as above; the id is our own unreaped child
            unsafe { libc::kill(id, libc::SIGKILL) };
        }
    }

    /// Whether the child has exited. Looks without reaping it.
    fn has_exited(&self) -> io::Result<bool> {
        // SAFETY: siginfo_t is plain data, and waitid only writes to the one we pass
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let options = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
        // SAFETY: `info` is valid for writes for the whole call
        if unsafe { libc::waitid(libc::P_PID, self.child.id(), &mut info, options) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: waitid filled `info`, or left the zeros that mean "still running"
        Ok(unsafe { info.si_pid() } != 0)
    }

    fn finish(&mut self) -> io::Result<ExitStatus> {
        self.kill();
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish();
        }
    }
}

enum Event {
    Closed(usize, io::Result<Vec<u8>>),
    TooMuch,
}

/// Reads one pipe to its end on its own thread, keeping at most `max` bytes.
fn drain(
    mut pipe: impl Read + Send + 'static,
    which: usize,
    max: usize,
    events: mpsc::Sender<Event>,
) {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8192];
        let event = loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break Event::Closed(which, Ok(kept)),
                Ok(n) if n > max - kept.len() => break Event::TooMuch,
                Ok(n) => kept.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => break Event::Closed(which, Err(e)),
            }
        };
        let too_much = matches!(event, Event::TooMuch);
        let _ = events.send(event);
        // keep emptying the pipe, so the tool is stopped by our kill and not left blocked on a full pipe
        while too_much && pipe.read(&mut chunk).is_ok_and(|n| n > 0) {}
    });
}

fn run(tool: &Tool) -> Result<Finished, Failed> {
    if !tool.program.is_absolute() {
        return Err(Failed::RelativePath);
    }
    let mut command = Command::new(&tool.program);
    command
        .args(&tool.args)
        .env_clear()
        .envs(tool.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&tool.folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0); // a new group, numbered like the child
    let started = Instant::now();
    let mut group = Group {
        child: command.spawn().map_err(Failed::Start)?,
        reaped: false,
    };

    let (send, events) = mpsc::channel();
    let missing = || Failed::Io(io::Error::other("pipe missing"));
    drain(
        group.child.stdout.take().ok_or_else(missing)?,
        0,
        tool.max_output,
        send.clone(),
    );
    drain(
        group.child.stderr.take().ok_or_else(missing)?,
        1,
        tool.max_output,
        send,
    );

    // The run is over when both pipes are closed AND the child has exited. One deadline covers both.
    let mut output: [Option<Vec<u8>>; 2] = [None, None];
    let failure = loop {
        let open = output.iter().any(Option::is_none);
        if !open && group.has_exited().map_err(Failed::Io)? {
            break None;
        }
        let left = tool.time_limit.saturating_sub(started.elapsed());
        if left.is_zero() {
            break Some(Failed::TooSlow);
        }
        let tick = left.min(Duration::from_millis(10));
        if !open {
            thread::sleep(tick);
            continue;
        }
        match events.recv_timeout(tick) {
            Ok(Event::Closed(which, Ok(bytes))) => output[which] = Some(bytes),
            Ok(Event::Closed(_, Err(e))) => break Some(Failed::Io(e)),
            Ok(Event::TooMuch) => break Some(Failed::TooMuchOutput),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Some(Failed::Io(io::Error::other("reader stopped")));
            }
        }
    };

    // kill the group on success too: a tool may leave a helper running
    let status = group.finish().map_err(Failed::Io)?;
    if let Some(failure) = failure {
        // if a pipe stays open after the kill, whoever holds it is outside the group
        let grace = Instant::now() + Duration::from_millis(500);
        loop {
            match events.recv_timeout(grace.saturating_duration_since(Instant::now())) {
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(failure),
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(Failed::Escaped),
            }
        }
    }
    let [Some(stdout), Some(stderr)] = output else {
        return Err(Failed::Io(io::Error::other("output missing")));
    };
    Ok(Finished {
        status,
        stdout,
        stderr,
    })
}

// ---------------------------------------------------------------- the pretend tool

/// The checks run this program again with `helper <what>`, so they depend on no installed tool.
#[allow(clippy::zombie_processes)] // leaving a child unwaited is the behaviour under test
fn helper(args: &[String]) {
    let again = |what: &[&str]| {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.arg("helper").args(what);
        c
    };
    match args[0].as_str() {
        "show" => {
            let mut env: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
            env.sort();
            println!("env {}", env.join(" "));
            println!("folder {}", std::env::current_dir().unwrap().display());
            println!("args {:?}", &args[1..]);
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input).unwrap();
            println!("input {}", input.len());
        }
        "fail" => {
            eprintln!("broken");
            std::process::exit(3);
        }
        "print" => {
            let count: usize = args[1].parse().unwrap();
            io::stdout().write_all(&vec![b'x'; count]).unwrap();
        }
        "flood" => {
            while io::stdout().write_all(&[b'x'; 4096]).is_ok()
                && io::stderr().write_all(b"y").is_ok()
            {}
        }
        "group" => {
            // SAFETY: both calls only read
            println!("{} {}", std::process::id(), unsafe { libc::getpgrp() });
        }
        // moves itself into the group of the program that started it
        "leave-group" => {
            // SAFETY: these calls change only which group this process is in
            let moved = unsafe { libc::setpgid(0, libc::getpgid(libc::getppid())) };
            assert_eq!(moved, 0);
            thread::sleep(Duration::from_secs(15));
        }
        // like "escape", but the new process joins the group of the program that started the tool
        "join" => {
            // SAFETY: both calls only read
            let outer = unsafe { libc::getpgid(libc::getppid()) };
            let mut c = again(&["sleep", "30"]);
            c.process_group(outer)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let child = c.spawn().unwrap();
            std::fs::write(&args[1], child.id().to_string()).unwrap();
        }
        // prints too much while a process outside the group holds the same standard output
        "escape-and-flood" => {
            let mut c = again(&["sleep", "30"]);
            c.stderr(Stdio::null());
            // SAFETY: setsid is safe to call between fork and exec
            unsafe {
                c.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
            let child = c.spawn().unwrap();
            std::fs::write(&args[1], child.id().to_string()).unwrap();
            let _ = io::stdout().write_all(&[b'x'; 200_000]);
        }
        // fills one pipe completely before it writes a byte to the other
        "both" => {
            let big = vec![b'x'; 1_000_000];
            let (mut out, mut err) = (io::stdout(), io::stderr());
            if args[1] == "stderr-first" {
                err.write_all(&big).unwrap();
                out.write_all(&big).unwrap();
            } else {
                out.write_all(&big).unwrap();
                err.write_all(&big).unwrap();
            }
        }
        // reads from a file descriptor it was never told to open
        "read-descriptor" => {
            // SAFETY: the check passes the number of a descriptor that this process inherited
            let mut file = unsafe { std::fs::File::from_raw_fd(args[1].parse().unwrap()) };
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            print!("{text}");
        }
        "sleep" => thread::sleep(Duration::from_secs(args[1].parse().unwrap())),
        "close-then-sleep" => {
            // SAFETY: this helper writes nothing after closing its output pipes
            unsafe {
                libc::close(1);
                libc::close(2);
            }
            thread::sleep(Duration::from_secs(30));
        }
        // starts a helper of its own, writes that process's number to a file, then ends or sleeps
        "leave" | "stay" => {
            let mut c = again(&["sleep", "30"]);
            if args.get(2).is_some_and(|a| a == "quiet") {
                c.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let child = c.spawn().unwrap();
            std::fs::write(&args[1], child.id().to_string()).unwrap();
            if args[0] == "stay" {
                thread::sleep(Duration::from_secs(30));
            }
        }
        // like "leave", but the new process starts its own session, outside the group
        "escape" => {
            let mut c = again(&["sleep", "30"]);
            if args.get(2).is_some_and(|a| a == "quiet") {
                c.stdout(Stdio::null()).stderr(Stdio::null());
            }
            // SAFETY: setsid is safe to call between fork and exec
            unsafe {
                c.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
            let child = c.spawn().unwrap();
            std::fs::write(&args[1], child.id().to_string()).unwrap();
            if args.get(3).is_some_and(|a| a == "stay") {
                thread::sleep(Duration::from_secs(30));
            }
        }
        "read" => print!("{}", std::fs::read_to_string(&args[1]).unwrap()),
        "connect" => {
            let mut s = std::net::TcpStream::connect(&args[1]).unwrap();
            s.write_all(b"data leaving").unwrap();
        }
        other => panic!("unknown helper {other}"),
    }
}

// ---------------------------------------------------------------- checks

/// A private folder for one run of the checks, removed at the end.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Scratch {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tools-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Scratch(path.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fake_tool(folder: &Path, args: &[&str]) -> Tool {
    Tool {
        program: std::env::current_exe().unwrap(),
        args: std::iter::once("helper")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect(),
        // set PATH yourself: some shells make one up when it is missing, and it may include the current folder
        env: vec![
            ("LANG".into(), "C".into()),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ],
        folder: folder.to_path_buf(),
        max_output: 100_000,
        time_limit: Duration::from_secs(20),
    }
}

fn alive(id: i32) -> bool {
    // SAFETY: signal 0 sends nothing; it only asks whether the process exists
    unsafe { libc::kill(id, 0) == 0 }
}

/// Waits a while for a process to disappear.
fn gone_soon(id: i32) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    while alive(id) && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    !alive(id)
}

fn read_id(file: &Path) -> i32 {
    let id = std::fs::read_to_string(file).unwrap().parse().unwrap();
    assert!(id > 1);
    id
}

fn setup_checks(folder: &Path) {
    // the tool gets exactly the environment, folder and arguments we chose, and no input
    let text = "a b; $(touch made) `x` | > *\n'\"";
    let done = run(&fake_tool(folder, &["show", text])).unwrap();
    let out = String::from_utf8(done.stdout).unwrap();
    assert!(done.status.success());
    assert!(out.contains("env LANG=C PATH=/usr/bin:/bin\n"), "{out}");
    assert!(
        out.contains(&format!("folder {}\n", folder.display())),
        "{out}"
    );
    assert!(out.contains(&format!("args {:?}\n", [text])), "{out}");
    assert!(out.contains("input 0\n"), "{out}");
    assert!(!folder.join("made").exists(), "no shell read the text");

    // the child leads a group of its own: its process number is also its group number
    let done = run(&fake_tool(folder, &["group"])).unwrap();
    let out = String::from_utf8(done.stdout).unwrap();
    let (id, group) = out.trim().split_once(' ').unwrap();
    assert_eq!(id, group, "the child is not in its own group");
    shell_path_checks(folder);

    let done = run(&fake_tool(folder, &["fail"])).unwrap();
    assert_eq!(
        (done.status.code(), done.stderr.as_slice()),
        (Some(3), b"broken\n".as_slice())
    );

    let mut relative = fake_tool(folder, &["show"]);
    relative.program = "show".into();
    assert!(matches!(run(&relative), Err(Failed::RelativePath)));
    let mut missing = fake_tool(folder, &["show"]);
    missing.program = folder.join("no-such-program");
    assert!(matches!(run(&missing), Err(Failed::Start(_))));
    assert!(matches!(
        run(&fake_tool(folder, &["show", "a\0b"])),
        Err(Failed::Start(_))
    ));
    println!("environment, folder, arguments, exit status: ok");
}

/// What a shell sees in PATH, with ours set and with none.
fn shell_path_checks(folder: &Path) {
    let ask = |env: Vec<(OsString, OsString)>| {
        let tool = Tool {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf %s \"$PATH\"".into()],
            env,
            ..fake_tool(folder, &[])
        };
        String::from_utf8(run(&tool).unwrap().stdout).unwrap()
    };
    assert_eq!(ask(fake_tool(folder, &[]).env), "/usr/bin:/bin");
    if cfg!(target_os = "macos") {
        let made_up = ask(Vec::new());
        assert!(
            made_up.split(':').any(|part| part == "."),
            "expected the shell to add the current folder: {made_up}"
        );
    }
}

/// A tool must not read what is waiting on **our** input. The child started here has text waiting on its own.
fn input_checks() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("input-check")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let waiting = child.stdin.take().unwrap().write_all(b"meant for us");
    waiting.unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout).unwrap().contains("input 0\n"),
        "the tool read our input"
    );
    println!("input: ok");
}

/// `sort` is a real tool with an option that writes a file.
fn option_checks(folder: &Path) {
    let sort = Path::new("/usr/bin/sort");
    if !sort.exists() {
        println!("option injection with sort: SKIPPED, no /usr/bin/sort");
        return;
    }
    let target = folder.join("precious.txt");
    let attack = OsString::from(format!("-o{}", target.display()));
    let tool = |args: Vec<OsString>| Tool {
        program: sort.into(),
        args,
        ..fake_tool(folder, &[])
    };

    // the attack: a "file name" that is really an option
    std::fs::write(&target, "do not lose me\n").unwrap();
    let done = run(&tool(vec![attack.clone()])).unwrap();
    assert!(done.status.success());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "",
        "the attack must work without a defence"
    );

    // after `--`, sort takes it as a file name, and no such file exists
    std::fs::write(&target, "do not lose me\n").unwrap();
    let done = run(&tool(vec!["--".into(), attack.clone()])).unwrap();
    assert!(!done.status.success());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "do not lose me\n"
    );

    // the check refuses it before any program starts, and lets ordinary names through
    assert_eq!(operand(&attack), None);
    for bad in ["", "-", "--output=x", "-o", "a\0"] {
        assert_eq!(operand(OsStr::new(bad)), None, "{bad:?}");
    }
    for good in ["report.txt", "./-o", "a -o b", "héllo"] {
        assert_eq!(operand(OsStr::new(good)), Some(OsStr::new(good)));
    }
    // the check says nothing about where the name leads
    assert!(operand(OsStr::new("../../etc/hosts")).is_some());
    println!("option injection with sort: ok");
}

/// Random byte strings: whatever `operand` accepts, the tool receives as it was, and it cannot start an option.
fn random_operand_checks(folder: &Path) {
    let mut seed = 99u64;
    let mut random = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let mut accepted = Vec::new();
    for _ in 0..5000 {
        let value: Vec<u8> = (0..random(6))
            .map(|_| *b"-a =;$'\0\n./*".get(random(12)).unwrap())
            .collect();
        let value = OsStr::from_bytes(&value);
        let expected =
            value.as_bytes().first().is_some_and(|c| *c != b'-') && !value.as_bytes().contains(&0);
        assert_eq!(operand(value).is_some(), expected, "{value:?}");
        if expected && accepted.len() < 200 {
            accepted.push(value.to_str().unwrap().to_string());
        }
    }
    let args: Vec<&str> = std::iter::once("show")
        .chain(accepted.iter().map(String::as_str))
        .collect();
    let out = String::from_utf8(run(&fake_tool(folder, &args)).unwrap().stdout).unwrap();
    assert!(out.contains(&format!("args {accepted:?}\n")));
    println!("random arguments: ok");
}

fn limit_checks(folder: &Path) {
    let mut tool = fake_tool(folder, &["print", "100000"]);
    assert_eq!(
        run(&tool).unwrap().stdout.len(),
        100_000,
        "exactly the cap is allowed"
    );
    tool.args[2] = "100001".into();
    assert!(matches!(run(&tool), Err(Failed::TooMuchOutput)));
    assert!(matches!(
        run(&fake_tool(folder, &["flood"])),
        Err(Failed::TooMuchOutput)
    ));
    // far more than a pipe buffer on BOTH pipes, in each order: reading one pipe after the other would hang
    for order in ["stdout-first", "stderr-first"] {
        let mut both = fake_tool(folder, &["both", order]);
        both.max_output = 2_000_000;
        both.time_limit = Duration::from_secs(5);
        let done = run(&both).unwrap_or_else(|e| panic!("{order}: {e:?}"));
        assert_eq!(
            (done.stdout.len(), done.stderr.len()),
            (1_000_000, 1_000_000)
        );
    }

    for what in [["sleep", "30"].as_slice(), &["close-then-sleep"]] {
        let mut slow = fake_tool(folder, what);
        slow.time_limit = Duration::from_millis(700);
        let started = Instant::now();
        let got = run(&slow);
        assert!(matches!(got, Err(Failed::TooSlow)), "{what:?}: {got:?}");
        assert!(started.elapsed() < Duration::from_secs(15), "{what:?}");
    }
    println!("output and time limits: ok");
}

fn group_checks(folder: &Path) {
    let file = folder.join("id");
    let name = file.to_str().unwrap();

    // the tool ends at once, but its own child keeps the output pipe open
    let mut tool = fake_tool(folder, &["leave", name]);
    tool.time_limit = Duration::from_secs(2);
    assert!(matches!(run(&tool), Err(Failed::TooSlow)));
    assert!(
        gone_soon(read_id(&file)),
        "the grandchild survived the time limit"
    );

    // the same, with the tool still running
    let mut tool = fake_tool(folder, &["stay", name]);
    tool.time_limit = Duration::from_secs(2);
    assert!(matches!(run(&tool), Err(Failed::TooSlow)));
    assert!(
        gone_soon(read_id(&file)),
        "the grandchild survived the time limit"
    );

    // a success that leaves a quiet helper behind: it is killed as well
    std::fs::remove_file(&file).unwrap();
    assert!(
        run(&fake_tool(folder, &["leave", name, "quiet"]))
            .unwrap()
            .status
            .success()
    );
    assert!(
        gone_soon(read_id(&file)),
        "a helper outlived a successful run"
    );
    println!("process groups: ok");
}

/// A tool that moves out of its group must still be killed, or waiting for it would ignore the time limit.
fn left_group_checks(folder: &Path) {
    let mut tool = fake_tool(folder, &["leave-group"]);
    tool.time_limit = Duration::from_millis(500);
    let started = Instant::now();
    let got = run(&tool);
    assert!(matches!(got, Err(Failed::TooSlow)), "{got:?}");
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "the 500 ms limit took {:?}",
        started.elapsed()
    );

    // after too much output the reader keeps going, so it still notices that someone holds the pipe
    let file = folder.join("flood-id");
    let mut tool = fake_tool(folder, &["escape-and-flood", file.to_str().unwrap()]);
    tool.max_output = 1000;
    let got = run(&tool);
    let id = read_id(&file);
    // SAFETY: `id` is the helper this check started
    unsafe { libc::kill(id, libc::SIGKILL) };
    assert!(matches!(got, Err(Failed::Escaped)), "{got:?}");
    println!("a tool that leaves its group: ok");
}

/// What the limits do NOT stop. Each check passes because the tool succeeds.
fn not_a_sandbox_checks(folder: &Path) {
    let file = folder.join("id");
    let name = file.to_str().unwrap();

    // a new session leaves the group: it survives a successful run
    assert!(
        run(&fake_tool(folder, &["escape", name, "quiet"]))
            .unwrap()
            .status
            .success()
    );
    let id = read_id(&file);
    assert!(alive(id), "the escaped process was expected to survive");
    // SAFETY: `id` is the helper this check started
    unsafe { libc::kill(id, libc::SIGKILL) };

    // if it also keeps the pipes, the kill cannot close them, and the run says so
    let mut tool = fake_tool(folder, &["escape", name, "loud", "stay"]);
    tool.time_limit = Duration::from_secs(2);
    assert!(matches!(run(&tool), Err(Failed::Escaped)));
    let id = read_id(&file);
    assert!(alive(id));
    // SAFETY: as above
    unsafe { libc::kill(id, libc::SIGKILL) };

    // a grandchild can also leave by joining another group
    assert!(
        run(&fake_tool(folder, &["join", name]))
            .unwrap()
            .status
            .success()
    );
    let id = read_id(&file);
    assert!(
        alive(id),
        "the process that changed group was expected to survive"
    );
    // SAFETY: `id` is the helper this check started
    unsafe { libc::kill(id, libc::SIGKILL) };

    // a descriptor without close-on-exec passes to the tool, although no argument or variable names a file.
    // Rust marks the files it opens; `dup` makes a copy without the mark, as C libraries may.
    let hidden = folder.join("open-file");
    std::fs::write(&hidden, "still open").unwrap();
    let opened = std::fs::File::open(&hidden).unwrap();
    // SAFETY: dup copies a descriptor that is open
    let copy = unsafe { libc::dup(opened.as_raw_fd()) };
    assert!(copy > 2);
    let done = run(&fake_tool(folder, &["read-descriptor", &copy.to_string()])).unwrap();
    // SAFETY: `copy` is ours and is not used again
    unsafe { libc::close(copy) };
    assert_eq!(done.stdout, b"still open");

    // it can read a file outside its folder, whatever its environment says
    let outside = Scratch::new();
    let secret = outside.0.join("secret");
    std::fs::write(&secret, "my token").unwrap();
    let done = run(&fake_tool(folder, &["read", secret.to_str().unwrap()])).unwrap();
    assert_eq!(done.stdout, b"my token");

    // and it can use the network
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    assert!(
        run(&fake_tool(folder, &["connect", &address]))
            .unwrap()
            .status
            .success()
    );
    let mut got = String::new();
    listener
        .accept()
        .unwrap()
        .0
        .read_to_string(&mut got)
        .unwrap();
    assert_eq!(got, "data leaving");
    println!("not a sandbox (escape, descriptors, files, network all work): ok");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "helper") {
        return helper(&args[2..]);
    }
    if args.get(1).is_some_and(|a| a == "input-check") {
        let scratch = Scratch::new();
        let done = run(&fake_tool(&scratch.0, &["show"])).unwrap();
        return io::stdout().write_all(&done.stdout).unwrap();
    }
    let scratch = Scratch::new();
    setup_checks(&scratch.0);
    input_checks();
    option_checks(&scratch.0);
    random_operand_checks(&scratch.0);
    limit_checks(&scratch.0);
    group_checks(&scratch.0);
    left_group_checks(&scratch.0);
    not_a_sandbox_checks(&scratch.0);
    println!("all tool running checks passed");
}
