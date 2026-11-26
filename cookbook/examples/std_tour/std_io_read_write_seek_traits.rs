//! RECIPE 1060 (std_tour): `Read`, `Write`, `BufRead`, `Seek`, `Cursor` and `io::Error` by realistic example.
//! LEVEL: intermediate
//! Use in: protocol parsers, file formats, tests of I/O code without files.   Run: cargo run --release --example std_io_read_write_seek_traits
//! COMMON USES: generic `fn f<R: Read, W: Write>` functions tested on `Cursor`/`Vec<u8>`; length-limited reads on untrusted input; tee/counting adapters (metrics, checksums); retry on `Interrupted`.
//! METHODS: Read::read, Read::read_exact, Read::read_to_end, Read::read_to_string, Write::write_all, Write::flush, BufRead::read_line, BufRead::lines, Read::take, Write::write, Write::write_fmt, BufRead::read_until, BufRead::split, BufRead::fill_buf, BufRead::consume, BufRead::skip_until, Read::bytes, Read::chain, Read::by_ref, Write::by_ref, Write::write_vectored, std::io::IoSlice, Seek::seek, Seek::stream_position, Seek::rewind, Seek::seek_relative, std::io::SeekFrom, std::io::Cursor, Cursor::new, Cursor::position, Cursor::set_position, Cursor::get_ref, Cursor::into_inner, std::io::copy, std::io::sink, std::io::empty, std::io::repeat, std::io::read_to_string, std::io::Take::limit, std::io::Take::set_limit, std::io::Error::kind, std::io::Error::from_raw_os_error, std::io::Error::raw_os_error, std::io::Error::other, std::io::Error::new, std::io::Error::get_ref, std::io::Error::into_inner, std::error::Error::source, std::io::ErrorKind
//! DEFINED IN: std for use. In the 1.98.1 source the traits `Read`, `Write`, `Seek`, `BufRead` and `SeekFrom` are in `std::io`;
//!   `Error`, `ErrorKind`, `Cursor`, `Take`, `Chain`, `Empty`, `Sink`, `Repeat`, `IoSlice` have their source in `core::io`, which is still unstable (`core_io`), so import from `std::io`.
//! DOCS: https://doc.rust-lang.org/std/io/index.html
//! NOT COVERED (and why): Read::read_buf / read_buf_exact / Seek::stream_len / BufRead::has_data_left / Write::write_all_vectored (nightly); Read::read_vectored (same shape as write_vectored); Read::read_to_end on huge files (see buffered_io_internals).
//! Course: none. Pairs with `io_files_paths` (basic Read/Write) and `buffered_io_internals`; this file goes beyond them into every provided method.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   read whatever is available, once            -> `Read::read` (may return fewer bytes than asked!)
//!   fill a fixed header exactly                 -> `Read::read_exact` (EOF mid-way => UnexpectedEof)
//!   slurp a small file / stream                 -> `read_to_end`, `read_to_string`, `io::read_to_string(r)`
//!   write everything                            -> `Write::write_all` (never bare `write`), then `flush`
//!   formatted output                            -> `write!` / `writeln!` (=> `write_fmt`)
//!   process line by line                        -> `BufRead::lines` or reuse a String with `read_line`
//!   split by a delimiter byte                   -> `read_until`, `split`, `skip_until`
//!   peek without consuming                      -> `fill_buf` + `consume`
//!   cap what untrusted input can cost           -> `Read::take(max)`
//!   read a header then pass the rest on         -> `by_ref()` (keeps ownership of the reader)
//!   concatenate two readers                     -> `chain`
//!   test I/O code without files                 -> `Cursor<Vec<u8>>`, `&[u8]` as Read, `Vec<u8>` as Write
//!   go back to the start / jump to the end      -> `rewind`, `seek(SeekFrom::End(-n))`
//!   discard / count output                      -> `io::sink()`, `io::copy(&mut r, &mut sink())`
//!   branch on the failure kind                  -> `err.kind()` and `ErrorKind::{NotFound, UnexpectedEof, Interrupted, WouldBlock}`
//!   wrap a custom error                         -> `io::Error::other(e)`, recover with `get_ref().downcast_ref()`
//!
//! Design: every section is a small scenario with asserts. Nothing here sleeps or depends on the clock.
//! Read: https://doc.rust-lang.org/std/io/index.html  (the module page lists the traits and `Cursor`)
//! Read: https://doc.rust-lang.org/std/io/trait.Read.html
//! TRY NEXT:
//!   - [S] Change `OneByte` to return `ErrorKind::Interrupted` every third call and make `parse_lines_streaming` still pass without touching it. Which provided methods already retry for you?
//!   - [S] Predict, then run: what does `Cursor::new(vec![]).write_all` at `set_position(5)` leave in the Vec (zero fill)? Assert it.
//!   - [M] Port `read_tlv` to also verify a trailing CRC byte; keep the `take` DoS bound and fuzz it with truncated inputs.
//!   - [M] Add a `RateLimited<R>` reader that returns at most N bytes per call; run the TLV and line parsers over it.
//!   - [A] PROJECT: combine this with `std_bufreader_bufwriter_patterns` and `std_hash_collections_io_serialization_formats`: a streaming record reader over `TcpStream`-like input with a `Counting` metric and a size cap.
#![allow(dead_code)]

