//! RECIPE 1068 (std_tour): stdin, stdout, stderr and terminal behaviour: writing command-line filters that are correct in a pipe and testable without one.
//! LEVEL: intermediate
//! Use in: CLIs, log processors, `cmd | filter` tools, anything printing progress.   Run: cargo run --release --example std_io_stdio_terminal_text_processing
//! COMMON USES: read all of stdin or stream it by line; lock and buffer stdout for speed; diagnostics on stderr; colour only on a terminal; exit quietly when `| head` closes the pipe.
//! METHODS: std::io::stdin, Stdin::lock, Stdin::read_line, Stdin::lines, std::io::read_to_string, std::io::stdout, Stdout::lock, std::io::stderr,
//! Write::flush, Write::write_all, writeln!, print!, println!, eprintln!, IsTerminal::is_terminal, BufRead::read_until, BufRead::lines,
//! BufWriter::new, std::io::Error::kind, ErrorKind::BrokenPipe, ErrorKind::InvalidData, std::process::ExitCode, std::env::args_os, std::env::var_os
//! DOCS: https://doc.rust-lang.org/std/io/fn.stdin.html
//! DEFINED IN: std: `stdin`, `stdout`, `stderr`, their locks, `IsTerminal`, `BufWriter`, `BufRead`, the print macros, `ExitCode`.
//!   core: `write!`/`writeln!` and `core::fmt::Write` (formatting into Strings; no I/O).
//! NOT COVERED (and why): terminal size, raw mode, key events and cursor control beyond raw escape codes (not in std: see `crossterm`); display width of
//! Unicode text (not in std: see `unicode-width`); argument parsing (see clap examples); BufWriter internals (see `std_bufreader_bufwriter_patterns`).
//! NOT EXERCISED: the `is_terminal() == true` path needs a real terminal; Windows console behaviour was not run.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   read all input                                -> `io::read_to_string(io::stdin())?`
//!   stream lines                                  -> `for line in io::stdin().lock().lines()`
//!   one interactive answer                        -> print prompt, `flush()`, `read_line(&mut s)`; return value 0 means EOF; `s` keeps the `\n`
//!   non-UTF-8 input                               -> `read_until(b'\n', &mut vec)` (`lines()` fails with InvalidData)
//!   many lines out, fast                          -> `let mut out = BufWriter::new(io::stdout().lock());` then `writeln!(out, ..)?` and `out.flush()?`
//!   errors, logs, progress                        -> stderr (`eprintln!`): it is unbuffered and does not pollute the pipe
//!   colour / progress bars                        -> only if `stream.is_terminal()` (and `NO_COLOR` unset)
//!   survive `| head`                              -> use `writeln!` (not `println!`) and treat `BrokenPipe` as a normal end
//!   exit code                                     -> `fn main() -> ExitCode`
//!   make a filter testable                        -> `fn run(input: impl BufRead, out: &mut impl Write) -> io::Result<..>`
//!
//! FACTS: `println!` PANICS if the write fails; Rust ignores SIGPIPE at startup, so a closed pipe shows up as an `Err`, not a silent signal death.
//! stdout is line buffered even when redirected. The stdin lock is NOT re-entrant (calling `stdin().read_line` while holding `stdin().lock()` on the
//! same thread deadlocks); the stdout and stderr locks are re-entrant.
//! TRY NEXT:
//!   - [S] In `head`, replace the early `break` by `lines.take(n)` after collecting everything. Which test in section 2 hangs, and why?
//!   - [S] Run `cargo run --release --example std_io_stdio_terminal_text_processing -- --child spam | head -1` and then the `spam-println` mode. Compare.
//!   - [M] Add `-i` (ignore case) and `-c` (count only) to `grep`, with Cursor tests; keep it byte based.
//!   - [M] Make `Progress` throttle by bytes processed so a 10 GB stream prints at most 100 updates; assert the count.
//!   - [A] PROJECT: combine with `std_process_command_pipelines`: a mini coreutils binary (`wc|head|uniq|sort|grep` subcommands) wired together through real pipes.
//! Read: https://doc.rust-lang.org/std/io/index.html#standard-input-and-output ; Command Line Applications in Rust https://rust-cli.github.io/book/ ; https://no-color.org/
#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, BufWriter, Cursor, IsTerminal, Read, Write};
use std::process::{Command, ExitCode, Stdio};

// ================================================================ filters: generic over BufRead / Write
#[derive(Debug, Default, PartialEq)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
}

