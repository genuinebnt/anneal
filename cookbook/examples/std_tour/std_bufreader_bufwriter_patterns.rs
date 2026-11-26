//! RECIPE 1061 (std_tour): `BufReader`, `BufWriter`, `LineWriter` in depth, with syscall counting.
//! LEVEL: intermediate
//! Use in: log tailers, CSV/JSONL pipelines, grep-like tools, anything that does many small reads or writes.   Run: cargo run --release --example std_bufreader_bufwriter_patterns
//! COMMON USES: `BufWriter::new(File::create(..))` for exports; `BufReader::new(File::open(..)).lines()`; `stdout().lock()` + `BufWriter` for CLI output; reusing one `String` for `read_line` in hot loops.
//! METHODS: BufReader::new, BufWriter::new, BufRead::lines, BufRead::read_line, BufWriter::flush, BufWriter::with_capacity, BufReader::with_capacity, BufWriter::buffer, BufWriter::capacity, BufReader::buffer, BufReader::capacity, BufWriter::get_ref, BufWriter::get_mut, BufReader::get_ref, BufReader::get_mut, BufReader::into_inner, BufWriter::into_inner, BufWriter::into_parts, std::io::IntoInnerError, IntoInnerError::error, IntoInnerError::into_inner, IntoInnerError::into_error, IntoInnerError::into_parts, BufReader::seek_relative, Seek::seek, LineWriter::new, LineWriter::with_capacity, LineWriter::get_ref, LineWriter::into_inner, std::io::stdin, Stdin::lock, std::io::stdout, Stdout::lock, BufRead::read_until, std::io::WriterPanicked
//! DEFINED IN: std: `BufReader`, `BufWriter`, `LineWriter`, `IntoInnerError`, `BufRead`, `Write`, `stdin`/`stdout` are all `std::io`.
//! DOCS: https://doc.rust-lang.org/std/io/struct.BufWriter.html
//! NOT COVERED (and why): BufReader::peek (unstable in 1.98.1); BufRead::has_data_left (unstable).
//! Course: none. Goes beyond `buffered_io_internals` (how a buffer works) by covering the full std API and failure semantics.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   write many small pieces to a file           -> `BufWriter::new(file)`; call `flush()` (or `into_inner()`) and CHECK it
//!   read a file line by line                    -> `BufReader::new(file).lines()`; hot loop: reuse a String with `read_line` + `clear`
//!   tune the buffer                             -> `with_capacity(64 * 1024)` for big sequential I/O
//!   see unflushed bytes                         -> `BufWriter::buffer()`, `capacity()`
//!   get the file back AND learn about failure   -> `into_inner()` (flushes; Err keeps the data)
//!   get unflushed bytes without any I/O         -> `into_parts()` => `(writer, Ok(buffered_bytes))`
//!   interactive output (flush per line)         -> `LineWriter` (stdout already is one)
//!   jump a little inside a BufReader            -> `seek_relative` (keeps the buffer if within it)
//!   fast stdout for 1M lines                    -> `BufWriter::new(stdout().lock())` + `writeln!`
//!   read stdin lines                            -> `stdin().lock().lines()`
//!   count syscalls                              -> wrap the inner Read/Write in a counter
//!   read until a delimiter                      -> `read_until(b';', &mut Vec)`
//!
//! Design: a `CountingWrite` wrapper stands in for the kernel: each `write` call it sees is one would-be syscall.
//! Read: https://doc.rust-lang.org/std/io/struct.BufWriter.html (see the Drop and `into_inner` notes)
//! Read: https://doc.rust-lang.org/std/io/struct.BufReader.html
//! TRY NEXT:
//!   - [S] Change `with_capacity(8192)` to 64 and re-run section 2: which assertion on the call count breaks first and why?
//!   - [S] Predict: what does a `BufWriter<LineWriter<_>>` do on `writeln!`? Wrap it, count inner calls, check.
//!   - [M] Make `Tailer::poll` survive log rotation where the new file is LONGER than the old offset (hint: compare inode on unix, or remember the first line).
//!   - [M] Add a `read_line_into` variant to `grep_offsets` that refuses lines over 1 MiB using `Read::take` (see std_io_read_write_seek_traits).
//!   - [A] PROJECT: combine with `std_fs_file_operations_usecases` (atomic write + rotation): a log shipper that tails files with `Tailer`, batches JSONL through a `BufWriter`, and rotates by size.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::fs::{self, File, OpenOptions};
use std::io::{
    self, BufRead, BufReader, BufWriter, Cursor, ErrorKind, LineWriter, Read, Seek, SeekFrom, Write,
};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