use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Cursor, ErrorKind, IoSlice, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

const COVERED: &[&str] = &[
    "read",
    "read_exact",
    "read_to_end",
    "read_to_string",
    "write_all",
    "flush",
    "read_line",
    "lines",
    "take",
    "write",
    "write_fmt",
    "read_until",
    "split",
    "fill_buf",
    "consume",
    "skip_until",
    "bytes",
    "chain",
    "by_ref",
    "write_vectored",
    "seek",
    "stream_position",
    "rewind",
    "seek_relative",
    "Cursor",
    "copy",
    "sink",
    "empty",
    "repeat",
    "read_to_string_fn",
    "limit",
    "set_limit",
    "kind",
    "from_raw_os_error",
    "raw_os_error",
    "other",
    "new",
    "get_ref",
    "into_inner",
    "source",
];

// ---------------------------------------------------------------- helpers

/// RAII temp dir (unique per pid + counter, no clock).
struct TempDir(PathBuf);
static COUNTER: AtomicU32 = AtomicU32::new(0);
impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "cb1060_{tag}_{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
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

/// Reader that hands out at most `max` bytes per `read` call: simulates a TCP stream.
struct Trickle<R> {
    inner: R,
    max: usize,
}
impl<R: Read> Read for Trickle<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.max);
        self.inner.read(&mut buf[..n])
    }
}

/// Writer that accepts at most `max` bytes per `write`: why `write_all` exists.
struct Short {
    data: Vec<u8>,
    max: usize,
    calls: usize,
}
impl Write for Short {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = buf.len().min(self.max);
        self.data.extend_from_slice(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Counts bytes passing through, for metrics.
struct Counting<T> {
    inner: T,
    bytes: u64,
    calls: u64,
}
impl<T> Counting<T> {
    fn new(inner: T) -> Self {
        Counting {
            inner,
            bytes: 0,
            calls: 0,
        }
    }
}
impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes += n as u64;
        self.calls += 1;
        Ok(n)
    }
}
impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += n as u64;
        self.calls += 1;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Duplicates all output into two writers (like `tee`).
struct Tee<A, B> {
    a: A,
    b: B,
}
impl<A: Write, B: Write> Write for Tee<A, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.a.write(buf)?;
        self.b.write_all(&buf[..n])?; // b must take exactly what a took
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.a.flush()?;
        self.b.flush()
    }
}

/// Flaky reader: every other call fails with `Interrupted` (like a signal hitting a blocking read).
struct Flaky<R> {
    inner: R,
    n: u32,
}
impl<R: Read> Read for Flaky<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.n += 1;
        if self.n % 2 == 1 {
            return Err(io::Error::from(ErrorKind::Interrupted));
        }
        self.inner.read(buf)
    }
}

fn section(n: u32, title: &str) {
    println!("\n== {n}. {title}");
}

// ---------------------------------------------------------------- 1 Everyday core

