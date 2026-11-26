//! RECIPE 241 (os): syscalls: what they are, how Rust reaches the kernel, fd ownership, cost, and counting them.
//! LEVEL: intermediate
//! Use in: debugging slow IO, writing FFI/low-level code, understanding what std does for you.   Run: cargo run --release --example syscalls_portable
#![allow(dead_code)]
//!
//! WHAT: a system call is a controlled jump from user mode into the kernel (trap instruction: `syscall` on x86-64,
//! `svc` on AArch64). The kernel checks arguments, does the work and returns a result in a register.
//! The layering in a Rust program is: your code -> `std` (File, TcpStream, Command) -> libc (glibc, musl, libSystem
//! on macOS; thin wrappers that set `errno`) -> kernel. On Linux you can skip libc with `libc::syscall(SYS_x, ..)`;
//! macOS discourages that (the stable ABI is libSystem, raw syscall numbers are private), and Windows has no stable
//! syscall numbers at all: the stable interface is kernel32/ntdll functions.
//!
//! ERRORS: libc wrappers return -1 and set the thread-local `errno` (read it with `io::Error::last_os_error()`).
//! The raw Linux kernel convention is different: the register holds `-errno` (values -4095..=-1 are errors); glibc's
//! `syscall()` function converts that back into the -1 + errno convention for you.
//!
//! OWNERSHIP: raw `RawFd` (an `i32`) has no owner, so double close and use-after-close are easy: fd numbers are
//! reused immediately, so a stale fd can silently refer to a different file. `OwnedFd` closes on drop (exactly once),
//! `BorrowedFd<'a>` is a lifetime-checked view. Prefer them in every new API.
//!
//! COST: a trivial syscall costs on the order of 100 ns to 1 us (mode switch, mitigations) versus ~20 ns for
//! `clock_gettime`/`Instant::now` on Linux, which the vDSO serves in user space without entering the kernel. This is
//! why buffering matters (see buffered_io_internals): fewer, larger syscalls. We count calls with wrapper `Read`/`Write` types.
//!
//! COMMON USES: strace/dtruss-driven debugging, sandboxes and seccomp filters (a syscall allow-list), container
//! runtimes, async runtimes (epoll/kevent/io_uring), custom allocators (mmap/brk), performance work that batches IO.
//! Read: https://man7.org/linux/man-pages/man2/syscalls.2.html https://doc.rust-lang.org/std/os/fd/index.html https://docs.rs/nix https://github.com/microsoft/windows-rs
use std::io::{self, Read, Write};

/// Wraps a writer and counts how many times the underlying `write` is called.
#[derive(Debug)]
struct CountingWrite<W> {
    inner: W,
    calls: usize,
    bytes: usize,
}
impl<W: Write> Write for CountingWrite<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = self.inner.write(buf)?;
        self.bytes += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
struct CountingRead<R> {
    inner: R,
    calls: usize,
    bytes: usize,
}
impl<R: Read> Read for CountingRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        let n = self.inner.read(buf)?;
        self.bytes += n;
        Ok(n)
    }
}
fn cw<W: Write>(inner: W) -> CountingWrite<W> {
    CountingWrite {
        inner,
        calls: 0,
        bytes: 0,
    }
}

fn errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(unix)]
mod unix_part {
    use super::*;
    use std::fs::File;
    use std::hint::black_box;
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
    use std::time::Instant;