const COVERED: &[&str] = &[
    "BufReader::new",
    "BufWriter::new",
    "lines",
    "read_line",
    "flush",
    "with_capacity",
    "buffer",
    "capacity",
    "get_ref",
    "get_mut",
    "into_inner",
    "into_parts",
    "IntoInnerError",
    "seek_relative",
    "seek",
    "LineWriter",
    "stdin_lock",
    "stdout_lock",
    "read_until",
    "WriterPanicked",
];

struct TempDir(PathBuf);
static CTR: AtomicU32 = AtomicU32::new(0);
impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "cb1061_{tag}_{}_{}",
            std::process::id(),
            CTR.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Stand-in for the OS: counts `write` calls (would-be syscalls) and bytes.
#[derive(Default)]
struct CountingWrite {
    calls: usize,
    bytes: usize,
    data: Vec<u8>,
    keep: bool,
}
impl Write for CountingWrite {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        self.bytes += b.len();
        if self.keep {
            self.data.extend_from_slice(b);
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Counts inner `read` and `seek` calls under a BufReader.
struct CountingRw<T> {
    inner: T,
    reads: Rc<Cell<usize>>,
    seeks: Rc<Cell<usize>>,
}
impl<T: Read> Read for CountingRw<T> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read(b)
    }
}
impl<T: Seek> Seek for CountingRw<T> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.seeks.set(self.seeks.get() + 1);
        self.inner.seek(p)
    }
}

/// A sink that can be told to fail; state is shared so we can look at it after the writer is dropped.
#[derive(Default)]
struct Shared {
    data: Vec<u8>,
    fail: bool,
}
struct FailableSink(Rc<RefCell<Shared>>);
impl Write for FailableSink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let mut s = self.0.borrow_mut();
        if s.fail {
            return Err(io::Error::other("disk full"));
        }
        s.data.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn section(n: u32, t: &str) {
    println!("\n== {n}. {t}");
}

// ---------------------------------------------------------------- 1 everyday

fn everyday_core() {
    section(
        1,
        "Everyday core: BufReader::new + lines, BufWriter::new + flush, stdout lock",
    );
    let d = TempDir::new("core");
    let p = d.path("data.txt");
    {
        let mut w = BufWriter::new(File::create(&p).unwrap());
        for i in 0..1000 {
            writeln!(w, "line {i}").unwrap();
        }
        w.flush().unwrap(); // explicit flush: errors are reported here, not lost in Drop
    }
    let r = BufReader::new(File::open(&p).unwrap());
    let lines: Vec<String> = r.lines().collect::<Result<_, _>>().unwrap(); // first Err wins
    assert_eq!(lines.len(), 1000);
    assert_eq!(lines[999], "line 999");

    // The generic shape for CLI tools: `stdin().lock()` is a BufRead, `stdout().lock()` a Write.
    fn run<R: BufRead, W: Write>(r: R, mut w: W) -> io::Result<usize> {
        let mut n = 0;
        for l in r.lines() {
            writeln!(w, "> {}", l?)?;
            n += 1;
        }
        Ok(n)
    }
    let stdin = io::stdin();
    let _typed: io::StdinLock<'_> = stdin.lock(); // not read here: only showing the types line up
    drop(_typed);
    let mut out = Vec::new();
    assert_eq!(run(Cursor::new("a\nb"), &mut out).unwrap(), 2);
    assert_eq!(out, b"> a\n> b\n");
    // Real stdout: lock once, not per line (each println! locks and, being a LineWriter, flushes per line).
    let so = io::stdout();
    let mut lock = BufWriter::new(so.lock());
    writeln!(lock, "stdout via BufWriter(StdoutLock): flush before exit").unwrap();
    lock.flush().unwrap();
    println!("everyday ok");
}

// ---------------------------------------------------------------- 2 syscall counting