/// Like `wc` in the C locale: lines are newline COUNTS, words are runs of non-ASCII-whitespace. Constant memory.
fn wc(mut input: impl BufRead) -> io::Result<Counts> {
    let mut c = Counts::default();
    let mut in_word = false;
    loop {
        let chunk = input.fill_buf()?;
        if chunk.is_empty() {
            return Ok(c);
        }
        for &b in chunk {
            c.lines += u64::from(b == b'\n');
            let word_byte = !b.is_ascii_whitespace();
            c.words += u64::from(word_byte && !in_word);
            in_word = word_byte; // state survives chunk boundaries
        }
        let n = chunk.len();
        c.bytes += n as u64;
        input.consume(n);
    }
}

/// Calls `f` with each line INCLUDING its newline, reusing one buffer. Byte based: any input is accepted. `f` returns false to stop.
fn for_each_line(
    mut input: impl BufRead,
    mut f: impl FnMut(&[u8]) -> io::Result<bool>,
) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 || !f(&line)? {
            return Ok(());
        }
    }
}

fn head(input: impl BufRead, out: &mut impl Write, n: usize) -> io::Result<()> {
    if n == 0 {
        return Ok(());
    }
    let mut left = n;
    for_each_line(input, |l| {
        out.write_all(l)?;
        left -= 1;
        Ok(left > 0) // stop WITHOUT reading line n+1: that read could block forever on a live stream
    })
}

fn tail(input: impl BufRead, out: &mut impl Write, n: usize) -> io::Result<()> {
    let mut keep: VecDeque<Vec<u8>> = VecDeque::new();
    for_each_line(input, |l| {
        keep.push_back(l.to_vec());
        if keep.len() > n {
            keep.pop_front();
        }
        Ok(true)
    })?;
    keep.iter().try_for_each(|l| out.write_all(l))
}

/// `uniq -c`: collapses ADJACENT duplicates only.
fn uniq_count(input: impl BufRead, out: &mut impl Write) -> io::Result<()> {
    let mut run: Option<(String, u64)> = None;
    for line in input.lines() {
        let line = line?;
        match &mut run {
            Some((prev, n)) if *prev == line => *n += 1,
            _ => {
                if let Some((prev, n)) = run.replace((line, 1)) {
                    writeln!(out, "{n} {prev}")?;
                }
            }
        }
    }
    if let Some((prev, n)) = run {
        writeln!(out, "{n} {prev}")?; // do not forget the last run
    }
    Ok(())
}