    /// `pipe(2)` returning owned ends: the OwnedFd wrappers make leaking or double-closing impossible.
    fn pipe() -> (OwnedFd, OwnedFd) {
        let mut fds = [0; 2];
        // SAFETY: `fds` is a valid 2-element array for pipe to fill.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: pipe just created these two fds and nothing else owns them.
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
    }
    fn write_fd(fd: BorrowedFd<'_>, data: &[u8]) -> isize {
        // SAFETY: pointer/len come from a live slice; fd is valid for the borrow's lifetime.
        unsafe { libc::write(fd.as_raw_fd(), data.as_ptr().cast(), data.len()) }
    }
    fn read_fd(fd: BorrowedFd<'_>, buf: &mut [u8]) -> isize {
        // SAFETY: buf is a valid writable slice of the given length.
        unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) }
    }
    fn fd_is_open(fd: RawFd) -> bool {
        // SAFETY: F_GETFD only inspects the descriptor table.
        unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
    }

    pub fn demo() {
        println!("== 1. libc::write directly (std -> libc -> kernel, minus std) ==");
        let (r, w) = pipe();
        let n = write_fd(w.as_fd_(), b"hello kernel");
        assert_eq!(n, 12);
        let mut buf = [0u8; 32];
        let n = read_fd(r.as_fd_(), &mut buf);
        assert_eq!(&buf[..n as usize], b"hello kernel");
        // error convention: -1 and errno
        let bad = write_fd(unsafe { BorrowedFd::borrow_raw(9999) }, b"x"); // SAFETY: fd is never dereferenced by Rust; the kernel just rejects it
        assert_eq!(bad, -1);
        assert_eq!(errno(), libc::EBADF);
        println!(
            "write(9999) = -1, errno = {} ({})",
            errno(),
            io::Error::last_os_error()
        );

        println!("\n== 2. raw libc::syscall ==");
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            // SAFETY: SYS_getpid/SYS_gettid take no arguments; SYS_write gets a valid pointer/length.
            unsafe {
                assert_eq!(libc::syscall(libc::SYS_getpid) as u32, std::process::id());
                let tid = libc::syscall(libc::SYS_gettid);
                println!("pid {} tid {tid}", std::process::id());
                let n = libc::syscall(libc::SYS_write, w.as_raw_fd(), b"raw".as_ptr(), 3usize);
                assert_eq!(n, 3);
                let bad = libc::syscall(libc::SYS_write, 9999, b"x".as_ptr(), 1usize);
                assert_eq!((bad, errno()), (-1, libc::EBADF)); // glibc converted the kernel's -EBADF
            }
            let n = read_fd(r.as_fd_(), &mut buf);
            assert_eq!(&buf[..n as usize], b"raw");
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: SYS_getpid takes no arguments. macOS marks `syscall` deprecated; libSystem is the stable ABI.
            #[allow(deprecated)]
            let pid = unsafe {
                libc::syscall(
                    20, /* BSD SYS_getpid; libc crate has no SYS_* table on macOS */
                )
            };
            assert_eq!(pid as u32, std::process::id());
            println!("macOS: syscall(SYS_getpid) = {pid} (works, but prefer libc::getpid)");
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
        println!("raw syscall demo skipped on this OS");

        println!("\n== 3. fd ownership, dup, dup2, fcntl ==");
        let raw = w.as_raw_fd();
        let dupd = w.try_clone().unwrap(); // dup(2) with CLOEXEC set
        assert_ne!(dupd.as_raw_fd(), raw);
        write_fd(dupd.as_fd_(), b"via dup");
        let n = read_fd(r.as_fd_(), &mut buf);
        assert_eq!(&buf[..n as usize], b"via dup"); // both fds refer to one open file description
        // dup2 makes `target` an alias for the write end (this is how shells implement `> file`)
        // SAFETY: dup of stdin yields a fresh fd we own; dup2 then atomically replaces it.
        let target = unsafe { OwnedFd::from_raw_fd(libc::dup(0)) };
        // SAFETY: both fds are valid and open.
        assert_eq!(
            unsafe { libc::dup2(w.as_raw_fd(), target.as_raw_fd()) },
            target.as_raw_fd()
        );
        write_fd(target.as_fd_(), b"via dup2");
        let n = read_fd(r.as_fd_(), &mut buf);
        assert_eq!(&buf[..n as usize], b"via dup2");
        // CLOEXEC: close this fd automatically across exec so children do not inherit it
        // SAFETY: fcntl on valid fds with documented commands.
        unsafe {
            assert_eq!(libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC), 0);
            assert!(libc::fcntl(raw, libc::F_GETFD) & libc::FD_CLOEXEC != 0);
            // NONBLOCK: read on an empty pipe returns EAGAIN instead of sleeping
            let fl = libc::fcntl(r.as_raw_fd(), libc::F_GETFL);
            assert_eq!(
                libc::fcntl(r.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK),
                0
            );
        }
        assert_eq!(read_fd(r.as_fd_(), &mut buf), -1);
        assert_eq!(errno(), libc::EAGAIN);
        println!("nonblocking read on empty pipe -> EAGAIN (io::ErrorKind::WouldBlock)");
        // drop closes exactly once; the number is then free for reuse
        let dead = dupd.as_raw_fd();
        drop(dupd);
        assert!(!fd_is_open(dead));
        println!(
            "after drop(OwnedFd) fd {dead} is closed; with a bare RawFd nothing would stop a double close"
        );
        // converting into std types transfers ownership: File closes it for us
        let mut f = File::from(w);
        f.write_all(b"file over pipe").unwrap();
        let n = read_fd(r.as_fd_(), &mut buf);
        assert_eq!(&buf[..n as usize], b"file over pipe");
        assert_eq!(
            read_fd(r.as_fd_(), &mut buf),
            -1,
            "a write end (the dup2 target) is still open: EAGAIN, not EOF"
        );
        drop(f);
        drop(target);
        assert_eq!(
            read_fd(r.as_fd_(), &mut buf),
            0,
            "EOF only once every write end is closed"
        );

        println!("\n== 4. cost of a syscall (printed, machine dependent) ==");
        let iters = 200_000u32;
        let t = Instant::now();
        for _ in 0..iters {
            // SAFETY: getppid has no arguments and cannot fail (not cached by libc, unlike getpid on some libcs).
            black_box(unsafe { libc::getppid() });
        }
        let sys = t.elapsed().as_nanos() as f64 / iters as f64;
        let t = Instant::now();
        for _ in 0..iters {
            black_box(Instant::now());
        }
        let clk = t.elapsed().as_nanos() as f64 / iters as f64;
        let t = Instant::now();
        for i in 0..iters {
            black_box(i.wrapping_mul(2654435761));
        }
        let base = t.elapsed().as_nanos() as f64 / iters as f64;
        println!(
            "getppid {sys:.0} ns/call | Instant::now (vDSO/commpage) {clk:.0} ns | plain arithmetic {base:.1} ns"
        );
        // 1-byte write(2) to /dev/null: the cheapest IO syscall
        let mut devnull = File::create("/dev/null").unwrap();
        let t = Instant::now();
        for _ in 0..iters {
            devnull.write_all(b"x").unwrap();
        }
        println!(
            "1-byte write to /dev/null {:.0} ns/call",
            t.elapsed().as_nanos() as f64 / iters as f64
        );

        println!("\n== 5. counting the syscalls std makes ==");
        let lines = 1000;
        let mut unbuf = cw(Vec::new());
        for i in 0..lines {
            writeln!(unbuf, "line {i}").unwrap();
        }
        // each format piece is its own write_all: "line ", the number, "\n" (padding like {i:04} adds even more pieces!)
        println!(
            "unbuffered writeln!: {} write calls for {} lines ({} bytes)",
            unbuf.calls, lines, unbuf.bytes
        );
        assert_eq!(unbuf.calls, 3 * lines);
        let mut bw = io::BufWriter::new(cw(Vec::new()));
        for i in 0..lines {
            writeln!(bw, "line {i}").unwrap();
        }
        let inner = bw.into_inner().unwrap();
        println!(
            "BufWriter(8 KiB):    {} write calls for the same output",
            inner.calls
        );
        assert_eq!(inner.bytes, unbuf.bytes);
        assert!(inner.calls <= inner.bytes / 8000 + 1 && inner.calls >= 1);

        let data = vec![7u8; 1 << 20];
        let mut src = CountingRead {
            inner: &data[..],
            calls: 0,
            bytes: 0,
        };
        let mut dst = cw(io::sink());
        io::copy(&mut src, &mut dst).unwrap();
        println!(
            "io::copy of 1 MiB: {} reads, {} writes (8 KiB chunks + the EOF read)",
            src.calls, dst.calls
        );
        assert_eq!(dst.bytes, 1 << 20);
        assert_eq!(src.calls, 129);
        let mut one = CountingRead {
            inner: &data[..],
            calls: 0,
            bytes: 0,
        };
        let mut b = [0u8; 1];
        for _ in 0..1000 {
            one.read_exact(&mut b).unwrap();
        }
        let mut buffered = io::BufReader::new(CountingRead {
            inner: &data[..],
            calls: 0,
            bytes: 0,
        });
        for _ in 0..1000 {
            buffered.read_exact(&mut b).unwrap();
        }
        println!(
            "1000 one-byte reads: unbuffered {} calls vs BufReader {} call",
            one.calls,
            buffered.get_ref().calls
        );
        assert_eq!((one.calls, buffered.get_ref().calls), (1000, 1));
        // a real file through the same counter
        let path = std::env::temp_dir().join(format!("r241_{}.bin", std::process::id()));
        let mut f = cw(File::create(&path).unwrap());
        f.write_all(&data).unwrap();
        assert_eq!(f.calls, 1); // one large write_all = one write(2) when the kernel accepts it all
        std::fs::remove_file(&path).unwrap();

        table();
    }
    // helper so the code above reads cleanly on OwnedFd
    trait AsFd_ {
        fn as_fd_(&self) -> BorrowedFd<'_>;
    }
    impl AsFd_ for OwnedFd {
        fn as_fd_(&self) -> BorrowedFd<'_> {
            std::os::fd::AsFd::as_fd(self)
        }
    }

    fn table() {
        println!("\n== 6. std call -> syscall (Linux / macOS) ==");
        for (std_call, linux, mac) in [
            ("File::open", "openat", "open/openat"),
            ("File::read", "read", "read"),
            ("File::write", "write", "write"),
            (
                "FileExt::read_at / write_at",
                "pread64 / pwrite64",
                "pread / pwrite",
            ),
            (
                "write_vectored / read_vectored",
                "writev / readv",
                "writev / readv",
            ),
            (
                "io::copy File->File, fs::copy",
                "copy_file_range / sendfile",
                "fcopyfile / clonefile",
            ),
            (
                "fs::read_dir",
                "getdents64",
                "getdirentries64 (via readdir)",
            ),
            ("fs::metadata", "statx / newfstatat", "stat64 / fstatat"),
            ("memmap2::Mmap::map", "mmap", "mmap"),
            (
                "Mutex/Condvar/park contended",
                "futex",
                "__ulock_wait / psynch",
            ),
            ("mio/tokio poll", "epoll_wait", "kevent"),
            (
                "Instant::now",
                "clock_gettime (vDSO, no trap)",
                "mach_absolute_time (commpage)",
            ),
            (
                "thread::sleep",
                "clock_nanosleep",
                "nanosleep / __semwait_signal",
            ),
        ] {
            println!("  {std_call:34} {linux:30} {mac}");
        }
        println!("Trace them yourself:");
        println!(
            "  Linux: strace -f -c ./bin   (summary table)   strace -f -tt -e trace=%file,read,write ./bin   ltrace for libc calls"
        );
        println!(
            "  macOS: sudo dtruss -f ./bin   sudo fs_usage -w -f filesys <pid>   Instruments 'System Trace' (SIP limits dtruss)"
        );
    }
}