fn syscall_counting() {
    section(
        2,
        "Writing 100k small lines: syscalls with and without buffering",
    );
    const N: usize = 100_000;
    let mut raw = CountingWrite::default();
    for i in 0..N {
        writeln!(raw, "{i}").unwrap();
    } // write_fmt: several write calls per line
    let raw_calls = raw.calls;

    let mut bw = BufWriter::with_capacity(8192, CountingWrite::default());
    for i in 0..N {
        writeln!(bw, "{i}").unwrap();
    }
    bw.flush().unwrap();
    let buffered = bw.get_ref();
    let expected_max = buffered.bytes / 8192 + 2;
    println!(
        "unbuffered: {raw_calls} calls; BufWriter(8K): {} calls for {} bytes",
        buffered.calls, buffered.bytes
    );
    assert_eq!(raw.bytes, buffered.bytes, "same bytes either way");
    assert!(
        raw_calls >= N,
        "at least one syscall per line without a buffer"
    );
    assert!(
        buffered.calls <= expected_max,
        "buffered calls ~ bytes/capacity"
    );
    assert!(raw_calls > 50 * buffered.calls);

    // LineWriter: one write per line (good for logs/terminals, still N calls).
    let mut lw = LineWriter::new(CountingWrite::default());
    for i in 0..1000 {
        writeln!(lw, "{i}").unwrap();
    }
    assert_eq!(lw.get_ref().calls, 1000);

    // A single write larger than the capacity bypasses the buffer (no double copy).
    let mut bw = BufWriter::with_capacity(16, CountingWrite::default());
    bw.write_all(&[1u8; 100]).unwrap();
    assert_eq!(bw.get_ref().calls, 1);
    assert_eq!(bw.buffer().len(), 0);
    // Reads: 1-byte reads over BufReader cost one inner read per buffer fill, not per byte.
    let reads = Rc::new(Cell::new(0));
    let inner = CountingRw {
        inner: Cursor::new(vec![0u8; 10_000]),
        reads: reads.clone(),
        seeks: Rc::new(Cell::new(0)),
    };
    let mut br = BufReader::with_capacity(4096, inner);
    let mut one = [0u8; 1];
    for _ in 0..10_000 {
        br.read_exact(&mut one).unwrap();
    }
    println!("10k 1-byte reads => {} inner reads", reads.get());
    assert!(reads.get() <= 4);
    println!("syscall counting ok");
}

// ---------------------------------------------------------------- 3 BufWriter internals/errors

fn bufwriter_api() {
    section(
        3,
        "BufWriter: buffer/capacity, into_inner, into_parts, Drop swallowing errors",
    );
    let mut bw = BufWriter::with_capacity(64, Vec::new());
    assert_eq!(bw.capacity(), 64);
    bw.write_all(b"hello").unwrap();
    assert_eq!(bw.buffer(), b"hello"); // unflushed bytes are visible
    assert!(bw.get_ref().is_empty()); // ...but not yet in the inner writer
    bw.get_mut().extend_from_slice(b"[direct]"); // writing around the buffer reorders output: avoid
    let v = bw.into_inner().unwrap(); // flushes then returns the Vec
    assert_eq!(v, b"[direct]hello");

    // into_parts: take the buffered bytes WITHOUT writing them (cancel / transfer).
    let mut bw = BufWriter::new(Vec::new());
    bw.write_all(b"pending").unwrap();
    let (inner, pending) = bw.into_parts();
    assert!(inner.is_empty());
    assert_eq!(pending.unwrap(), b"pending");

    // WriterPanicked: if the inner writer PANICKED mid-write, nobody knows which bytes got out. into_parts then returns the
    // buffer inside an error, to make you decide, instead of letting it look like cleanly unwritten data.
    struct PanickingSink;
    impl Write for PanickingSink {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            panic!("sink panicked")
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bw = BufWriter::new(PanickingSink);
    bw.write_all(b"in doubt").unwrap(); // only buffered
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let flushed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bw.flush()));
    std::panic::set_hook(old_hook);
    assert!(flushed.is_err());
    let (_sink, pending) = bw.into_parts();
    let panicked: std::io::WriterPanicked = pending.unwrap_err();
    assert_eq!(panicked.into_inner(), b"in doubt");

    // into_inner on failure: IntoInnerError hands the BufWriter back with data intact.
    let st = Rc::new(RefCell::new(Shared {
        fail: true,
        ..Default::default()
    }));
    let mut bw = BufWriter::new(FailableSink(st.clone()));
    bw.write_all(b"important").unwrap(); // accepted into memory, no error yet!
    let err = bw.into_inner().err().expect("flush must fail");
    assert_eq!(err.error().to_string(), "disk full");
    let mut bw = err.into_inner(); // recover the writer, bytes still buffered
    assert_eq!(bw.buffer(), b"important");
    st.borrow_mut().fail = false; // "disk freed"
    bw.flush().unwrap();
    assert_eq!(st.borrow().data, b"important");

    // PITFALL: Drop flushes but IGNORES errors => silent data loss.
    let st = Rc::new(RefCell::new(Shared {
        fail: true,
        ..Default::default()
    }));
    {
        let mut bw = BufWriter::new(FailableSink(st.clone()));
        bw.write_all(b"lost").unwrap();
    } // dropped: flush failed, nobody was told
    assert!(st.borrow().data.is_empty());
    // FIX: flush explicitly and propagate.
    let st = Rc::new(RefCell::new(Shared {
        fail: true,
        ..Default::default()
    }));
    let mut bw = BufWriter::new(FailableSink(st.clone()));
    bw.write_all(b"lost").unwrap();
    let e = bw.flush().unwrap_err();
    assert_eq!(e.to_string(), "disk full");
    st.borrow_mut().fail = false;
    drop(bw); // the retry in Drop now succeeds: failed flush kept the bytes
    assert_eq!(st.borrow().data, b"lost");
    println!("bufwriter ok");
}