/// Needs all input in memory; byte order, like `LC_ALL=C sort`.
fn sort_lines(input: impl BufRead, out: &mut impl Write) -> io::Result<()> {
    let mut lines = input.split(b'\n').collect::<io::Result<Vec<_>>>()?;
    lines.sort_unstable();
    for l in lines {
        out.write_all(&l)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

/// Substring grep with line numbers. Returns whether anything matched (drives the exit code).
fn grep(input: impl BufRead, out: &mut impl Write, needle: &[u8]) -> io::Result<bool> {
    let (mut no, mut found) = (0u64, false);
    for_each_line(input, |l| {
        no += 1;
        if needle.is_empty() || l.windows(needle.len()).any(|w| w == needle) {
            found = true;
            write!(out, "{no}:")?;
            out.write_all(l)?;
            if !l.ends_with(b"\n") {
                out.write_all(b"\n")?;
            }
        }
        Ok(true)
    })?;
    Ok(found)
}

/// A closed downstream pipe is a normal way for a filter to end.
fn ignore_broken_pipe(r: io::Result<()>) -> io::Result<()> {
    match r {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

/// Prompt, flush, read one line. `None` on EOF. Generic so a test can script the conversation.
fn ask(
    input: &mut impl BufRead,
    out: &mut impl Write,
    question: &str,
) -> io::Result<Option<String>> {
    write!(out, "{question} ")?;
    out.flush()?; // without this the prompt (no newline) stays in the buffer while we wait for the answer
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        return Ok(None); // EOF is not the same as an empty line, which reads as "\n"
    }
    Ok(Some(answer.trim_end_matches(['\n', '\r']).to_string()))
}

// ================================================================ terminal helpers
struct Style {
    colour: bool,
}
impl Style {
    /// The policy as a pure function, so it can be tested without a terminal.
    fn decide(is_terminal: bool, no_color: Option<&std::ffi::OsStr>) -> Style {
        Style {
            colour: is_terminal && no_color.is_none_or(|v| v.is_empty()),
        }
    }
    fn red(&self, s: &str) -> String {
        if self.colour {
            format!("\x1b[31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

/// Removes CSI sequences: ESC '[' parameters... final byte in 0x40..=0x7E. Other escape kinds are left alone.
fn strip_csi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.next_if_eq(&'[').is_some() {
            for c in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&c) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

struct Progress<W: Write> {
    out: W,
    interactive: bool,
    total: u64,
    last_percent: Option<u64>,
}
impl<W: Write> Progress<W> {
    fn bar(percent: u64) -> String {
        let filled = (percent / 10) as usize;
        format!(
            "[{}{}] {percent:>3}%",
            "#".repeat(filled),
            " ".repeat(10 - filled)
        )
    }
    fn update(&mut self, done: u64) -> io::Result<()> {
        let percent = if self.total == 0 {
            100
        } else {
            done.min(self.total) * 100 / self.total
        };
        if self.last_percent == Some(percent) {
            return Ok(()); // redraw only when what the user sees changes
        }
        self.last_percent = Some(percent);
        if self.interactive {
            write!(self.out, "\r{}", Self::bar(percent))?; // \r: back to column 0, overwrite
            if percent == 100 {
                writeln!(self.out)?;
            }
        } else if percent % 25 == 0 {
            writeln!(self.out, "progress {percent}%")?; // in a log file, carriage returns are noise
        }
        self.out.flush()
    }
}

// ================================================================ tests on in-memory streams
fn run<F: FnOnce(Cursor<&[u8]>, &mut Vec<u8>) -> io::Result<()>>(input: &str, f: F) -> String {
    let mut out = Vec::new();
    f(Cursor::new(input.as_bytes()), &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn s01_wc() {
    assert_eq!(wc(Cursor::new("")).unwrap(), Counts::default());
    assert_eq!(
        wc(Cursor::new("one two\nthree\n")).unwrap(),
        Counts {
            lines: 2,
            words: 3,
            bytes: 14
        }
    );
    assert_eq!(
        wc(Cursor::new("no newline")).unwrap().lines,
        0,
        "wc counts newline bytes, not lines of text"
    );
    // chunk boundaries must not change the answer: try every small buffer size
    let text = "  alpha beta\r\n\n gamma\tdelta  \nx".repeat(20);
    let want = wc(Cursor::new(text.as_bytes())).unwrap();
    assert_eq!(want.words as usize, text.split_ascii_whitespace().count());
    for cap in 1..=17 {
        assert_eq!(
            wc(BufReader::with_capacity(cap, text.as_bytes())).unwrap(),
            want,
            "cap {cap}"
        );
    }
    println!("1. wc ok");
}

fn s02_head_tail() {
    let five = "1\n2\n3\n4\n5";
    assert_eq!(run(five, |i, o| head(i, o, 2)), "1\n2\n");
    assert_eq!(
        run(five, |i, o| head(i, o, 99)),
        five,
        "no newline is invented"
    );
    assert_eq!(run(five, |i, o| head(i, o, 0)), "");
    assert_eq!(run(five, |i, o| tail(i, o, 2)), "4\n5");
    assert_eq!(run(five, |i, o| tail(i, o, 0)), "");
    // an ENDLESS input: head must finish. This can only terminate if it stops reading.
    let mut out = Vec::new();
    head(BufReader::new(io::repeat(b'\n')), &mut out, 3).unwrap();
    assert_eq!(out, b"\n\n\n");
    println!("2. head / tail ok");
}

fn s03_text_filters() {
    assert_eq!(
        run("a\na\nb\na\n", |i, o| uniq_count(i, o)),
        "2 a\n1 b\n1 a\n"
    );
    assert_eq!(run("", |i, o| uniq_count(i, o)), "");
    assert_eq!(
        run("b\nB\na10\na9\n\n", |i, o| sort_lines(i, o)),
        "\nB\na10\na9\nb\n",
        "byte order: not numeric, not locale aware"
    );
    // composing filters through buffers: sort | uniq -c
    let mut sorted = Vec::new();
    sort_lines(Cursor::new("x\ny\nx\nx\n"), &mut sorted).unwrap();
    let mut counted = Vec::new();
    uniq_count(sorted.as_slice(), &mut counted).unwrap();
    assert_eq!(counted, b"3 x\n1 y\n");

    let mut out = Vec::new();
    assert!(
        grep(
            Cursor::new(b"ok\n\xff bad bytes error\nfine\nerror"),
            &mut out,
            b"error"
        )
        .unwrap()
    );
    assert_eq!(
        out, b"2:\xff bad bytes error\n4:error\n",
        "non-UTF-8 lines pass through unchanged"
    );
    assert!(!grep(Cursor::new("abc"), &mut Vec::new(), b"zzz").unwrap());
    // lines() demands UTF-8: it reports InvalidData for that line, and iteration can continue after it
    let mut lines = Cursor::new(b"good\n\xff\nafter\r\n").lines();
    assert_eq!(lines.next().unwrap().unwrap(), "good");
    assert_eq!(
        lines.next().unwrap().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        lines.next().unwrap().unwrap(),
        "after",
        "lines() strips \\n and \\r\\n"
    );
    assert!(lines.next().is_none());
    println!("3. uniq / sort / grep ok");
}

/// A downstream that goes away after `room` bytes.
struct ClosingPipe {
    room: usize,
    got: Vec<u8>,
}
impl Write for ClosingPipe {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.room == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = b.len().min(self.room);
        self.room -= n;
        self.got.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn s04_prompt_pipe_errors() {
    let mut input = Cursor::new("yes\r\n\n");
    let mut screen = Vec::new();
    assert_eq!(
        ask(&mut input, &mut screen, "continue?").unwrap(),
        Some("yes".into())
    );
    assert_eq!(
        ask(&mut input, &mut screen, "name?").unwrap(),
        Some(String::new())
    );
    assert_eq!(ask(&mut input, &mut screen, "more?").unwrap(), None);
    assert_eq!(screen, b"continue? name? more? ");

    let mut pipe = ClosingPipe {
        room: 5,
        got: Vec::new(),
    };
    let r = head(Cursor::new("aaaa\nbbbb\ncccc\n"), &mut pipe, 10);
    assert_eq!(r.as_ref().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    assert!(ignore_broken_pipe(r).is_ok());
    assert_eq!(pipe.got, b"aaaa\n");
    assert!(
        ignore_broken_pipe(Err(io::ErrorKind::PermissionDenied.into())).is_err(),
        "other errors still surface"
    );

    // PITFALL: BufWriter hides the failure until flush; Drop would swallow it. Flush explicitly and check.
    let mut buffered = BufWriter::new(ClosingPipe {
        room: 0,
        got: Vec::new(),
    });
    assert!(writeln!(buffered, "lost").is_ok());
    assert_eq!(
        buffered.flush().unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    println!("4. prompts / broken pipe (simulated) ok");
}

fn s05_terminal() {
    let on = Style::decide(true, None);
    let off = Style::decide(false, None);
    assert_eq!(on.red("x"), "\x1b[31mx\x1b[0m");
    assert_eq!(off.red("x"), "x");
    assert!(!Style::decide(true, Some("1".as_ref())).colour);
    // the production wiring: ask the real stream and the real environment
    let real = Style::decide(
        io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").as_deref(),
    );
    assert!(io::stdout().is_terminal() || !real.colour);
    assert!(
        Style::decide(true, Some("".as_ref())).colour,
        "no-color.org: an EMPTY variable does not disable"
    );
    assert_eq!(
        strip_csi(&format!("a {} b \x1b[1;38;5;10mc\x1b[0m", on.red("red"))),
        "a red b c"
    );
    assert_eq!(strip_csi("trailing \x1b["), "trailing ");
    // length is not width: bytes, chars and terminal columns are three different numbers
    let s = on.red("héllo");
    assert_eq!(
        (s.len(), s.chars().count(), strip_csi(&s).chars().count()),
        (15, 14, 5)
    );
    assert_eq!("e\u{301}".chars().count(), 2); // one visible column
    assert_eq!("漢".chars().count(), 1); // usually two columns: std cannot tell you
    assert_eq!(
        format!("{:<5}|", "漢"),
        "漢    |",
        "format width counts chars, so CJK columns misalign"
    );

    let mut p = Progress {
        out: Vec::new(),
        interactive: true,
        total: 1000,
        last_percent: None,
    };
    (0..=1000).for_each(|i| p.update(i).unwrap());
    let drawn = String::from_utf8(p.out).unwrap();
    assert_eq!(
        drawn.matches('\r').count(),
        101,
        "1001 updates, 101 redraws"
    );
    assert!(drawn.ends_with("\r[##########] 100%\n"));
    let mut p = Progress {
        out: Vec::new(),
        interactive: false,
        total: 8,
        last_percent: None,
    };
    (0..=8).for_each(|i| p.update(i).unwrap());
    assert_eq!(
        p.out,
        b"progress 0%\nprogress 25%\nprogress 50%\nprogress 75%\nprogress 100%\n"
    );
    println!("5. terminal helpers ok");
}

// ================================================================ the same code against REAL pipes (self re-exec)
fn child_main(mode: &str) -> io::Result<ExitCode> {
    match mode {
        // a complete filter: lock stdin, lock + buffer stdout, diagnostics on stderr, exit code
        "grep" => {
            let needle = std::env::args_os().nth(3).unwrap_or_default();
            let mut out = BufWriter::new(io::stdout().lock());
            let found = grep(io::stdin().lock(), &mut out, needle.as_encoded_bytes())?;
            out.flush()?;
            eprintln!("grep: done");
            Ok(ExitCode::from(u8::from(!found)))
        }
        "slurp" => {
            let all = io::read_to_string(io::stdin())?;
            print!("{}", all.to_uppercase());
            io::stdout().flush()?; // report a failed flush instead of losing it at exit
            Ok(ExitCode::SUCCESS)
        }
        "first-then-rest" => {
            let mut first = String::new();
            io::stdin().read_line(&mut first)?; // locks per call, so the later lock() below is fine
            let rest: Vec<String> = io::stdin().lines().collect::<io::Result<_>>()?;
            println!("first={first:?} rest={rest:?}");
            Ok(ExitCode::SUCCESS)
        }
        "ttys" => {
            // stdout and stderr locks are re-entrant: println! while holding them does not deadlock
            let _o = io::stdout().lock();
            let _e = io::stderr().lock();
            println!(
                "{} {} {}",
                io::stdin().is_terminal(),
                io::stdout().is_terminal(),
                io::stderr().is_terminal()
            );
            Ok(ExitCode::SUCCESS)
        }
        // both writers wait for stdin EOF, which the parent sends only AFTER closing the read end of stdout
        "spam" => {
            io::read_to_string(io::stdin())?;
            let mut out = io::stdout().lock();
            let r = (0..100_000).try_for_each(|i| writeln!(out, "line {i}"));
            if r.as_ref()
                .is_err_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
            {
                eprintln!("downstream closed");
            }
            ignore_broken_pipe(r)?;
            Ok(ExitCode::SUCCESS)
        }
        "spam-println" => {
            io::read_to_string(io::stdin())?;
            (0..100_000).for_each(|i| println!("line {i}"));
            Ok(ExitCode::SUCCESS)
        }
        _ => Ok(ExitCode::from(2)), // 2 is the conventional "usage error"
    }
}

fn spawn_child(args: &[&str]) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .arg("--child")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn feed(args: &[&str], input: &[u8]) -> (i32, String, String) {
    let mut child = spawn_child(args);
    child.stdin.take().unwrap().write_all(input).unwrap(); // small inputs only: large ones need a writer thread
    let o = child.wait_with_output().unwrap();
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn s06_real_pipes() {
    assert_eq!(
        feed(&["grep", "an"], b"apple\nbanana\nmango\n"),
        (0, "2:banana\n3:mango\n".into(), "grep: done\n".into())
    );
    assert_eq!(feed(&["grep", "zzz"], b"apple\n").0, 1);
    assert_eq!(feed(&["slurp"], b"a\nb").1, "A\nB");
    assert_eq!(
        feed(&["first-then-rest"], b"h\nr1\nr2\n").1,
        "first=\"h\\n\" rest=[\"r1\", \"r2\"]\n"
    );
    assert_eq!(feed(&["ttys"], b"").1, "false false false\n");
    assert_eq!(feed(&["nonsense"], b"").0, 2);

    for (mode, want_code) in [("spam", 0), ("spam-println", 101)] {
        let mut child = spawn_child(&[mode]);
        drop(child.stdout.take()); // the reader goes away, like `| head` after its last line
        drop(child.stdin.take()); // now let the child start writing
        let mut err = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(want_code), "{mode}: {err}");
        if mode == "spam" {
            assert_eq!(err, "downstream closed\n");
        } else {
            assert!(
                err.contains("failed printing to stdout"),
                "println! panicked: {err}"
            );
        }
    }
    println!("6. real pipes ok");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().take(3).collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        return child_main(args.get(2).map_or("", String::as_str)).unwrap_or_else(|e| {
            eprintln!("error: {e}"); // errors go to stderr, results to stdout
            ExitCode::FAILURE
        });
    }
    s01_wc();
    s02_head_tail();
    s03_text_filters();
    s04_prompt_pipe_errors();
    s05_terminal();
    s06_real_pipes();
    println!("all stdio sections passed");
    ExitCode::SUCCESS
}
