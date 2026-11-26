//! RECIPE 243 (storage): buffered IO internals: build BufReader, BufWriter, LineWriter and a prefetching reader from scratch.
//! LEVEL: advanced
//! Use in: parsers, log writers, network protocols, any code that does many small reads/writes.   Run: cargo run --release --example buffered_io_internals
#![allow(dead_code)]
//!
//! WHAT: `MyBufReader`, `MyBufWriter`, `MyLineWriter`, a double-buffered (prefetching) reader and a vectored-write
//! coalescing sink, each tested against `std` and against hostile mocks (short reads/writes, `Interrupted`).
//!
//! WHY: every `read`/`write` on a `File`/`TcpStream` is a syscall (syscalls_portable), ~100 ns-1 us even when it moves one byte.
//! A buffer turns N tiny calls into N*size/BUF big ones. Reading 1 MiB byte by byte through an 8 KiB `BufReader`
//! costs exactly 128 underlying reads; unbuffered it would be 1,048,576.
//!
//! HOW (the std algorithm): reader: `fill_buf` refills only when empty; `read` bypasses the buffer when it is empty and the
//! caller's buffer is at least as large as ours (no double copy); `seek` must discard the buffer and, for
//! `SeekFrom::Current`, subtract the unread bytes because the inner cursor is ahead of the logical position.
//! writer: append when it fits, flush first when it would overflow, send writes >= capacity straight through.
//!
//! FOOTGUNS: (1) `BufWriter` flushes on `Drop` but SWALLOWS the error (drop cannot return one): call `flush()` or
//! `into_inner()` and check the result before you consider the data written. (2) Do not wrap in-memory data
//! (`&[u8]`, `Cursor<Vec<u8>>`, `Vec<u8>`): a `BufReader` there only adds a memcpy; a `BufWriter` over a `Vec` the same.
//! Wrap files, sockets and pipes. (3) `write_all` retry loops are mandatory: `write` may accept fewer bytes than offered.
//! (4) A `LineWriter` (stdout when it is a terminal) syscalls once per line; piping stdout to a file makes it
//! block-buffered in other languages, but Rust's stdout is always line-buffered, so wrap it in a `BufWriter` for bulk output.
//!
//! COMMON USES: log/CSV/JSON-lines writers, line-oriented protocol parsers (Redis/SMTP-style), file copy tools,
//! decompression pipelines that overlap IO with CPU (prefetch), network servers coalescing responses with writev.
//! Read: https://doc.rust-lang.org/std/io/struct.BufReader.html https://doc.rust-lang.org/std/io/struct.BufWriter.html https://man7.org/linux/man-pages/man2/readv.2.html
use bytes::Bytes;
use std::io::{self, BufRead, BufReader, BufWriter, Cursor, IoSlice, Read, Seek, SeekFrom, Write};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ---------------------------------------------------------------- counting and hostile mocks
#[derive(Debug)]
struct Counting<T> {
    inner: T,
    calls: usize,
    bytes: usize,
}
fn counting<T>(inner: T) -> Counting<T> {
    Counting {
        inner,
        calls: 0,
        bytes: 0,
    }
}
impl<R: Read> Read for Counting<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = self.inner.read(b)?;
        self.bytes += n;
        Ok(n)
    }
}
impl<W: Write> Write for Counting<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = self.inner.write(b)?;
        self.bytes += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<S: Seek> Seek for Counting<S> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.inner.seek(p)
    }
}

/// Reader that returns 1-3 bytes at a time and sometimes `Interrupted`.
struct FlakyReader<'a> {
    data: &'a [u8],
    rng: Rng,
    interrupts: usize,
}
impl Read for FlakyReader<'_> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if self.rng.below(4) == 0 {
            self.interrupts += 1;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = (1 + self.rng.below(3) as usize)
            .min(b.len())
            .min(self.data.len());
        b[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}