fn everyday_core() {
    section(
        1,
        "Everyday core: read / read_exact / read_to_end / read_to_string / write_all / flush",
    );
    // `&[u8]` implements Read and advances itself: the simplest test input.
    let mut src: &[u8] = b"HDR1payload-bytes";
    let mut hdr = [0u8; 4];
    src.read_exact(&mut hdr).unwrap(); // use read_exact for fixed headers
    assert_eq!(&hdr, b"HDR1");
    assert_eq!(src, b"payload-bytes"); // the slice itself moved forward

    // PITFALL: a single `read` may return fewer bytes than the buffer: never assume it filled.
    let mut t = Trickle {
        inner: &b"abcdefgh"[..],
        max: 3,
    };
    let mut buf = [0u8; 8];
    let n = t.read(&mut buf).unwrap();
    assert_eq!(
        n, 3,
        "read returned a short count; read_exact loops for you"
    );
    let mut t = Trickle {
        inner: &b"abcdefgh"[..],
        max: 3,
    };
    t.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"abcdefgh");

    // read_exact EOF mid-way is UnexpectedEof; buffer contents are unspecified afterwards.
    let mut short: &[u8] = b"xy";
    let e = short.read_exact(&mut [0u8; 4]).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnexpectedEof);

    // read_to_end appends to an existing Vec (does not clear it) and returns the count added.
    let mut v = b"keep:".to_vec();
    let n = (&b"tail"[..]).read_to_end(&mut v).unwrap();
    assert_eq!((n, v.as_slice()), (4, &b"keep:tail"[..]));

    // read_to_string validates UTF-8: invalid bytes => InvalidData.
    let mut s = String::new();
    let e = (&[0xff, 0xfe][..]).read_to_string(&mut s).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidData);
    // io::read_to_string(reader) is the one-liner for owned results.
    assert_eq!(io::read_to_string(&b"hello"[..]).unwrap(), "hello");

    // Write: `Vec<u8>` is a Write. write_all + write! + flush is the standard output recipe.
    let mut out = Vec::new();
    out.write_all(b"id=").unwrap();
    write!(out, "{:03}", 7).unwrap(); // write_fmt under the hood
    writeln!(out, ";ok").unwrap();
    out.flush().unwrap();
    assert_eq!(out, b"id=007;ok\n");

    // WHY write_all: `write` may take only a prefix. Ignoring the count silently drops data.
    let mut s = Short {
        data: vec![],
        max: 3,
        calls: 0,
    };
    let n = s.write(b"0123456789").unwrap();
    assert_eq!(n, 3);
    let mut s = Short {
        data: vec![],
        max: 3,
        calls: 0,
    };
    s.write_all(b"0123456789").unwrap();
    assert_eq!(s.data, b"0123456789");
    assert_eq!(s.calls, 4); // 3+3+3+1
    // A writer returning Ok(0) makes write_all fail with WriteZero instead of looping forever.
    let mut full: &mut [u8] = &mut [0u8; 2];
    let e = full.write_all(b"abc").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::WriteZero);
    println!("everyday core ok");
}

// ---------------------------------------------------------------- 2 BufRead

/// Parse `key=value` lines with a reusable String: no allocation per line.
fn parse_kv<R: BufRead>(mut r: R) -> io::Result<Vec<(String, String)>> {
    let mut out = vec![];
    let mut line = String::new();
    loop {
        line.clear(); // read_line APPENDS, forgetting clear() is the classic bug
        if r.read_line(&mut line)? == 0 {
            break;
        } // 0 => EOF
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            out.push((k.trim().into(), v.trim().into()));
        }
    }
    Ok(out)
}

fn bufread_section() {
    section(
        2,
        "BufRead: read_line, lines, read_until, split, skip_until, fill_buf/consume",
    );
    let cfg = "# comment\nname = demo\r\nport=80\n\nlast=nonl";
    let kv = parse_kv(Cursor::new(cfg)).unwrap();
    assert_eq!(
        kv,
        vec![
            ("name".into(), "demo".into()),
            ("port".into(), "80".into()),
            ("last".into(), "nonl".into())
        ]
    );

    // lines() strips \n and \r\n but allocates a String per line; read_line + reuse does not.
    let lines: Vec<String> = Cursor::new("a\r\nb\n\nc")
        .lines()
        .map(Result::unwrap)
        .collect();
    assert_eq!(lines, ["a", "b", "", "c"]);

    // read_until keeps the delimiter; split drops it.
    let mut c = Cursor::new(b"k1:v1;k2:v2;tail".to_vec());
    let mut rec = vec![];
    assert_eq!(c.read_until(b';', &mut rec).unwrap(), 6);
    assert_eq!(rec, b"k1:v1;");
    let rest: Vec<Vec<u8>> = c.split(b';').map(Result::unwrap).collect();
    assert_eq!(rest, vec![b"k2:v2".to_vec(), b"tail".to_vec()]);

    // skip_until (since 1.83): drop a field you do not care about without buffering it.
    let mut c = Cursor::new(b"ignored-prefix|payload".to_vec());
    let skipped = c.skip_until(b'|').unwrap();
    assert_eq!(skipped, 15);
    let mut p = String::new();
    c.read_to_string(&mut p).unwrap();
    assert_eq!(p, "payload");

    // fill_buf + consume: peek at the buffer, decide, then consume exactly what you used.
    let mut r = BufReader::with_capacity(8, &b"GET /index\nrest"[..]);
    let peek = r.fill_buf().unwrap();
    let is_get = peek.starts_with(b"GET ");
    assert!(is_get);
    r.consume(4); // consumed 4 of the buffered bytes only
    let mut path = String::new();
    r.read_line(&mut path).unwrap();
    assert_eq!(path, "/index\n");
    // fill_buf returning an empty slice means EOF.
    let mut e = &b""[..];
    assert!(e.fill_buf().unwrap().is_empty());
    println!("bufread ok");
}