// ---------------------------------------------------------------- 4 BufReader API + seek

fn bufreader_api() {
    section(
        4,
        "BufReader: buffer/capacity, into_inner, seek vs seek_relative",
    );
    let data: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let (reads, seeks) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let inner = CountingRw {
        inner: Cursor::new(data.clone()),
        reads: reads.clone(),
        seeks: seeks.clone(),
    };
    let mut br = BufReader::with_capacity(1024, inner);
    assert_eq!(br.capacity(), 1024);
    assert!(br.buffer().is_empty());
    let mut b = [0u8; 10];
    br.read_exact(&mut b).unwrap();
    assert_eq!(
        br.buffer().len(),
        1014,
        "rest of the first fill is still buffered"
    );
    let s0 = seeks.get();
    br.seek_relative(5).unwrap(); // within the buffer: no inner seek
    assert_eq!(seeks.get(), s0, "seek_relative inside the buffer is free");
    br.read_exact(&mut b).unwrap();
    assert_eq!(b[0], 15);
    br.seek(SeekFrom::Current(0)).unwrap(); // generic seek asks the inner object (and drops the buffer)
    assert!(seeks.get() > s0);
    assert!(br.buffer().is_empty());
    br.seek(SeekFrom::Start(300)).unwrap();
    br.read_exact(&mut b).unwrap();
    assert_eq!(b[0], (300 % 256) as u8);
    // stream_position accounts for buffered-but-unread bytes.
    assert_eq!(br.stream_position().unwrap(), 310);
    let inner = br.into_inner(); // buffered bytes are discarded
    assert_eq!(
        inner.inner.position() > 310,
        true,
        "inner is ahead of the logical position"
    );
    println!("bufreader ok");
}

// ---------------------------------------------------------------- 5 line oriented hot paths