/// Writer that accepts 1-3 bytes per call and sometimes fails with `Interrupted`.
struct FlakyWriter {
    sink: Vec<u8>,
    rng: Rng,
    calls: usize,
}
impl Write for FlakyWriter {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.rng.below(4) == 0 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = (1 + self.rng.below(3) as usize).min(b.len());
        self.sink.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct ZeroWriter;
impl Write for ZeroWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Ok(0)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct BrokenWriter(std::rc::Rc<std::cell::Cell<usize>>);
impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        self.0.set(self.0.get() + 1);
        Err(io::Error::other("disk full"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------- MyBufReader
struct MyBufReader<R> {
    inner: R,
    buf: Box<[u8]>,
    pos: usize,
    filled: usize,
}
impl<R: Read> MyBufReader<R> {
    fn new(inner: R) -> Self {
        Self::with_capacity(8192, inner)
    }
    fn with_capacity(cap: usize, inner: R) -> Self {
        assert!(cap > 0);
        MyBufReader {
            inner,
            buf: vec![0; cap].into_boxed_slice(),
            pos: 0,
            filled: 0,
        }
    }
    fn get_ref(&self) -> &R {
        &self.inner
    }
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.pos >= self.filled {
            self.filled = self.inner.read(&mut self.buf)?; // on Err the buffer stays empty and nothing is lost
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..self.filled])
    }
    fn consume(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.filled);
    }
    /// Look at up to `n` bytes (n <= capacity) without consuming; may read more from the source to satisfy it.
    fn peek(&mut self, n: usize) -> io::Result<&[u8]> {
        if n > self.buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "peek larger than buffer",
            ));
        }
        if self.filled - self.pos < n {
            self.buf.copy_within(self.pos..self.filled, 0);
            self.filled -= self.pos;
            self.pos = 0;
            while self.filled < n {
                match self.inner.read(&mut self.buf[self.filled..]) {
                    Ok(0) => break,
                    Ok(k) => self.filled += k,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(&self.buf[self.pos..(self.pos + n).min(self.filled)])
    }
    fn read_until(&mut self, delim: u8, out: &mut Vec<u8>) -> io::Result<usize> {
        let mut total = 0;
        loop {
            let (done, used) = {
                let avail = match self.fill_buf() {
                    Ok(b) => b,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                };
                match avail.iter().position(|&b| b == delim) {
                    Some(i) => {
                        out.extend_from_slice(&avail[..=i]);
                        (true, i + 1)
                    }
                    None => {
                        out.extend_from_slice(avail);
                        (false, avail.len())
                    }
                }
            };
            self.consume(used);
            total += used;
            if done || used == 0 {
                return Ok(total);
            } // used == 0 is EOF
        }
    }
    /// Like std: on invalid UTF-8 return InvalidData and append nothing (the bytes are still consumed).
    fn read_line(&mut self, out: &mut String) -> io::Result<usize> {
        let mut raw = Vec::new();
        let n = self.read_until(b'\n', &mut raw)?;
        out.push_str(std::str::from_utf8(&raw).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })?);
        Ok(n)
    }
    fn lines(self) -> MyLines<R> {
        MyLines(self)
    }
}
impl<R: Read> Read for MyBufReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.filled && out.len() >= self.buf.len() {
            return self.inner.read(out); // bypass: avoid copying through our buffer
        }
        let n = {
            let mut rem = self.fill_buf()?;
            rem.read(out)?
        };
        self.consume(n);
        Ok(n)
    }
}
impl<R: Read + Seek> Seek for MyBufReader<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let r = match to {
            // the inner cursor is `filled - pos` bytes AHEAD of the logical position
            SeekFrom::Current(n) => self
                .inner
                .seek(SeekFrom::Current(n - (self.filled - self.pos) as i64))?,
            other => self.inner.seek(other)?,
        };
        self.pos = 0;
        self.filled = 0; // buffered bytes belong to the old position
        Ok(r)
    }
}
struct MyLines<R>(MyBufReader<R>);
impl<R: Read> Iterator for MyLines<R> {
    type Item = io::Result<String>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut s = String::new();
        match self.0.read_line(&mut s) {
            Ok(0) => None,
            Ok(_) => {
                if s.ends_with('\n') {
                    s.pop();
                    if s.ends_with('\r') {
                        s.pop();
                    }
                }
                Some(Ok(s))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

// ---------------------------------------------------------------- MyBufWriter / MyLineWriter
struct MyBufWriter<W: Write> {
    inner: Option<W>,
    buf: Vec<u8>,
    cap: usize,
}
impl<W: Write> MyBufWriter<W> {
    fn new(inner: W) -> Self {
        Self::with_capacity(8192, inner)
    }
    fn with_capacity(cap: usize, inner: W) -> Self {
        MyBufWriter {
            inner: Some(inner),
            buf: Vec::with_capacity(cap),
            cap,
        }
    }
    fn buffered(&self) -> usize {
        self.buf.len()
    }
    fn get_ref(&self) -> &W {
        self.inner.as_ref().unwrap()
    }
    fn flush_buf(&mut self) -> io::Result<()> {
        let mut written = 0;
        let mut res = Ok(());
        while written < self.buf.len() {
            match self.inner.as_mut().unwrap().write(&self.buf[written..]) {
                Ok(0) => {
                    res = Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write the buffered data",
                    ));
                    break;
                }
                Ok(n) => written += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }
        self.buf.drain(..written); // keep what was NOT written so a retry does not lose or duplicate data
        res
    }
    /// Flush and give the writer back. On failure the BufWriter (with unwritten data) is returned with the error.
    fn into_inner(mut self) -> Result<W, (MyBufWriter<W>, io::Error)> {
        match self.flush_buf() {
            Ok(()) => Ok(self.inner.take().unwrap()),
            Err(e) => Err((self, e)),
        }
    }
}
impl<W: Write> Write for MyBufWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buf.len() + data.len() > self.cap {
            self.flush_buf()?;
        }
        if data.len() >= self.cap {
            self.inner.as_mut().unwrap().write(data)
        }
        // large write bypasses the buffer
        else {
            self.buf.extend_from_slice(data);
            Ok(data.len())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flush_buf()?;
        self.inner.as_mut().unwrap().flush()
    }
}
impl<W: Write> Drop for MyBufWriter<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            let _ = self.flush_buf();
        } // the footgun: errors are discarded here
    }
}