// ---------------------------------------------------------------- 3 adapters: take/chain/by_ref/bytes

fn adapters_section() {
    section(3, "Adapters: take, chain, by_ref, bytes");
    // by_ref: read a header from `r` then give the SAME reader to another consumer.
    let mut r = &b"v1\nbody-data"[..];
    let mut ver = String::new();
    {
        let mut line = (&mut r).take(3); // `&mut R` is Read: take() consumes the &mut, not r
        line.read_to_string(&mut ver).unwrap();
        assert_eq!(line.limit(), 0);
    }
    assert_eq!(ver, "v1\n");
    assert_eq!(r, b"body-data");

    // by_ref() is the method form of `&mut r`.
    let mut r = Cursor::new(b"AAAABBBBCCCC".to_vec());
    let mut first = vec![];
    Read::by_ref(&mut r)
        .take(4)
        .read_to_end(&mut first)
        .unwrap(); // Cursor<Vec> is Read AND Write: both have by_ref
    assert_eq!(first, b"AAAA");
    assert_eq!(r.position(), 4);

    // chain: prepend an already-peeked prefix to the remaining stream.
    let peeked = Cursor::new(b"GE".to_vec());
    let rest = &b"T /\n"[..];
    let mut joined = String::new();
    peeked.chain(rest).read_to_string(&mut joined).unwrap();
    assert_eq!(joined, "GET /\n");

    // take(): limit() and set_limit() (since 1.27) to re-arm per message.
    let mut t = (&b"0123456789"[..]).take(4);
    let mut b = vec![];
    t.read_to_end(&mut b).unwrap();
    assert_eq!(b, b"0123");
    t.set_limit(2);
    b.clear();
    t.read_to_end(&mut b).unwrap();
    assert_eq!(b, b"45");

    // bytes(): one Result<u8> per byte. Convenient, slow on unbuffered files.
    let sum: u32 = (&b"\x01\x02\x03"[..])
        .bytes()
        .map(|b| b.unwrap() as u32)
        .sum();
    assert_eq!(sum, 6);
    println!("adapters ok");
}

// ---------------------------------------------------------------- 4 Seek + Cursor

