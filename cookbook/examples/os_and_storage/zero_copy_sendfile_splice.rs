//! RECIPE 446 (storage): zero-copy IO: read/write loop vs sendfile, io::copy, clonefile/copy_file_range, mmap, splice, bytes::Bytes.
//! LEVEL: intermediate
//! Use in: static file servers, proxies, backup/copy tools, log shippers, anything moving big byte ranges disk <-> disk/socket.   Run: cargo run --release --example zero_copy_sendfile_splice
#![allow(dead_code)]
//!
//! WHAT: one 64 MiB file is copied to another file and sent over loopback TCP with every method this OS offers; each result is
//! checked by checksum, and the number of user-visible calls (syscalls) and MB/s are printed.
//!
//! WHY: the classic `read(fd, buf)` + `write(sock, buf)` loop moves every byte TWICE across the user/kernel boundary:
//!   disk -> page cache -> (copy 1) user buffer -> (copy 2) socket buffer -> NIC, plus 2 syscalls and 2 context switches per chunk.
//! `sendfile(2)` asks the kernel to do page cache -> socket buffer itself (no user-space copy, 1 syscall per call; with DMA-capable
//! NICs even that copy can be skipped on Linux). `copy_file_range`/`clonefile`/reflinks can avoid moving the data at all:
//! the filesystem just points the new file at the same blocks (copy-on-write).
//!
//! HOW, per method (what ran on this machine is printed at the end):
//!  - read/write loop: baseline. Larger buffers = fewer syscalls (1 MiB buffer: 64 reads + 64 writes instead of 1024 + 1024).
//!  - `std::io::copy`: with the generic fallback it is an 8 KiB read/write loop. On LINUX std specialises it
//!    (library/std/src/sys/io/kernel_copy.rs): File->File uses `copy_file_range`, File->socket/pipe uses `sendfile` or `splice`,
//!    falling back to the loop when the kernel says no. The specialisation only triggers when both ends are plain std
//!    `File`/`TcpStream`/`UnixStream`/pipe types: wrap one in your own struct (as the counting wrapper below does) and you silently
//!    get the slow loop. On macOS (this machine) `io::copy` is the generic loop (measured below: same speed as the wrapper loop);
//!    `std::fs::copy` instead calls `fcopyfile(3)`, a kernel-side copy that on APFS turned out to be a copy-on-write CLONE
//!    (observed below: ~1000x the read/write speed because no data moves); on Linux it uses `copy_file_range`.
//!  - `sendfile`: signature differs. Linux: `sendfile(out_fd, in_fd, *offset, count) -> ssize_t` (any out fd since 2.6.33).
//!    macOS/BSD: `sendfile(fd, socket, offset, *len /*in: max, out: sent*/, hdtr, flags)`, destination MUST be a socket; the
//!    `len` out-parameter reports partial progress when the call is interrupted or times out, so always loop.
//!  - `clonefile(2)` (macOS, APFS) / `FICLONE` ioctl (Linux btrfs/XFS reflink): O(1) copy-on-write clone, no data moved.
//!    Fails with EXDEV/ENOTSUP across volumes or on non-COW filesystems (ext4): always keep a fallback.
//!  - mmap + write: the file's pages are mapped, so there is no `read` copy, but `write` still copies page cache -> socket buffer,
//!    and page faults cost; a good middle ground when you also want to inspect the data (checksums, headers).
//!  - Linux only (described here, compiled under `cfg(target_os = "linux")` but NOT run on this macOS machine): `splice(2)` moves
//!    pages between a file/socket and a PIPE without copying (the pipe is just a ring of page references; file -> pipe -> socket
//!    is the sendfile of old kernels and works for socket -> file too), `tee(2)` duplicates pipe contents without consuming them
//!    (fan-out to two sockets, e.g. a TCP proxy that also logs), `vmsplice(2)` maps user pages into a pipe (the caller must not
//!    touch them until the consumer is done, which is hard to know: rarely worth it). io_uring has splice/send_zc opcodes too.
//!  - `bytes::Bytes` is the USER-SPACE version of the idea: an immutable, reference-counted view of a buffer. `clone()` and
//!    `slice()` only bump a counter and move a pointer, so parsers and servers can hand header/body slices to many tasks with
//!    no memcpy. `Bytes::from(Vec)` takes ownership of the allocation; `Bytes::from_owner(mmap)` makes slices of a mapped file.
//!
//! FOOTGUNS: (1) none of the page-cache copies are `fsync`ed: MB/s here is memory speed, not disk speed (use `sync_all` and a cold
//! cache for durability numbers). (2) sendfile on a file truncated meanwhile sends short/garbage; mmap on a truncated file raises SIGBUS.
//! (3) zero-copy is not free of cost: a splice/sendfile call has setup overhead, so for tiny payloads (< ~4-16 KiB) a plain `write`
//! wins, which is why servers send small responses from memory and large static files with sendfile. (4) TLS defeats sendfile
//! (the kernel cannot encrypt for you unless kTLS is enabled). (5) Always set socket timeouts in tests.
//!
//! COMMON USES: nginx/Apache static files (`sendfile on;`), Kafka brokers serving consumer fetches straight from the page cache,
//! `cp --reflink`, `cp -c` on macOS, rsync/backup tools, proxies (HAProxy splices between sockets), Rust servers that hold
//! response bodies as `Bytes` (hyper/tonic/tokio-util codecs).
//! Read: https://man7.org/linux/man-pages/man2/sendfile.2.html https://man7.org/linux/man-pages/man2/splice.2.html https://man7.org/linux/man-pages/man2/tee.2.html https://man7.org/linux/man-pages/man2/vmsplice.2.html https://man7.org/linux/man-pages/man2/copy_file_range.2.html https://doc.rust-lang.org/std/io/fn.copy.html https://docs.rs/bytes
use bytes::Bytes;
use memmap2::Mmap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const SIZE: usize = 64 << 20; // 64 MiB
const TIMEOUT: Duration = Duration::from_secs(20);