/// Windows has no public syscall numbers: user code calls kernel32 / ntdll. This module is a COMPILE-ONLY reference
/// that could not be run on the macOS host that produced this recipe. Real projects use the `windows-sys` crate
/// (raw bindings) or `windows` (safe-ish, https://github.com/microsoft/windows-rs); Unix users have `nix`
/// (https://docs.rs/nix). HANDLEs are the analogue of fds; `std::os::windows::io::OwnedHandle` closes on drop.
#[cfg(windows)]
mod windows_part {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    type Handle = *mut c_void;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const CREATE_ALWAYS: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const MEM_COMMIT_RESERVE: u32 = 0x3000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_READWRITE: u32 = 0x04;
    unsafe extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            sec: *mut c_void,
            disp: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn ReadFile(
            h: Handle,
            buf: *mut c_void,
            n: u32,
            read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn WriteFile(
            h: Handle,
            buf: *const c_void,
            n: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(h: Handle) -> i32;
        fn GetLastError() -> u32;
        fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
        fn VirtualFree(addr: *mut c_void, size: usize, kind: u32) -> i32;
    }
    pub fn demo() {
        let invalid = -1isize as Handle; // INVALID_HANDLE_VALUE
        let path: Vec<u16> = std::env::temp_dir()
            .join("r241_win.bin")
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: `path` is NUL-terminated UTF-16; all other pointers are null or point to live locals.
        unsafe {
            let h = CreateFileW(
                path.as_ptr(),
                GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            );
            assert!(h != invalid, "CreateFileW failed: {}", GetLastError());
            let mut written = 0u32;
            assert!(
                WriteFile(
                    h,
                    b"hello".as_ptr().cast(),
                    5,
                    &mut written,
                    std::ptr::null_mut()
                ) != 0
            );
            CloseHandle(h);
            let h = CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                1,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            );
            let mut buf = [0u8; 8];
            let mut got = 0u32;
            assert!(
                ReadFile(
                    h,
                    buf.as_mut_ptr().cast(),
                    8,
                    &mut got,
                    std::ptr::null_mut()
                ) != 0
            );
            assert_eq!(&buf[..got as usize], b"hello");
            CloseHandle(h);
            let p = VirtualAlloc(
                std::ptr::null_mut(),
                4096,
                MEM_COMMIT_RESERVE,
                PAGE_READWRITE,
            ); // ~ mmap(MAP_ANON)
            assert!(!p.is_null());
            VirtualFree(p, 0, MEM_RELEASE);
        }
        let _ = std::fs::remove_file(std::env::temp_dir().join("r241_win.bin"));
        println!(
            "windows: CreateFileW/WriteFile/ReadFile/VirtualAlloc ok (errors come from GetLastError, not errno)"
        );
    }
}

fn main() {
    println!("layering: your code -> std -> libc -> kernel");
    #[cfg(unix)]
    unix_part::demo();
    #[cfg(windows)]
    windows_part::demo();
    println!("\nOK");
}