fn seek_cursor_section() {
    section(
        4,
        "Seek and Cursor over Vec<u8>, &[u8], &mut [u8]; real File seek",
    );
    let mut c = Cursor::new(Vec::new());
    c.write_all(b"hello world").unwrap();
    assert_eq!(c.position(), 11);
    c.rewind().unwrap();
    let mut w = [0u8; 5];
    c.read_exact(&mut w).unwrap();
    assert_eq!(&w, b"hello");
    c.seek(SeekFrom::Start(6)).unwrap();
    c.write_all(b"WORLD").unwrap(); // overwrite in place
    assert_eq!(c.get_ref().as_slice(), b"hello WORLD");
    assert_eq!(c.seek(SeekFrom::End(-5)).unwrap(), 6);
    c.seek_relative(-1).unwrap(); // since 1.80
    assert_eq!(c.stream_position().unwrap(), 5);

    // Seeking before 0 is an error (InvalidInput); seeking past the end is allowed.
    let e = c.seek(SeekFrom::Current(-100)).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidInput);
    c.set_position(14);
    c.write_all(b"!").unwrap();
    let v = c.into_inner();
    assert_eq!(&v[11..14], &[0, 0, 0], "gap is zero-filled");
    assert_eq!(v.len(), 15);

    // Cursor<&mut [u8]> is fixed-size: it cannot grow, so a write beyond it is WriteZero.
    let mut arr = [0u8; 4];
    let mut c = Cursor::new(&mut arr[..]);
    assert!(c.write_all(b"abcde").is_err());
    assert_eq!(c.position(), 4);
    assert_eq!(&arr, b"abcd");

    // Real file: tail the last 3 bytes with SeekFrom::End, then append.
    let d = TempDir::new("seek");
    let p = d.path("f.bin");
    fs::write(&p, b"0123456789").unwrap();
    let mut f = OpenOptions::new().read(true).write(true).open(&p).unwrap();
    f.seek(SeekFrom::End(-3)).unwrap();
    let mut tail = String::new();
    f.read_to_string(&mut tail).unwrap();
    assert_eq!(tail, "789");
    f.rewind().unwrap();
    f.write_all(b"AB").unwrap(); // overwrite the first two bytes
    drop(f);
    assert_eq!(fs::read(&p).unwrap(), b"AB23456789");
    println!("seek ok");
}

// ---------------------------------------------------------------- 5 copy/sink/empty/repeat/vectored

fn utilities_section() {
    section(5, "io::copy, sink, empty, repeat, write_vectored");
    let mut src = Cursor::new(vec![7u8; 100_000]);
    let n = io::copy(&mut src, &mut io::sink()).unwrap(); // measure size without storing
    assert_eq!(n, 100_000);
    let mut dst = Vec::new();
    assert_eq!(io::copy(&mut io::empty(), &mut dst).unwrap(), 0);
    // repeat + take = a bounded run of one byte (padding, test data).
    let mut padding = vec![];
    io::repeat(b' ').take(5).read_to_end(&mut padding).unwrap();
    assert_eq!(padding, b"     ");
    // Counting writer behind copy: bytes counted without touching the content.
    let mut cw = Counting::new(io::sink());
    io::copy(&mut &b"abcdef"[..], &mut cw).unwrap();
    assert_eq!(cw.bytes, 6);

    // write_vectored: header + body in one call (gather write) without concatenating.
    let hdr = b"LEN:5\n";
    let body = b"hello";
    let mut out: Vec<u8> = vec![];
    let n = out
        .write_vectored(&[IoSlice::new(hdr), IoSlice::new(body)])
        .unwrap();
    assert_eq!(n, 11); // Vec accepts everything; other writers may take less: check n
    assert_eq!(out, b"LEN:5\nhello");
    println!("utilities ok");
}

// ---------------------------------------------------------------- 6 io::Error

#[derive(Debug)]
struct QuotaExceeded(u32);
impl fmt::Display for QuotaExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "quota {} exceeded", self.0)
    }
}
impl StdError for QuotaExceeded {}

/// Retry a closure on `Interrupted`, the one error that is always safe to retry blindly.
fn retry_interrupted<T>(mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match f() {
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            other => return other,
        }
    }
}

fn error_section() {
    section(
        6,
        "io::Error: kind, raw_os_error, other/new, get_ref downcast, Interrupted",
    );
    let e = io::Error::from_raw_os_error(2); // ENOENT on unix, ERROR_FILE_NOT_FOUND on Windows
    assert_eq!(e.kind(), ErrorKind::NotFound);
    assert_eq!(e.raw_os_error(), Some(2));
    let e2 = io::Error::from(ErrorKind::PermissionDenied);
    assert_eq!(e2.raw_os_error(), None); // simple kinds carry no OS code
    assert_eq!(e2.to_string(), "permission denied");

    // Wrap domain errors, recover them later.
    let e = io::Error::other(QuotaExceeded(10)); // since 1.74
    assert_eq!(e.kind(), ErrorKind::Other);
    let inner = e
        .get_ref()
        .and_then(|i| i.downcast_ref::<QuotaExceeded>())
        .unwrap();
    assert_eq!(inner.0, 10);
    let e = io::Error::new(ErrorKind::InvalidData, "bad magic");
    let boxed = e.into_inner().unwrap();
    assert_eq!(boxed.to_string(), "bad magic");
    assert!(boxed.source().is_none());

    // Match on kind, never on message text.
    let r = File::open("/definitely/not/here/xyz");
    let msg = match r {
        Err(e) if e.kind() == ErrorKind::NotFound => "missing",
        Err(_) => "other",
        Ok(_) => "ok",
    };
    assert_eq!(msg, "missing");

    // Interrupted: read_to_end/read_exact/write_all retry for you; a bare read does not.
    let mut f = Flaky {
        inner: &b"data"[..],
        n: 0,
    };
    assert_eq!(
        f.read(&mut [0u8; 4]).unwrap_err().kind(),
        ErrorKind::Interrupted
    );
    let mut f = Flaky {
        inner: &b"data"[..],
        n: 0,
    };
    let mut v = vec![];
    f.read_to_end(&mut v).unwrap();
    assert_eq!(v, b"data");
    let mut f = Flaky {
        inner: &b"data"[..],
        n: 0,
    };
    let mut b = [0u8; 4];
    let n = retry_interrupted(|| f.read(&mut b)).unwrap();
    assert_eq!(n, 4);
    println!("errors ok");
}