/// Order-dependent word checksum (not cryptographic): detects missing, duplicated, reordered or corrupt data.
fn checksum(data: &[u8]) -> u64 {
    let (words, tail) = data.as_chunks::<8>();
    let mut h = data.len() as u64;
    for w in words {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*w)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    for &b in tail {
        h = (h.rotate_left(5) ^ b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    h
}
fn checksum_file(p: &Path) -> io::Result<u64> {
    Ok(checksum(&fs::read(p)?))
}

/// Counts the calls made to the inner reader/writer: with plain files and sockets each call is one syscall.
struct CountR<R> {
    inner: R,
    calls: u64,
}
impl<R: Read> Read for CountR<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        self.inner.read(b)
    }
}
struct CountW<W> {
    inner: W,
    calls: u64,
}
impl<W: Write> Write for CountW<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        self.inner.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// `Ok(None)` = the method ran but the number of syscalls is internal to std; `Err` = not supported here.
type Calls = io::Result<Option<u64>>;

fn rw_loop(src: &File, dst: impl Write, bufsize: usize) -> Calls {
    let (mut r, mut w) = (
        CountR {
            inner: src,
            calls: 0,
        },
        CountW {
            inner: dst,
            calls: 0,
        },
    );
    let mut buf = vec![0u8; bufsize];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
    }
    Ok(Some(r.calls + w.calls))
}

fn unsupported(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, what.to_string())
}