/// Flushes whenever a newline is written (everything up to and including the last '\n').
struct MyLineWriter<W: Write>(MyBufWriter<W>);
impl<W: Write> Write for MyLineWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match data.iter().rposition(|&b| b == b'\n') {
            None => self.0.write(data),
            Some(i) => {
                self.0.write_all(&data[..=i])?;
                self.0.flush()?;
                self.0.write_all(&data[i + 1..])?;
                Ok(data.len())
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

// ---------------------------------------------------------------- double-buffered (prefetching) reader
/// A background thread reads blocks ahead (bounded channel = at most `depth` blocks in flight) so IO overlaps with
/// the consumer's processing. Dropping the reader closes the channel, which stops the thread.
struct PrefetchReader {
    rx: Option<mpsc::Receiver<io::Result<Vec<u8>>>>,
    cur: Vec<u8>,
    pos: usize,
    handle: Option<JoinHandle<()>>,
}
impl PrefetchReader {
    fn new<R: Read + Send + 'static>(mut src: R, block: usize, depth: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel(depth);
        let handle = std::thread::spawn(move || {
            loop {
                let mut b = vec![0u8; block];
                match src.read(&mut b) {
                    Ok(0) => return,
                    Ok(n) => {
                        b.truncate(n);
                        if tx.send(Ok(b)).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }
        });
        PrefetchReader {
            rx: Some(rx),
            cur: Vec::new(),
            pos: 0,
            handle: Some(handle),
        }
    }
}
impl Read for PrefetchReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.cur.len() {
            match self.rx.as_ref().unwrap().recv() {
                Ok(b) => {
                    self.cur = b?;
                    self.pos = 0;
                }
                Err(_) => return Ok(0), // sender finished: EOF
            }
        }
        let n = out.len().min(self.cur.len() - self.pos);
        out[..n].copy_from_slice(&self.cur[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
impl Drop for PrefetchReader {
    fn drop(&mut self) {
        drop(self.rx.take());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
struct SlowReader {
    data: Cursor<Vec<u8>>,
    delay: Duration,
}
impl Read for SlowReader {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(self.delay);
        self.data.read(b)
    }
}

// ---------------------------------------------------------------- write-coalescing sink with vectored writes
/// Queues cheap `Bytes` handles (no copying) and sends them with ONE `writev` per flush.
struct CoalescingSink<W: Write> {
    out: W,
    queue: Vec<Bytes>,
    queued: usize,
    limit: usize,
}
impl<W: Write> CoalescingSink<W> {
    fn new(out: W, limit: usize) -> Self {
        CoalescingSink {
            out,
            queue: vec![],
            queued: 0,
            limit,
        }
    }
    fn push(&mut self, b: Bytes) -> io::Result<()> {
        self.queued += b.len();
        self.queue.push(b);
        if self.queued >= self.limit || self.queue.len() >= 512 {
            self.flush()?;
        } // stay below IOV_MAX (1024 on Linux/macOS)
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        let (mut idx, mut off) = (0, 0); // first unwritten chunk and offset inside it
        while idx < self.queue.len() {
            let slices: Vec<IoSlice> = self.queue[idx..]
                .iter()
                .enumerate()
                .map(|(i, b)| IoSlice::new(if i == 0 { &b[off..] } else { b }))
                .collect();
            match self.out.write_vectored(&slices) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(mut n) => {
                    while n > 0 {
                        // advance across chunk boundaries after a partial write
                        let left = self.queue[idx].len() - off;
                        if n >= left {
                            n -= left;
                            idx += 1;
                            off = 0;
                        } else {
                            off += n;
                            n = 0;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        self.queue.clear();
        self.queued = 0;
        self.out.flush()
    }
}
/// Counts plain vs vectored calls and may accept only part of an iovec list.
struct VecMock {
    sink: Vec<u8>,
    plain: usize,
    vectored: usize,
    max_per_call: usize,
}
impl Write for VecMock {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.plain += 1;
        let n = b.len().min(self.max_per_call);
        self.sink.extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.vectored += 1;
        let mut left = self.max_per_call;
        let mut total = 0;
        for b in bufs {
            let n = b.len().min(left);
            self.sink.extend_from_slice(&b[..n]);
            total += n;
            left -= n;
            if left == 0 {
                break;
            }
        }
        Ok(total)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn random_text(rng: &mut Rng, len: usize) -> Vec<u8> {
    (0..len)
        .map(|_| {
            if rng.below(12) == 0 {
                b'\n'
            } else {
                b'a' + rng.below(26) as u8
            }
        })
        .collect()
}

fn main() {
    let mut rng = Rng(0x1357_9bdf_2468_ace1);

    // 1. exact syscall counts
    let mb = vec![42u8; 1 << 20];
    let mut r = MyBufReader::with_capacity(8192, counting(&mb[..]));
    let mut one = [0u8; 1];
    for _ in 0..mb.len() {
        r.read_exact(&mut one).unwrap();
    }
    assert_eq!(
        r.get_ref().calls,
        128,
        "1 MiB / 8 KiB = 128 underlying reads"
    );
    assert_eq!(r.read(&mut one).unwrap(), 0);
    assert_eq!(r.get_ref().calls, 129, "plus one read that observes EOF");
    let mut sr = BufReader::with_capacity(8192, counting(&mb[..]));
    for _ in 0..mb.len() {
        sr.read_exact(&mut one).unwrap();
    }
    assert_eq!(sr.get_ref().calls, 128);
    let mut r = MyBufReader::with_capacity(8192, counting(&mb[..]));
    let mut big = vec![0u8; 16384];
    for _ in 0..64 {
        r.read_exact(&mut big).unwrap();
    }
    assert_eq!(
        r.get_ref().calls,
        64,
        "reads >= capacity bypass the buffer: one inner read each"
    );
    println!(
        "1 MiB byte-by-byte via 8 KiB buffer: 128 inner reads (vs 1048576 unbuffered); 16 KiB reads bypass: 64"
    );
    let mut w = MyBufWriter::with_capacity(8192, counting(Vec::new()));
    let mut sw = BufWriter::with_capacity(8192, counting(Vec::new()));
    for i in 0..10_000usize {
        let c = vec![i as u8; 1 + i % 300];
        w.write_all(&c).unwrap();
        sw.write_all(&c).unwrap();
    }
    let (a, b) = (
        w.into_inner().map_err(|e| e.1).unwrap(),
        sw.into_inner().unwrap(),
    );
    assert_eq!((a.calls, a.bytes), (b.calls, b.bytes));
    assert_eq!(a.inner, b.inner);
    println!(
        "MyBufWriter issued {} writes for {} bytes in 10000 small writes: identical to std",
        a.calls, a.bytes
    );

    // 2. hostile mocks: write_all / read_exact retry loops
    let data = random_text(&mut rng, 5000);
    let mut fr = FlakyReader {
        data: &data,
        rng: Rng(7),
        interrupts: 0,
    };
    let mut got = vec![0u8; data.len()];
    fr.read_exact(&mut got).unwrap(); // read_exact retries Interrupted and short reads
    assert_eq!(got, data);
    let interrupts = fr.interrupts;
    let mut mr = MyBufReader::with_capacity(
        16,
        FlakyReader {
            data: &data,
            rng: Rng(8),
            interrupts: 0,
        },
    );
    let mut lines = Vec::new();
    loop {
        let mut l = Vec::new();
        if mr.read_until(b'\n', &mut l).unwrap() == 0 {
            break;
        }
        lines.push(l);
    }
    assert_eq!(lines.concat(), data);
    assert!(
        lines
            .iter()
            .all(|l| l.iter().filter(|&&b| b == b'\n').count() <= 1)
    );
    let mut fw = FlakyWriter {
        sink: vec![],
        rng: Rng(9),
        calls: 0,
    };
    fw.write_all(&data).unwrap();
    assert_eq!(fw.sink, data);
    let mut bw = MyBufWriter::with_capacity(
        64,
        FlakyWriter {
            sink: vec![],
            rng: Rng(10),
            calls: 0,
        },
    );
    for c in data.chunks(37) {
        bw.write_all(c).unwrap();
    }
    bw.flush().unwrap();
    assert_eq!(bw.get_ref().sink, data);
    println!(
        "flaky 1-3 byte reads/writes with Interrupted ({interrupts} interrupts): data intact, {} underlying write calls",
        bw.get_ref().calls
    );
    let mut zb = MyBufWriter::with_capacity(8, ZeroWriter);
    zb.write_all(b"abc").unwrap();
    assert_eq!(zb.flush().unwrap_err().kind(), io::ErrorKind::WriteZero); // no infinite loop on Ok(0)
    assert_eq!(zb.buffered(), 3);
    std::mem::forget(zb);

    // 3. the flush-on-drop footgun
    let attempts = std::rc::Rc::new(std::cell::Cell::new(0));
    {
        let mut w = MyBufWriter::new(BrokenWriter(attempts.clone()));
        w.write_all(b"important data").unwrap(); // Ok: it only went into the buffer
        assert_eq!(attempts.get(), 0);
    } // drop tries to flush, fails, and the error vanishes
    assert_eq!(attempts.get(), 1);
    let mut w = MyBufWriter::new(BrokenWriter(attempts.clone()));
    w.write_all(b"important data").unwrap();
    let (back, err) = w.into_inner().err().unwrap();
    assert_eq!(err.to_string(), "disk full");
    assert_eq!(back.buffered(), 14);
    std::mem::forget(back);
    println!(
        "drop-flush failure was silent; into_inner() reported '{err}' and returned the unwritten data"
    );

    // 4. line writer
    let mut lw = MyLineWriter(MyBufWriter::new(Vec::new()));
    lw.write_all(b"abc\ndef").unwrap();
    assert_eq!(lw.0.get_ref(), b"abc\n");
    lw.write_all(b"g\nh").unwrap();
    assert_eq!(lw.0.get_ref(), b"abc\ndefg\n");
    lw.flush().unwrap();
    assert_eq!(lw.0.get_ref(), b"abc\ndefg\nh");
    let mut slw = io::LineWriter::new(Vec::new());
    slw.write_all(b"abc\ndef").unwrap();
    assert_eq!(slw.get_ref(), b"abc\n");
    for _ in 0..300 {
        let text = random_text(&mut rng, 200);
        let mut lw = MyLineWriter(MyBufWriter::with_capacity(
            1 + rng.below(40) as usize,
            Vec::new(),
        ));
        let mut at = 0;
        while at < text.len() {
            let n = (1 + rng.below(20) as usize).min(text.len() - at);
            lw.write_all(&text[at..at + n]).unwrap();
            at += n;
            let sink = lw.0.get_ref();
            assert!(text.starts_with(sink));
            if let Some(i) = text[..at].iter().rposition(|&b| b == b'\n') {
                assert!(
                    sink.len() > i,
                    "everything through the last newline is flushed"
                );
            }
        }
    }

    // 5. differential tests vs std on 5000 seeded operation sequences
    for case in 0..5000 {
        let len = rng.below(400) as usize;
        let data = random_text(&mut rng, len);
        let cap = 1 + rng.below(48) as usize;
        let mut mine = MyBufReader::with_capacity(cap, Cursor::new(data.clone()));
        let mut std_r = BufReader::with_capacity(cap, Cursor::new(data.clone()));
        for _ in 0..12 {
            match rng.below(6) {
                0 | 1 => {
                    let n = rng.below(60) as usize;
                    let (mut a, mut b) = (vec![0u8; n], vec![0u8; n]);
                    let (ra, rb) = (mine.read(&mut a).unwrap(), std_r.read(&mut b).unwrap());
                    assert_eq!((ra, &a[..ra]), (rb, &b[..rb]), "case {case}: read");
                }
                2 => {
                    let (mut a, mut b) = (Vec::new(), Vec::new());
                    assert_eq!(
                        mine.read_until(b'\n', &mut a).unwrap(),
                        std_r.read_until(b'\n', &mut b).unwrap()
                    );
                    assert_eq!(a, b);
                }
                3 => {
                    let (mut a, mut b) = (String::new(), String::new());
                    assert_eq!(
                        mine.read_line(&mut a).unwrap(),
                        std_r.read_line(&mut b).unwrap()
                    );
                    assert_eq!(a, b);
                }
                4 => {
                    let k = rng.below(10) as usize;
                    let a = mine.fill_buf().unwrap().to_vec();
                    assert_eq!(a, std_r.fill_buf().unwrap(), "case {case}: fill_buf");
                    mine.consume(k);
                    std_r.consume(k);
                }
                _ => {
                    let to = match rng.below(3) {
                        0 => SeekFrom::Start(rng.below(len as u64 + 5)),
                        1 => SeekFrom::Current(rng.below(40) as i64 - 20),
                        _ => SeekFrom::End(-(rng.below(len as u64 + 3) as i64)),
                    };
                    let (a, b) = (
                        mine.seek(to).map_err(|e| e.kind()),
                        std_r.seek(to).map_err(|e| e.kind()),
                    );
                    assert_eq!(a, b, "case {case}: seek {to:?}");
                }
            }
        }
        let (mut ra, mut rb) = (Vec::new(), Vec::new());
        mine.read_to_end(&mut ra).unwrap();
        std_r.read_to_end(&mut rb).unwrap();
        assert_eq!(ra, rb, "case {case}: tail");
        let ls: Vec<String> = MyBufReader::with_capacity(cap, &data[..])
            .lines()
            .map(|l| l.unwrap())
            .collect();
        let want: Vec<String> = BufReader::new(&data[..])
            .lines()
            .map(|l| l.unwrap())
            .collect();
        assert_eq!(ls, want);
        // writers: same content, same buffered amount, same underlying call count
        let wcap = 1 + rng.below(40) as usize;
        let (mut a, mut b) = (
            MyBufWriter::with_capacity(wcap, counting(Vec::new())),
            BufWriter::with_capacity(wcap, counting(Vec::new())),
        );
        for _ in 0..15 {
            if rng.below(5) == 0 {
                a.flush().unwrap();
                b.flush().unwrap();
            } else {
                let n = rng.below(60) as usize;
                let c = random_text(&mut rng, n);
                a.write_all(&c).unwrap();
                b.write_all(&c).unwrap();
            }
            assert_eq!(
                a.buffered(),
                b.buffer().len(),
                "case {case}: buffered bytes"
            );
            assert_eq!(
                a.get_ref().calls,
                b.get_ref().calls,
                "case {case}: underlying calls"
            );
        }
        assert_eq!(
            a.into_inner().map_err(|e| e.1).unwrap().inner,
            b.into_inner().unwrap().inner
        );
    }
    println!(
        "5000 differential cases vs std BufReader/BufWriter: identical results, buffers and call counts"
    );

    // 6. peek and seek invalidation
    let src: Vec<u8> = (0..100u8).collect();
    let mut p = MyBufReader::with_capacity(16, counting(Cursor::new(src.clone())));
    assert_eq!(p.peek(4).unwrap(), &[0, 1, 2, 3]);
    assert_eq!(p.peek(4).unwrap(), &[0, 1, 2, 3], "peek does not consume");
    p.consume(3);
    assert_eq!(p.peek(10).unwrap(), &src[3..13]);
    assert!(p.peek(17).is_err());
    let mut b2 = [0u8; 2];
    p.read_exact(&mut b2).unwrap();
    assert_eq!(b2, [3, 4]);
    assert_eq!(
        p.seek(SeekFrom::Current(0)).unwrap(),
        5,
        "inner cursor was ahead; Current subtracts the unread bytes"
    );
    p.seek(SeekFrom::Start(50)).unwrap();
    p.read_exact(&mut b2).unwrap();
    assert_eq!(
        b2,
        [50, 51],
        "stale buffered bytes must not leak after a seek"
    );

    // 7. prefetching reader: overlap IO latency with processing
    let payload: Vec<u8> = (0..(16 * 65536)).map(|i| (i % 251) as u8).collect();
    let work = |r: &mut dyn Read| {
        let mut sum = 0u64;
        let mut blk = vec![0u8; 65536];
        loop {
            let n = r.read(&mut blk).unwrap();
            if n == 0 {
                return sum;
            }
            sum += blk[..n].iter().map(|&b| b as u64).sum::<u64>();
            std::thread::sleep(Duration::from_millis(3)); // pretend CPU work per block
        }
    };
    let slow = || SlowReader {
        data: Cursor::new(payload.clone()),
        delay: Duration::from_millis(3),
    };
    let t = Instant::now();
    let s1 = work(&mut BufReader::with_capacity(65536, slow()));
    let t_seq = t.elapsed();
    let t = Instant::now();
    let s2 = work(&mut PrefetchReader::new(slow(), 65536, 2));
    let t_pre = t.elapsed();
    assert_eq!(s1, s2);
    let mut early = PrefetchReader::new(slow(), 65536, 2); // dropping early must not hang or leak the thread
    let mut one = [0u8; 1];
    early.read_exact(&mut one).unwrap();
    drop(early);
    println!(
        "16 blocks, 3 ms IO + 3 ms work each: sequential {t_seq:?} vs prefetch {t_pre:?} (printed, not asserted)"
    );

    // 8. vectored coalescing sink
    let chunks: Vec<Bytes> = (0..100u8)
        .map(|i| Bytes::from(vec![i; 1 + i as usize % 7]))
        .collect();
    let want: Vec<u8> = chunks.iter().flat_map(|b| b.to_vec()).collect();
    let mut sink = CoalescingSink::new(
        VecMock {
            sink: vec![],
            plain: 0,
            vectored: 0,
            max_per_call: usize::MAX,
        },
        usize::MAX,
    );
    for c in &chunks {
        sink.push(c.clone()).unwrap();
    }
    sink.flush().unwrap();
    assert_eq!(
        (sink.out.sink.clone(), sink.out.vectored, sink.out.plain),
        (want.clone(), 1, 0)
    );
    let mut sink = CoalescingSink::new(
        VecMock {
            sink: vec![],
            plain: 0,
            vectored: 0,
            max_per_call: 5,
        },
        64,
    );
    for c in &chunks {
        sink.push(c.clone()).unwrap();
    }
    sink.flush().unwrap();
    assert_eq!(
        sink.out.sink, want,
        "partial vectored writes are advanced correctly across chunk boundaries"
    );
    println!(
        "100 chunks -> 1 writev (vs 100 write calls); with a 5-bytes-per-call device: {} writev calls, data intact",
        sink.out.vectored
    );
    println!("OK");
}