fn huge_file_no_alloc() {
    section(
        5,
        "Reading a big file line by line without allocating per line",
    );
    let d = TempDir::new("big");
    let p = d.path("big.txt");
    {
        let mut w = BufWriter::with_capacity(1 << 16, File::create(&p).unwrap());
        for i in 0..200_000u32 {
            writeln!(w, "{:08}: {}", i, "x".repeat((i % 40) as usize)).unwrap();
        }
        w.flush().unwrap();
    }
    let mut r = BufReader::with_capacity(1 << 16, File::open(&p).unwrap());
    let mut line = String::new(); // one allocation reused for every line
    let (mut n, mut bytes, mut longest) = (0u32, 0usize, 0usize);
    loop {
        line.clear(); // read_line appends: forgetting this concatenates lines
        let got = r.read_line(&mut line).unwrap();
        if got == 0 {
            break;
        }
        n += 1;
        bytes += got;
        longest = longest.max(line.len());
    }
    assert_eq!(n, 200_000);
    assert_eq!(bytes as u64, fs::metadata(&p).unwrap().len());
    assert!(
        line.capacity() >= longest && line.capacity() < 4 * longest,
        "String grew once to the longest line"
    );
    // The same through `lines()` allocates 200k Strings: fine for 1k lines, wasteful for 200M.
    let count = BufReader::new(File::open(&p).unwrap()).lines().count();
    assert_eq!(count, 200_000);
    // read_until for non-newline records (NUL-separated `find -print0`, `;`-separated)
    let mut r = Cursor::new(b"a\0bb\0ccc".to_vec());
    let mut rec = Vec::new();
    let mut recs = vec![];
    while r.read_until(0, &mut rec).unwrap() > 0 {
        if rec.last() == Some(&0) {
            rec.pop();
        }
        recs.push(String::from_utf8(rec.clone()).unwrap());
        rec.clear();
    }
    assert_eq!(recs, ["a", "bb", "ccc"]);
    println!("huge file ok");
}

// ---------------------------------------------------------------- 6 scenario: grep offsets

#[derive(Debug, PartialEq)]
struct Hit {
    offset: u64,
    line_no: usize,
    text: String,
}

fn grep_offsets<R: BufRead>(mut r: R, needle: &str) -> io::Result<Vec<Hit>> {
    let (mut off, mut no, mut hits) = (0u64, 0usize, vec![]);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = r.read_until(b'\n', &mut buf)?; // bytes, not String: tolerate invalid UTF-8
        if n == 0 {
            break;
        }
        no += 1;
        let text = String::from_utf8_lossy(&buf);
        if text.contains(needle) {
            hits.push(Hit {
                offset: off,
                line_no: no,
                text: text.trim_end_matches(['\n', '\r']).to_string(),
            });
        }
        off += n as u64; // advance by what was CONSUMED (includes the delimiter), not trimmed length
    }
    Ok(hits)
}

fn grep_section() {
    section(6, "Scenario: grep-like filter with byte offsets");
    let text = "alpha\r\nbeta ERROR x\nge\u{e4}mma\n\nERROR last";
    let hits = grep_offsets(Cursor::new(text), "ERROR").unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits[0],
        Hit {
            offset: 7,
            line_no: 2,
            text: "beta ERROR x".into()
        }
    );
    // offsets must point at the line start in the original bytes: verify against an oracle.
    for h in &hits {
        assert!(text.as_bytes()[h.offset as usize..].starts_with(h.text.as_bytes()));
    }
    // seek straight to a hit and re-read it: the use of offsets (an index).
    let mut c = Cursor::new(text);
    c.seek(SeekFrom::Start(hits[1].offset)).unwrap();
    let mut l = String::new();
    BufReader::new(c).read_line(&mut l).unwrap();
    assert_eq!(l, "ERROR last");
    // invalid UTF-8 does not abort the scan (lines() would return InvalidData).
    let bad = [b'o', b'k', b'\n', 0xff, b'E', b'R', b'R', b'\n'];
    assert_eq!(grep_offsets(&bad[..], "ER").unwrap().len(), 1);
    assert_eq!(
        BufReader::new(&bad[..])
            .lines()
            .nth(1)
            .unwrap()
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    println!("grep ok");
}

// ---------------------------------------------------------------- 7 scenario: tailer

struct Tailer {
    path: PathBuf,
    pos: u64,
}
impl Tailer {
    /// New complete lines since the last call. A trailing partial line stays unread.
    fn poll(&mut self) -> io::Result<Vec<String>> {
        let mut f = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]), // not rotated in yet
            Err(e) => return Err(e),
        };
        let len = f.metadata()?.len();
        if len < self.pos {
            self.pos = 0;
        } // truncated or rotated: start over
        f.seek(SeekFrom::Start(self.pos))?;
        let mut r = BufReader::new(f);
        let (mut out, mut line) = (vec![], String::new());
        loop {
            line.clear();
            let n = r.read_line(&mut line)?;
            if n == 0 || !line.ends_with('\n') {
                break;
            } // EOF or partial line: retry next time
            self.pos += n as u64;
            out.push(line.trim_end().to_string());
        }
        Ok(out)
    }
}