// ------------------------------------------------------------------------------------------------ file -> file
fn f_rw_64k(s: &Path, d: &Path) -> Calls {
    rw_loop(&File::open(s)?, File::create(d)?, 64 << 10)
}
fn f_rw_1m(s: &Path, d: &Path) -> Calls {
    rw_loop(&File::open(s)?, File::create(d)?, 1 << 20)
}
fn f_io_copy(s: &Path, d: &Path) -> Calls {
    io::copy(&mut File::open(s)?, &mut File::create(d)?)?; // Linux: copy_file_range; macOS: 8 KiB loop
    Ok(None)
}
fn f_io_copy_counted(s: &Path, d: &Path) -> Calls {
    // wrapper types defeat std's Linux specialisation: this is always the generic 8 KiB loop
    let mut r = CountR {
        inner: File::open(s)?,
        calls: 0,
    };
    let mut w = CountW {
        inner: File::create(d)?,
        calls: 0,
    };
    io::copy(&mut r, &mut w)?;
    Ok(Some(r.calls + w.calls))
}
fn f_fs_copy(s: &Path, d: &Path) -> Calls {
    fs::copy(s, d)?; // macOS: fcopyfile; Linux: copy_file_range loop
    Ok(None)
}
fn f_mmap_write(s: &Path, d: &Path) -> Calls {
    let f = File::open(s)?;
    // SAFETY: the file is private to this process and is not modified or truncated while mapped (this recipe owns the temp dir).
    let map = unsafe { Mmap::map(&f)? };
    let mut w = CountW {
        inner: File::create(d)?,
        calls: 0,
    };
    w.write_all(&map)?;
    Ok(Some(w.calls))
}
#[cfg(target_os = "macos")]
fn f_clone(s: &Path, d: &Path) -> Calls {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let (cs, cd) = (
        CString::new(s.as_os_str().as_bytes()).unwrap(),
        CString::new(d.as_os_str().as_bytes()).unwrap(),
    );
    // SAFETY: both pointers are valid NUL-terminated strings that outlive the call; clonefile does not retain them.
    let r = unsafe { libc::clonefile(cs.as_ptr(), cd.as_ptr(), 0) };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Some(1))
}
#[cfg(target_os = "linux")]
fn f_clone(s: &Path, d: &Path) -> Calls {
    let (src, dst) = (File::open(s)?, File::create(d)?);
    // SAFETY: FICLONE takes the source fd as its argument; both fds are open for the whole call.
    let r = unsafe { libc::ioctl(dst.as_raw_fd(), libc::FICLONE, src.as_raw_fd()) };
    if r != 0 {
        return Err(io::Error::last_os_error()); // EOPNOTSUPP/EXDEV on ext4, tmpfs, cross-device
    }
    Ok(Some(1))
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn f_clone(_: &Path, _: &Path) -> Calls {
    Err(unsupported("no clone primitive on this OS"))
}

#[cfg(target_os = "linux")]
fn f_copy_file_range(s: &Path, d: &Path) -> Calls {
    let (src, dst) = (File::open(s)?, File::create(d)?);
    let (mut left, mut calls) = (src.metadata()?.len(), 0u64);
    while left > 0 {
        // SAFETY: valid open fds, null offsets = use and advance the files' own offsets; `left` bounds the count.
        let n = unsafe {
            libc::copy_file_range(
                src.as_raw_fd(),
                std::ptr::null_mut(),
                dst.as_raw_fd(),
                std::ptr::null_mut(),
                left as usize,
                0,
            )
        };
        calls += 1;
        match n {
            -1 if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => continue,
            -1 => return Err(io::Error::last_os_error()),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => left -= n as u64,
        }
    }
    Ok(Some(calls))
}
#[cfg(not(target_os = "linux"))]
fn f_copy_file_range(_: &Path, _: &Path) -> Calls {
    Err(unsupported("copy_file_range is Linux/FreeBSD"))
}

#[cfg(target_os = "linux")]
fn f_sendfile(s: &Path, d: &Path) -> Calls {
    let (src, dst) = (File::open(s)?, File::create(d)?);
    sendfile_loop(&src, dst.as_raw_fd(), src.metadata()?.len())
}
#[cfg(not(target_os = "linux"))]
fn f_sendfile(_: &Path, _: &Path) -> Calls {
    Err(unsupported(
        "sendfile to a regular file: Linux only (macOS needs a socket)",
    ))
}

// ------------------------------------------------------------------------------------------------ sendfile(2)
/// Linux: `sendfile(out_fd, in_fd, &mut offset, count)` returns bytes sent.
#[cfg(target_os = "linux")]
fn sendfile_loop(file: &File, out_fd: i32, total: u64) -> Calls {
    let (mut off, mut calls) = (0 as libc::off_t, 0u64);
    while (off as u64) < total {
        // SAFETY: both fds are open; `off` is a live off_t the kernel updates; count never exceeds what is left.
        let n = unsafe {
            libc::sendfile(
                out_fd,
                file.as_raw_fd(),
                &mut off,
                (total - off as u64) as usize,
            )
        };
        calls += 1;
        match n {
            -1 if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => continue,
            -1 => return Err(io::Error::last_os_error()),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            _ => {}
        }
    }
    Ok(Some(calls))
}
/// macOS: `sendfile(file, socket, offset, &mut len, NULL, 0)`; `len` is bytes wanted on entry and bytes SENT on return.
#[cfg(target_os = "macos")]
fn sendfile_loop(file: &File, sock_fd: i32, total: u64) -> Calls {
    let (mut sent, mut calls) = (0u64, 0u64);
    while sent < total {
        let mut len = (total - sent) as libc::off_t;
        // SAFETY: both fds are open; `len` is a live off_t; no header/trailer (null hdtr); sock_fd is a connected TCP socket.
        let r = unsafe {
            libc::sendfile(
                file.as_raw_fd(),
                sock_fd,
                sent as libc::off_t,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        calls += 1;
        sent += len as u64; // valid even when r == -1 (EINTR/EAGAIN after partial progress)
        if r == -1 {
            let e = io::Error::last_os_error();
            match e.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock if len > 0 => continue, // partial send before the socket timeout
                _ => return Err(e),
            }
        }
    }
    Ok(Some(calls))
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn sendfile_loop(_: &File, _: i32, _: u64) -> Calls {
    Err(unsupported("sendfile not wired up for this OS"))
}

/// Linux only: file -> pipe -> socket with `splice(2)`, never copying the bytes into user space.
#[cfg(target_os = "linux")]
fn splice_loop(file: &File, sock_fd: i32, total: u64) -> Calls {
    use std::os::fd::{FromRawFd, OwnedFd};
    let mut fds = [0i32; 2];
    // SAFETY: `fds` has room for the two descriptors pipe(2) writes.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: pipe(2) just returned these two fresh descriptors that nothing else owns; OwnedFd closes them on drop.
    let (rd, wr) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    // SAFETY: plain fcntl on an owned pipe fd; a bigger pipe means fewer, larger splices (default is 64 KiB).
    unsafe { libc::fcntl(wr.as_raw_fd(), libc::F_SETPIPE_SZ, 1 << 20) };
    let (mut off, mut calls) = (0 as libc::loff_t, 0u64);
    while (off as u64) < total {
        let chunk = ((total - off as u64) as usize).min(1 << 20);
        // SAFETY: valid fds; `off` is a live loff_t for the file side; the pipe side takes a null offset.
        let n = unsafe {
            libc::splice(
                file.as_raw_fd(),
                &mut off,
                wr.as_raw_fd(),
                std::ptr::null_mut(),
                chunk,
                libc::SPLICE_F_MOVE,
            )
        };
        calls += 1;
        if n <= 0 {
            return Err(if n == 0 {
                io::ErrorKind::UnexpectedEof.into()
            } else {
                io::Error::last_os_error()
            });
        }
        let mut pending = n as usize;
        while pending > 0 {
            // SAFETY: valid fds, null offsets on both pipe/socket sides, `pending` bytes are sitting in the pipe.
            let m = unsafe {
                libc::splice(
                    rd.as_raw_fd(),
                    std::ptr::null_mut(),
                    sock_fd,
                    std::ptr::null_mut(),
                    pending,
                    libc::SPLICE_F_MOVE | libc::SPLICE_F_MORE,
                )
            };
            calls += 1;
            if m <= 0 {
                return Err(io::Error::last_os_error());
            }
            pending -= m as usize;
        }
    }
    Ok(Some(calls))
}
#[cfg(not(target_os = "linux"))]
fn splice_loop(_: &File, _: i32, _: u64) -> Calls {
    Err(unsupported("splice(2) is Linux only"))
}

// ------------------------------------------------------------------------------------------------ file -> loopback socket
fn s_rw_64k(f: &File, s: &mut TcpStream) -> Calls {
    rw_loop(f, s, 64 << 10)
}
fn s_io_copy(f: &File, s: &mut TcpStream) -> Calls {
    let mut f = f; // &File implements Read
    io::copy(&mut f, s)?; // Linux: sendfile/splice specialisation; macOS: 8 KiB loop
    Ok(None)
}
fn s_io_copy_counted(f: &File, s: &mut TcpStream) -> Calls {
    let mut r = CountR { inner: f, calls: 0 };
    let mut w = CountW { inner: s, calls: 0 };
    io::copy(&mut r, &mut w)?;
    Ok(Some(r.calls + w.calls))
}
fn s_mmap_write(f: &File, s: &mut TcpStream) -> Calls {
    // SAFETY: the file is private to this process and is not modified or truncated while mapped.
    let map = unsafe { Mmap::map(f)? };
    let mut w = CountW { inner: s, calls: 0 };
    w.write_all(&map)?;
    Ok(Some(w.calls))
}
fn s_sendfile(f: &File, s: &mut TcpStream) -> Calls {
    sendfile_loop(f, s.as_raw_fd(), f.metadata()?.len())
}
fn s_splice(f: &File, s: &mut TcpStream) -> Calls {
    splice_loop(f, s.as_raw_fd(), f.metadata()?.len())
}

/// Run one file->socket method against a receiver thread that reads all bytes into `buf`. Returns (seconds, calls, buf).
fn run_socket(
    listener: &TcpListener,
    f: fn(&File, &mut TcpStream) -> Calls,
    src: &File,
    buf: Vec<u8>,
) -> io::Result<(f64, Option<u64>, Vec<u8>)> {
    // `&File` shares one offset across calls: rewind so every repetition reads from the start
    let mut rewind = src;
    rewind.seek(SeekFrom::Start(0))?;
    let l = listener.try_clone()?;
    let rx = thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut buf = buf;
        let (mut s, _) = l.accept()?;
        s.set_read_timeout(Some(TIMEOUT))?;
        s.read_exact(&mut buf)?;
        Ok(buf)
    });
    let mut tx = TcpStream::connect(listener.local_addr()?)?;
    tx.set_write_timeout(Some(TIMEOUT))?;
    let t = Instant::now();
    let sent = f(src, &mut tx);
    drop(tx); // closes the socket: if the sender failed the receiver sees EOF instead of waiting for its timeout
    let got = rx.join().expect("receiver panicked");
    let secs = t.elapsed().as_secs_f64();
    Ok((secs, sent?, got?))
}

// ------------------------------------------------------------------------------------------------ bytes::Bytes
fn bytes_demo(src: &Path, want: u64) -> io::Result<()> {
    println!("\n== bytes::Bytes: user-space zero-copy (no memcpy for clone / slice / split) ==");
    let v: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
    let vptr = v.as_ptr();
    let b = Bytes::from(v);
    assert_eq!(
        b.as_ptr(),
        vptr,
        "Bytes::from(Vec) must reuse the allocation"
    );
    assert!(b.is_unique());
    let c = b.clone();
    assert_eq!(c.as_ptr(), b.as_ptr(), "clone shares the same memory");
    assert!(
        !b.is_unique() && !c.is_unique(),
        "two handles: refcount is 2"
    );
    let s = b.slice(100..200);
    assert_eq!(
        s.as_ptr(),
        b.as_ptr().wrapping_add(100),
        "slice is a pointer + length into the same buffer"
    );
    assert_eq!(
        &s[..],
        &(100..200u32).map(|i| i as u8).collect::<Vec<_>>()[..]
    );
    drop(b);
    drop(c);
    assert!(
        s.is_unique(),
        "after the other handles drop, the slice is the only owner of the whole 1000-byte buffer"
    );
    let copy = Bytes::copy_from_slice(&s);
    assert_ne!(
        copy.as_ptr(),
        s.as_ptr(),
        "copy_from_slice really copies (and frees the big buffer's pin)"
    );
    let mut framing = Bytes::from(vec![7u8; 64]);
    let base = framing.as_ptr();
    let head = framing.split_to(16); // protocol framing: peel a header off the front, no copy
    assert_eq!(head.as_ptr(), base);
    assert_eq!(framing.as_ptr(), base.wrapping_add(16));
    assert_eq!((head.len(), framing.len()), (16, 48));
    println!(
        "Vec -> Bytes, clone, slice, split_to: all pointer-equal to the original allocation; refcount observed via is_unique()"
    );

    // a 64 MiB buffer: Vec::clone copies, Bytes::clone does not
    let big = vec![1u8; SIZE];
    let t = Instant::now();
    let cloned = std::hint::black_box(big.clone());
    let vec_t = t.elapsed();
    let bb = Bytes::from(big);
    let t = Instant::now();
    let mut keep = Vec::with_capacity(1_000_000);
    for _ in 0..1_000_000 {
        keep.push(bb.clone());
    }
    let bytes_t = t.elapsed();
    assert!(keep.iter().all(|k| k.as_ptr() == bb.as_ptr()));
    println!(
        "Vec<u8>::clone of 64 MiB: {vec_t:.2?}   1,000,000 x Bytes::clone of the same 64 MiB: {bytes_t:.2?} (shared, {} handles)",
        keep.len() + 1
    );
    drop(cloned);

    // mmap as Bytes: slices of a file with no read() and no copy
    let f = File::open(src)?;
    // SAFETY: the file is private to this process and is not modified or truncated while mapped.
    let map = unsafe { Mmap::map(&f)? };
    let mptr = map.as_ptr();
    let mb = Bytes::from_owner(map);
    assert_eq!(
        mb.as_ptr(),
        mptr,
        "Bytes::from_owner keeps the mapping in place"
    );
    let part = mb.slice(4096..8192);
    assert_eq!(part.as_ptr(), mptr.wrapping_add(4096));
    let mut check = vec![0u8; 4096];
    std::os::unix::fs::FileExt::read_exact_at(&f, &mut check, 4096)?;
    assert_eq!(
        &part[..],
        &check[..],
        "mapped slice equals pread() of the same range"
    );
    assert_eq!(
        checksum(&mb),
        want,
        "whole mapped file as Bytes has the source checksum"
    );
    println!(
        "Bytes::from_owner(Mmap): {} MiB mapped, slice(4096..8192) is pointer-equal and byte-equal to pread()",
        mb.len() >> 20
    );
    Ok(())
}

// ------------------------------------------------------------------------------------------------ main
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mbps(secs: f64) -> f64 {
    SIZE as f64 / 1e6 / secs
}

fn main() -> io::Result<()> {
    let dir = TempDir(std::env::temp_dir().join(format!("zc_cookbook_{}", std::process::id())));
    fs::create_dir_all(&dir.0)?;
    let (src, dst) = (dir.0.join("src.bin"), dir.0.join("dst.bin"));
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let data: Vec<u8> = (0..SIZE / 8)
        .flat_map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x.to_le_bytes()
        })
        .collect();
    let want = checksum(&data);
    fs::write(&src, &data)?;
    drop(data);
    assert_eq!(checksum_file(&src)?, want);
    println!(
        "os = {}  file = {} MiB  checksum = {want:#018x}\n",
        std::env::consts::OS,
        SIZE >> 20
    );

    let mut ran: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    println!("== file -> file (best of 3, page cache to page cache) ==");
    println!(
        "{:<34}{:>9}{:>10}   user-space copies of the data",
        "method", "MB/s", "calls"
    );
    type FileM = fn(&Path, &Path) -> Calls;
    let file_methods: [(&str, &str, FileM); 9] = [
        (
            "read/write loop, 64 KiB buffer",
            "2 (read, write)",
            f_rw_64k,
        ),
        ("read/write loop, 1 MiB buffer", "2", f_rw_1m),
        (
            "io::copy (std specialisation)",
            "OS dependent: 0 on Linux, 2 on macOS",
            f_io_copy,
        ),
        ("io::copy via wrappers (8 KiB loop)", "2", f_io_copy_counted),
        (
            "std::fs::copy",
            "0 (APFS clone observed / copy_file_range)",
            f_fs_copy,
        ),
        ("mmap + write_all", "1 (page cache -> file)", f_mmap_write),
        ("clonefile / FICLONE (COW)", "0 and no data moved", f_clone),
        ("copy_file_range loop", "0 (Linux)", f_copy_file_range),
        ("sendfile file->file", "0 (Linux)", f_sendfile),
    ];
    for (name, copies, f) in file_methods {
        let mut best = f64::MAX;
        let mut calls = None;
        let mut failure = None;
        for _ in 0..3 {
            let _ = fs::remove_file(&dst); // clonefile requires that the destination does not exist
            let t = Instant::now();
            match f(&src, &dst) {
                Ok(c) => {
                    best = best.min(t.elapsed().as_secs_f64());
                    calls = c;
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        match failure {
            None => {
                assert_eq!(
                    checksum_file(&dst)?,
                    want,
                    "{name}: destination differs from source"
                );
                println!(
                    "{name:<34}{:>9.0}{:>10}   {copies}",
                    mbps(best),
                    calls.map_or("std".to_string(), |c| c.to_string())
                );
                ran.push(name.to_string());
            }
            Some(e) => {
                println!("{name:<34}{:>9}{:>10}   ({e})", "-", "-");
                skipped.push(format!("{name}: {e}"));
            }
        }
    }
    let _ = fs::remove_file(&dst);

    println!(
        "\n== file -> loopback TCP (receiver thread checksums what it got; timeouts {TIMEOUT:?}) =="
    );
    println!(
        "{:<34}{:>9}{:>10}   user-space copies on the sender",
        "method", "MB/s", "calls"
    );
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let srcf = File::open(&src)?;
    type SockM = fn(&File, &mut TcpStream) -> Calls;
    let sock_methods: [(&str, &str, SockM); 6] = [
        ("read/write loop, 64 KiB buffer", "2", s_rw_64k),
        (
            "io::copy (std specialisation)",
            "OS dependent: 0 on Linux, 2 on macOS",
            s_io_copy,
        ),
        ("io::copy via wrappers (8 KiB loop)", "2", s_io_copy_counted),
        ("mmap + write_all", "1", s_mmap_write),
        ("sendfile(2)", "0", s_sendfile),
        ("splice(2) via pipe", "0 (Linux)", s_splice),
    ];
    let mut rbuf = vec![1u8; SIZE]; // receive buffer, prefaulted once and reused
    for (name, copies, f) in sock_methods {
        let (mut best, mut calls, mut failure) = (f64::MAX, None, None);
        for _ in 0..3 {
            match run_socket(&listener, f, &srcf, std::mem::take(&mut rbuf)) {
                Ok((secs, c, buf)) => {
                    best = best.min(secs);
                    calls = c;
                    assert_eq!(
                        checksum(&buf),
                        want,
                        "{name}: bytes received differ from the file"
                    );
                    rbuf = buf;
                    rbuf.iter_mut().step_by(4096).for_each(|b| *b = 0); // make stale data detectable
                }
                Err(e) => {
                    failure = Some(e);
                    rbuf = vec![1u8; SIZE];
                    break;
                }
            }
        }
        match failure {
            None => {
                println!(
                    "{name:<34}{:>9.0}{:>10}   {copies}",
                    mbps(best),
                    calls.map_or("std".to_string(), |c| c.to_string())
                );
                ran.push(format!("socket: {name}"));
            }
            Some(e) => {
                println!("{name:<34}{:>9}{:>10}   ({e})", "-", "-");
                skipped.push(format!("socket: {name}: {e}"));
            }
        }
    }

    bytes_demo(&src, want)?;

    println!(
        "\nRAN on this machine ({}): {}",
        std::env::consts::OS,
        ran.join("; ")
    );
    println!(
        "NOT RUN here: {}",
        if skipped.is_empty() {
            "(nothing skipped)".to_string()
        } else {
            skipped.join("; ")
        }
    );
    #[cfg(not(target_os = "linux"))]
    println!(
        "The Linux-only code (copy_file_range, sendfile to a file, splice via pipe, FICLONE) is compiled only with cfg(target_os = \"linux\") and was not executed here."
    );
    println!("All {} byte-for-byte checks passed.", ran.len());
    Ok(())
}