// ---------------------------------------------------------------- 7 scenarios

/// Line protocol parser that is fed arbitrary chunks: it must not assume line == read.
#[derive(Default)]
struct LineSplitter {
    buf: Vec<u8>,
}
impl LineSplitter {
    fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut out = vec![];
        while let Some(i) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=i).collect();
            out.push(
                String::from_utf8_lossy(&line[..line.len() - 1])
                    .trim_end_matches('\r')
                    .to_string(),
            );
        }
        out
    }
}

fn parse_stream<R: Read>(mut r: R) -> io::Result<Vec<String>> {
    let mut sp = LineSplitter::default();
    let mut out = vec![];
    let mut chunk = [0u8; 16];
    loop {
        let n = match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        out.extend(sp.feed(&chunk[..n]));
    }
    Ok(out)
}

/// TLV record: type u8, length u16 BE, value. `Ok(None)` is clean EOF between records.
fn read_tlv<R: Read>(r: &mut R, max_len: u16) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut t = [0u8; 1];
    loop {
        match r.read(&mut t) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let mut l = [0u8; 2];
    r.read_exact(&mut l)?;
    let len = u16::from_be_bytes(l);
    if len > max_len {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("tlv too long: {len}"),
        ));
    }
    let mut v = Vec::new(); // NOT with_capacity(len): the length came from the peer
    r.by_ref().take(len as u64).read_to_end(&mut v)?;
    if v.len() != len as usize {
        return Err(ErrorKind::UnexpectedEof.into());
    }
    Ok(Some((t[0], v)))
}

fn put_tlv(out: &mut impl Write, t: u8, v: &[u8]) -> io::Result<()> {
    out.write_all(&[t])?;
    out.write_all(&(v.len() as u16).to_be_bytes())?;
    out.write_all(v)
}

/// DoS guard: a line longer than `max` is an error, not an OOM.
fn read_line_limited<R: BufRead>(r: &mut R, max: u64) -> io::Result<Option<Vec<u8>>> {
    let mut buf = vec![];
    let n = r.by_ref().take(max + 1).read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() != Some(&b'\n') && n as u64 > max {
        return Err(io::Error::new(ErrorKind::InvalidData, "line too long"));
    }
    Ok(Some(buf))
}