fn tailer_section() {
    section(7, "Scenario: log file tailer (partial lines, truncation)");
    let d = TempDir::new("tail");
    let p = d.path("app.log");
    let mut t = Tailer {
        path: p.clone(),
        pos: 0,
    };
    assert!(t.poll().unwrap().is_empty()); // file does not exist yet
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .unwrap();
    f.write_all(b"one\ntwo\npart").unwrap();
    assert_eq!(t.poll().unwrap(), ["one", "two"]);
    assert!(t.poll().unwrap().is_empty(), "no duplicates on re-poll");
    f.write_all(b"ial\nthree\n").unwrap();
    assert_eq!(t.poll().unwrap(), ["partial", "three"]);
    fs::write(&p, b"new\n").unwrap(); // truncate + rewrite (shorter than pos)
    assert_eq!(t.poll().unwrap(), ["new"]);
    println!("tailer ok");
}

// ---------------------------------------------------------------- 8 scenario: CSV + JSONL

fn csv_field(f: &str, out: &mut impl Write) -> io::Result<()> {
    if f.contains([',', '"', '\n', '\r']) {
        write!(out, "\"{}\"", f.replace('"', "\"\""))
    } else {
        out.write_all(f.as_bytes())
    }
}
fn write_csv<W: Write>(w: W, rows: impl Iterator<Item = Vec<String>>) -> io::Result<u64> {
    let mut w = BufWriter::new(w);
    let mut n = 0;
    for row in rows {
        for (i, f) in row.iter().enumerate() {
            if i > 0 {
                w.write_all(b",")?;
            }
            csv_field(f, &mut w)?;
        }
        w.write_all(b"\n")?;
        n += 1;
    }
    w.flush()?; // surface errors
    Ok(n)
}

fn read_jsonl<R: BufRead>(r: R) -> impl Iterator<Item = Result<serde_json::Value, String>> {
    r.lines().enumerate().filter_map(|(i, l)| {
        let l = match l {
            Ok(l) => l,
            Err(e) => return Some(Err(format!("line {}: io: {e}", i + 1))),
        };
        if l.trim().is_empty() {
            return None;
        }
        Some(serde_json::from_str(&l).map_err(|e| format!("line {}: {e}", i + 1)))
    })
}

fn csv_jsonl_section() {
    section(8, "Scenarios: streaming CSV export, JSONL reader");
    let rows = (0..50_000).map(|i| {
        vec![
            i.to_string(),
            format!("name, {i}"),
            "say \"hi\"".to_string(),
        ]
    });
    let sink = CountingWrite {
        keep: true,
        ..Default::default()
    };
    let mut sink = sink;
    let n = write_csv(&mut sink, rows).unwrap();
    assert_eq!(n, 50_000);
    assert!(sink.calls < sink.bytes / 8000 + 3, "streaming + buffered");
    let head = String::from_utf8_lossy(&sink.data[..40]).to_string();
    assert!(
        head.starts_with("0,\"name, 0\",\"say \"\"hi\"\"\"\n1,"),
        "{head}"
    );

    let doc = "{\"a\":1}\n\n{\"a\":2}\nnot json\n{\"a\":4}\n";
    let results: Vec<_> = read_jsonl(Cursor::new(doc)).collect();
    assert_eq!(results.len(), 4);
    assert_eq!(results[1].as_ref().unwrap()["a"], 2);
    assert!(
        results[2].as_ref().unwrap_err().starts_with("line 4:"),
        "line numbers count blank lines"
    );
    let ok_sum: i64 = results
        .iter()
        .flatten()
        .filter_map(|v| v["a"].as_i64())
        .sum();
    assert_eq!(ok_sum, 7); // one bad line does not stop the stream
    println!("csv/jsonl ok");
}

// ---------------------------------------------------------------- 9 LineWriter

fn linewriter_section() {
    section(9, "LineWriter: flush on newline only");
    let mut lw = LineWriter::with_capacity(256, Vec::new());
    lw.write_all(b"first\nsecond").unwrap();
    assert_eq!(
        lw.get_ref(),
        b"first\n",
        "only complete lines reach the inner writer"
    );
    lw.write_all(b" half\nthird").unwrap();
    assert_eq!(lw.get_ref(), b"first\nsecond half\n");
    let v = lw.into_inner().unwrap(); // flushes the trailing partial line
    assert_eq!(v, b"first\nsecond half\nthird");
    println!("linewriter ok");
}

// ---------------------------------------------------------------- 10 production tips

fn production_tips() {
    section(10, "Production tips and tricks");
    // 1. Wrap raw Files that get many small writes (anti-pattern: writeln! on a bare File, one syscall per piece).
    let mut raw = CountingWrite::default();
    for i in 0..100 {
        writeln!(raw, "{i}").unwrap();
    }
    let mut buf = BufWriter::new(CountingWrite::default());
    for i in 0..100 {
        writeln!(buf, "{i}").unwrap();
    }
    buf.flush().unwrap();
    assert!(raw.calls > 10 * buf.get_ref().calls);
    // 2. Do NOT double-buffer: BufReader around Stdin lock/File is right, but BufReader over a BufReader or a
    //    Vec/Cursor just copies. (clippy has no lint; review item.) Cursor is already a BufRead:
    fn takes_bufread(r: impl BufRead) -> usize {
        r.lines().count()
    }
    assert_eq!(takes_bufread(Cursor::new("a\nb")), 2);
    assert_eq!(takes_bufread(&b"a\nb\nc"[..]), 3); // &[u8] too
    // 3. Don't wrap stdin in BufReader: Stdin is already buffered internally. Use stdin().lock().
    // 4. Flush explicitly and propagate the error (clippy::unused_io_amount is related; Drop swallows errors).
    // 5. Pre-size the buffer for known big transfers: with_capacity(64 KiB) beats the 8 KiB default for sequential I/O.
    assert_eq!(
        BufWriter::with_capacity(65536, Vec::new()).capacity(),
        65536
    );
    // 6. After into_inner, the unflushed tail is flushed: use it to atomically hand back the File for sync_all().
    let d = TempDir::new("tips");
    let p = d.path("f");
    let f = File::create(&p).unwrap();
    let mut w = BufWriter::new(f);
    w.write_all(b"data").unwrap();
    let f = w.into_inner().map_err(|e| e.into_error()).unwrap();
    f.sync_all().unwrap(); // durability is the File's job, not the BufWriter's
    assert_eq!(fs::read(&p).unwrap(), b"data");
    // 7. Reuse a `String`/`Vec` with `clear()` in loops (clippy: none for this; `lines()` in a hot loop is the smell).
    // 8. Avoid `print!` in loops (stdout is line buffered; each call locks): lock once and use writeln!.
    let so = io::stdout();
    let mut l = so.lock();
    writeln!(l, "(tip 8) locked once").unwrap();
    drop(l);
    // 9. Handle BrokenPipe when piping into `head`: treat as clean exit, not a panic (println! panics on error).
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let r = writeln!(Closed, "x");
    assert!(matches!(r, Err(ref e) if e.kind() == ErrorKind::BrokenPipe));
    // 10. `lines()` result is io::Result<String>: `.map_while(Result::ok)` stops at the first error (an infinite
    //     error loop is possible with plain `.flatten()` on some readers: clippy::lines_filter_map_ok flags it).
    let bad = [b'a', b'\n', 0xff, b'\n', b'b', b'\n'];
    let good: Vec<String> = BufReader::new(&bad[..])
        .lines()
        .map_while(Result::ok)
        .collect();
    assert_eq!(good, ["a"]);
    // 11. Don't read-modify-write through BufReader+BufWriter on the same File without seeking: buffers go stale.
    //     Open two handles (try_clone shares the cursor!) or rewrite via temp + rename.
    println!("tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|c| seen.insert(*c)),
        "COVERED has duplicates"
    );
    everyday_core();
    syscall_counting();
    bufwriter_api();
    bufreader_api();
    huge_file_no_alloc();
    grep_section();
    tailer_section();
    csv_jsonl_section();
    linewriter_section();
    production_tips();
    println!("\nall sections passed ({} methods listed)", COVERED.len());
}