fn scenarios_section() {
    section(
        7,
        "Scenarios: chunked line protocol, TLV, tee, counting, DoS limit",
    );
    let payload = b"HELLO\r\nPING 1\nPING 2\r\n\nBYE";
    let whole = parse_stream(&payload[..]).unwrap();
    for max in [1usize, 2, 3, 5, 7, 16] {
        let chunked = parse_stream(Trickle {
            inner: &payload[..],
            max,
        })
        .unwrap();
        assert_eq!(
            chunked, whole,
            "chunk size {max} must not change the result"
        );
    }
    assert_eq!(whole, ["HELLO", "PING 1", "PING 2", ""]); // "BYE" has no newline: stays buffered
    // Same input through BufReader::lines over 1-byte reads:
    let l: Vec<_> = BufReader::new(Trickle {
        inner: &payload[..],
        max: 1,
    })
    .lines()
    .map(Result::unwrap)
    .collect();
    assert_eq!(l, ["HELLO", "PING 1", "PING 2", "", "BYE"]);

    // TLV round trip through a 1-byte trickle.
    let mut wire = Vec::new();
    put_tlv(&mut wire, 1, b"abc").unwrap();
    put_tlv(&mut wire, 2, b"").unwrap();
    put_tlv(&mut wire, 3, &[9; 300]).unwrap();
    let mut r = Trickle {
        inner: Cursor::new(wire.clone()),
        max: 1,
    };
    let mut got = vec![];
    while let Some(rec) = read_tlv(&mut r, 1000).unwrap() {
        got.push(rec);
    }
    assert_eq!(got.len(), 3);
    assert_eq!(got[2].1.len(), 300);
    // truncated record => UnexpectedEof; oversize => InvalidData; neither allocates the claimed size
    let cut = &wire[..wire.len() - 1];
    let mut c = Cursor::new(cut);
    read_tlv(&mut c, 1000).unwrap();
    read_tlv(&mut c, 1000).unwrap();
    assert_eq!(
        read_tlv(&mut c, 1000).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
    let evil = [1u8, 0xff, 0xff, 1, 2];
    assert_eq!(
        read_tlv(&mut &evil[..], 1000).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    let evil_max = read_tlv(&mut &evil[..], u16::MAX).unwrap_err(); // claims 65535 but has 2 bytes
    assert_eq!(evil_max.kind(), ErrorKind::UnexpectedEof);

    // Tee + Counting: write a report to a buffer and a checksum-ish counter at once.
    let mut tee = Tee {
        a: Vec::new(),
        b: Counting::new(Vec::new()),
    };
    writeln!(tee, "row {}", 1).unwrap();
    writeln!(tee, "row {}", 2).unwrap();
    assert_eq!(tee.a, tee.b.inner);
    assert_eq!(tee.b.bytes, 12);
    // Tee over a short writer still mirrors exactly what was accepted.
    let mut tee = Tee {
        a: Short {
            data: vec![],
            max: 2,
            calls: 0,
        },
        b: Vec::new(),
    };
    tee.write_all(b"abcdefg").unwrap();
    assert_eq!(tee.a.data, tee.b);

    // Counting reader: bytes in vs. read-call count (short reads mean many calls).
    let mut cr = Counting::new(Trickle {
        inner: &[1u8; 100][..],
        max: 10,
    });
    let mut sink = vec![];
    cr.read_to_end(&mut sink).unwrap();
    assert_eq!(cr.bytes, 100);
    assert!(cr.calls >= 10);

    // DoS limit.
    let mut ok = Cursor::new(b"short\nlines\n".to_vec());
    assert_eq!(read_line_limited(&mut ok, 10).unwrap().unwrap(), b"short\n");
    let mut huge = io::repeat(b'A').take(10_000_000); // 10 MB of newline-free data
    let mut br = BufReader::new(&mut huge);
    let e = read_line_limited(&mut br, 1024).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InvalidData);
    println!("scenarios ok");
}

// ---------------------------------------------------------------- 8 generic testing

fn word_count<R: BufRead, W: Write>(r: R, mut w: W) -> io::Result<usize> {
    let mut total = 0;
    for line in r.lines() {
        let line = line?;
        let n = line.split_whitespace().count();
        total += n;
        writeln!(w, "{n}\t{line}")?;
    }
    w.flush()?;
    Ok(total)
}

fn generic_testing_section() {
    section(8, "Generic R: Read / W: Write testable with Cursor/Vec");
    let mut out = Vec::new();
    let total = word_count(Cursor::new("a b c\n\nd e\n"), &mut out).unwrap();
    assert_eq!(total, 5);
    assert_eq!(String::from_utf8(out).unwrap(), "3\ta b c\n0\t\n2\td e\n");
    // Failure injection: a writer that fails after N bytes.
    struct FailAfter(usize);
    impl Write for FailAfter {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Err(io::Error::other("disk full"));
            }
            let n = b.len().min(self.0);
            self.0 -= n;
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let e = word_count(Cursor::new("one two\nthree\n"), FailAfter(5)).unwrap_err();
    assert_eq!(e.to_string(), "disk full");
    println!("generic ok");
}

// ---------------------------------------------------------------- 9 Production tips

fn production_tips() {
    section(9, "Production tips and tricks");
    // 1. Never trust a length from the wire for with_capacity (clippy has no lint; code review item).
    let mut v = Vec::new();
    (&[1u8, 2][..])
        .take(u32::MAX as u64)
        .read_to_end(&mut v)
        .unwrap(); // reads 2, allocates 2
    assert_eq!(v.len(), 2);
    // 2. Prefer `write_all` over `write` (clippy::unused_io_amount flags ignored counts).
    let mut s = Short {
        data: vec![],
        max: 1,
        calls: 0,
    };
    s.write_all(b"ab").unwrap();
    assert_eq!(s.data, b"ab");
    // 3. Same for reads: use read_exact (clippy::unused_io_amount again) when you need N bytes.
    let mut b = [0u8; 3];
    Trickle {
        inner: &b"xyz"[..],
        max: 1,
    }
    .read_exact(&mut b)
    .unwrap();
    assert_eq!(&b, b"xyz");
    // 4. Take `impl Read` / `impl Write` in APIs, not File: callers and tests get Cursor/Vec for free.
    fn sum_bytes(mut r: impl Read) -> io::Result<u64> {
        let mut t = 0u64;
        for b in r.by_ref().bytes() {
            t += b? as u64;
        }
        Ok(t)
    }
    assert_eq!(sum_bytes(&[1u8, 2, 3][..]).unwrap(), 6);
    // 5. `lines()` allocates per line; reuse a String for hot loops (anti-pattern: lines() on 10M lines).
    let data = "x\n".repeat(1000);
    let mut r = Cursor::new(data);
    let mut line = String::new();
    let mut n = 0;
    while r.read_line(&mut line).unwrap() > 0 {
        n += 1;
        line.clear();
    }
    assert_eq!(n, 1000);
    // 6. `&mut R` implements Read/Write: pass `&mut r` to helpers so you keep the reader.
    let mut r = &b"abcdef"[..];
    let mut first = [0u8; 2];
    (&mut r).read_exact(&mut first).unwrap();
    assert_eq!(r, b"cdef");
    // 7. Distinguish clean EOF from truncation: Ok(0) at a record boundary vs UnexpectedEof mid-record.
    assert!(read_tlv(&mut &b""[..], 10).unwrap().is_none());
    assert!(read_tlv(&mut &[1u8, 0][..], 10).is_err());
    // 8. Match `ErrorKind`, never strings; map WouldBlock/TimedOut/Interrupted separately.
    let retryable = |k: ErrorKind| {
        matches!(
            k,
            ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut
        )
    };
    assert!(retryable(ErrorKind::TimedOut) && !retryable(ErrorKind::NotFound));
    // 9. Return `io::Result` from helpers and add context at the boundary (e.g. map_err with the path).
    let r = File::open("/no/such/file")
        .map_err(|e| io::Error::new(e.kind(), format!("open config: {e}")));
    assert!(r.unwrap_err().to_string().starts_with("open config:"));
    // 10. Use `io::copy` instead of a manual read/write loop: it uses specialisations (copy_file_range/sendfile on Linux).
    let mut dst = Vec::new();
    assert_eq!(io::copy(&mut &b"12345"[..], &mut dst).unwrap(), 5);
    // 11. Cursor::new(&mut [u8]) is a zero-alloc, bounds-checked fixed buffer for encoding headers.
    let mut hdr = [0u8; 8];
    let mut c = Cursor::new(&mut hdr[..]);
    c.write_all(&0xCAFEu16.to_be_bytes()).unwrap();
    c.write_all(&1u32.to_le_bytes()).unwrap();
    assert_eq!(c.position(), 6);
    assert_eq!(hdr[..2], [0xCA, 0xFE]);
    // 12. Flush explicitly on buffered writers before relying on the data (see std_bufreader_bufwriter_patterns).
    let mut c = io::BufWriter::new(Vec::new());
    c.write_all(b"x").unwrap();
    assert_eq!(c.get_ref().len(), 0, "still in the buffer");
    c.flush().unwrap();
    assert_eq!(c.get_ref().len(), 1);
    println!("tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|c| seen.insert(*c)),
        "COVERED has duplicates"
    );
    everyday_core();
    bufread_section();
    adapters_section();
    seek_cursor_section();
    utilities_section();
    error_section();
    scenarios_section();
    generic_testing_section();
    production_tips();
    println!("\nall sections passed ({} methods listed)", COVERED.len());
}
