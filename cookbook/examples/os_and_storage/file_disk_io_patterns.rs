//! RECIPE 242 (storage): file and disk IO patterns: pread/pwrite, vectored IO, page cache, durability, sparse files, links.
//! LEVEL: advanced
//! Use in: databases, log-structured stores, caches, anything that reads/writes files at scale.   Run: cargo run --release --example file_disk_io_patterns
#![allow(dead_code)]
//!
//! WHAT: a tour of file IO behaviours that decide performance and crash safety, each with a small measurement.
//! Machine-dependent numbers are printed, never asserted; behaviours (data correctness, sparse allocation, link
//! semantics) are asserted. Temp files only, removed at the end (also on panic, via a guard).
//!
//! KEY IDEAS:
//!  * `FileExt::read_at/write_at` are `pread/pwrite`: the offset is an argument, the shared file cursor is untouched,
//!    so many threads can use one `&File` without a lock. `seek` + `read` is two syscalls and a race.
//!  * Vectored IO (`writev`/`readv`) sends several buffers in one syscall: header + payload without a copy.
//!  * The page cache serves repeated reads from RAM; sequential access triggers kernel readahead; random 4 KiB reads on
//!    a cold file are bounded by device IOPS. We cannot drop caches without root: Linux has `posix_fadvise(DONTNEED)`
//!    (per file, after fsync), macOS has `fcntl(F_NOCACHE)` (stop caching this fd's IO).
//!  * Durability: `write` only reaches the page cache. `fsync` asks the OS to push data+metadata to the device;
//!    `fdatasync` skips metadata that is not needed to read the data back (e.g. mtime). On macOS plain `fsync` hands
//!    the data to the drive but does NOT flush the drive's volatile cache; `fcntl(F_FULLFSYNC)` does (slower). Rust's
//!    `File::sync_all` uses `F_FULLFSYNC` on Apple platforms, which is why it is slow there.
//!  * `O_DIRECT` (Linux) bypasses the page cache: buffer address, length and file offset must be aligned to the
//!    logical block size (512 B or 4 KiB). Databases use it when they manage their own buffer pool.
//!  * Sparse files: `set_len` makes a hole; logical length != allocated blocks (`MetadataExt::blocks()` counts 512 B units).
//!  * Atomic creation: write a temp file in the same directory, fsync, `rename` over the target (atomic on POSIX), fsync
//!    the directory; `create_new` (O_EXCL) gives atomic "create if absent" for lock files.
//!  * `read_dir` order is unspecified: sort when you need determinism.
//!
//! COMMON USES: write-ahead logs (append + fdatasync), SSTable/B-tree page reads with pread, atomic config updates,
//! `cp --reflink`-style copies, sparse VM disk images, mail/queue spools (rename into place), backup tools using hard links.
//! Read: https://man7.org/linux/man-pages/man2/fsync.2.html https://man7.org/linux/man-pages/man2/open.2.html https://man7.org/linux/man-pages/man2/posix_fadvise.2.html https://doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html
#[cfg(unix)]
mod imp {
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, IoSlice, IoSliceMut, Read, Seek, SeekFrom, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }
    /// Removes the scratch directory on drop, even if an assert fires.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("r242_{}", std::process::id()));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn join(&self, n: &str) -> PathBuf {
            self.0.join(n)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const BLOCK: usize = 4096;
    const BIG: usize = 256 << 20; // scaled down automatically if it would be slow (see main)
    /// Deterministic content: word w of block b is a hash of (b, w), so any read can be verified without keeping a copy.
    fn word(b: u64, w: u64) -> u64 {
        (b.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ w.wrapping_mul(0xc2b2_ae3d_27d4_eb4f))
            .rotate_left(17)
    }
    fn fill_block(b: u64, out: &mut [u8]) {
        for (w, c) in out.chunks_exact_mut(8).enumerate() {
            c.copy_from_slice(&word(b, w as u64).to_le_bytes());
        }
    }
    fn check_block(b: u64, data: &[u8]) -> bool {
        data.chunks_exact(8)
            .enumerate()
            .all(|(w, c)| c == word(b, w as u64).to_le_bytes())
    }
    fn mbps(bytes: usize, d: Duration) -> f64 {
        bytes as f64 / (1 << 20) as f64 / d.as_secs_f64().max(1e-9)
    }
    fn timed<R>(f: impl FnOnce() -> R) -> (R, Duration) {
        let t = Instant::now();
        let r = f();
        (r, t.elapsed())
    }

    pub fn main() {
        let dir = TempDir::new();
        let total = Instant::now();

        // ---- build the big file (1 MiB writes)
        let big = dir.join("big.bin");
        let blocks = (BIG / BLOCK) as u64;
        let (_, d) = timed(|| {
            let mut f = io::BufWriter::with_capacity(1 << 20, File::create(&big).unwrap());
            let mut buf = vec![0u8; BLOCK];
            for b in 0..blocks {
                fill_block(b, &mut buf);
                f.write_all(&buf).unwrap();
            }
            f.flush().unwrap();
        });
        println!(
            "wrote {} MiB in {:.2}s = {:.0} MB/s (page cache, not yet durable)",
            BIG >> 20,
            d.as_secs_f64(),
            mbps(BIG, d)
        );
        assert_eq!(fs::metadata(&big).unwrap().len(), BIG as u64);

        // ---- sequential vs random reads with pread
        let f = File::open(&big).unwrap();
        let mut chunk = vec![0u8; 1 << 20];
        let (_, d) = timed(|| {
            for i in 0..(BIG >> 20) {
                f.read_exact_at(&mut chunk, (i << 20) as u64).unwrap();
            }
        });
        println!("sequential 1 MiB preads (warm): {:.0} MB/s", mbps(BIG, d));
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let n_rand = 100_000;
        let mut blk = vec![0u8; BLOCK];
        let (_, d) = timed(|| {
            for _ in 0..n_rand {
                let b = rng.next() % blocks;
                f.read_exact_at(&mut blk, b * BLOCK as u64).unwrap();
                assert!(
                    check_block(b, &blk),
                    "pread returned wrong data for block {b}"
                );
            }
        });
        println!(
            "random 4 KiB preads (warm): {:.0} IOPS = {:.0} MB/s (per-call syscall + copy cost; cold SSD is device bound)",
            n_rand as f64 / d.as_secs_f64(),
            mbps(n_rand * BLOCK, d)
        );
        println!(
            "  sequential is fast because the kernel detects the pattern and reads ahead asynchronously; random defeats it"
        );

        // ---- positional IO is thread safe: shared &File, no seeking
        let shared = &f;
        std::thread::scope(|s| {
            for t in 0..4u64 {
                s.spawn(move || {
                    let mut r = Rng(0xdead_beef + t);
                    let mut b4 = vec![0u8; BLOCK];
                    for _ in 0..5000 {
                        let b = r.next() % blocks;
                        shared.read_exact_at(&mut b4, b * BLOCK as u64).unwrap();
                        assert!(check_block(b, &b4));
                    }
                });
            }
        });
        let pw = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(dir.join("pw.bin"))
            .unwrap();
        pw.set_len(4 * 4096).unwrap();
        std::thread::scope(|s| {
            for t in 0..4u8 {
                let pw = &pw;
                s.spawn(move || {
                    for _ in 0..100 {
                        pw.write_all_at(&[t + 1; 4096], t as u64 * 4096).unwrap();
                    }
                });
            }
        });
        let mut all = vec![0u8; 4 * 4096];
        pw.read_exact_at(&mut all, 0).unwrap();
        for t in 0..4 {
            assert!(
                all[t * 4096..(t + 1) * 4096]
                    .iter()
                    .all(|&b| b == t as u8 + 1)
            );
        }
        println!("4 threads read_at/write_at on one shared File: no lock, no seek, data intact");
        // contrast: the seek cursor is shared state; read_at never moves it
        let mut g = File::open(&big).unwrap();
        g.seek(SeekFrom::Start(10)).unwrap();
        g.read_at(&mut [0u8; 8], 5000).unwrap();
        assert_eq!(g.stream_position().unwrap(), 10);

        // ---- vectored IO
        let vp = dir.join("vec.bin");
        {
            let mut vf = File::create(&vp).unwrap();
            let (hdr, body, trailer) = (b"HDR1".as_slice(), vec![9u8; 100], b"END".as_slice());
            let bufs = [
                IoSlice::new(hdr),
                IoSlice::new(&body),
                IoSlice::new(trailer),
            ];
            let want: usize = bufs.iter().map(|b| b.len()).sum();
            let n = vf.write_vectored(&bufs).unwrap(); // one writev(2) (may be short in theory)
            assert_eq!(n, want, "regular files accept the whole writev");
        }
        let mut vf = File::open(&vp).unwrap();
        let (mut h, mut b, mut t) = ([0u8; 4], [0u8; 100], [0u8; 3]);
        let n = vf
            .read_vectored(&mut [
                IoSliceMut::new(&mut h),
                IoSliceMut::new(&mut b),
                IoSliceMut::new(&mut t),
            ])
            .unwrap();
        assert_eq!((n, &h, &t), (107, b"HDR1", b"END"));
        println!("writev/readv: header + 100-byte body + trailer in one syscall each way");

        // ---- page cache: warm vs bypassed
        f.sync_all().unwrap();
        #[cfg(target_os = "linux")]
        {
            // SAFETY: fd is valid; offset 0 len 0 means "whole file"; DONTNEED only drops clean cached pages.
            let r = unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
            let (_, d) = timed(|| {
                for i in 0..(BIG >> 20) {
                    f.read_exact_at(&mut chunk, (i << 20) as u64).unwrap();
                }
            });
            println!(
                "after posix_fadvise(DONTNEED) (r={r}): sequential read {:.0} MB/s (closer to cold)",
                mbps(BIG, d)
            );
        }
        #[cfg(target_os = "macos")]
        {
            let c = File::open(&big).unwrap();
            // SAFETY: valid fd; F_NOCACHE=1 asks the kernel not to keep this fd's data in the unified buffer cache.
            let r = unsafe { libc::fcntl(c.as_raw_fd(), libc::F_NOCACHE, 1) };
            let (_, d) = timed(|| {
                for i in 0..(BIG >> 20) {
                    c.read_exact_at(&mut chunk, (i << 20) as u64).unwrap();
                }
            });
            println!(
                "with F_NOCACHE (r={r}): sequential read {:.0} MB/s (data already cached may still be served from RAM)",
                mbps(BIG, d)
            );
        }
        #[cfg(target_os = "linux")]
        o_direct(&dir.join("big.bin"));

        // ---- durability costs
        let dp = dir.join("dur.bin");
        let mut df = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&dp)
            .unwrap();
        let rounds = 20;
        let page = [1u8; BLOCK];
        let (_, d_none) = timed(|| {
            for _ in 0..rounds {
                df.write_all(&page).unwrap();
            }
        });
        let (_, d_data) = timed(|| {
            for _ in 0..rounds {
                df.write_all(&page).unwrap();
                df.sync_data().unwrap();
            }
        });
        let (_, d_all) = timed(|| {
            for _ in 0..rounds {
                df.write_all(&page).unwrap();
                df.sync_all().unwrap();
            }
        });
        let ms = |d: Duration| d.as_secs_f64() * 1000.0 / rounds as f64;
        println!(
            "4 KiB append: no sync {:.3} ms | sync_data {:.3} ms | sync_all {:.3} ms",
            ms(d_none),
            ms(d_data),
            ms(d_all)
        );
        #[cfg(target_os = "macos")]
        {
            // SAFETY: valid fd; plain fsync vs F_FULLFSYNC (flushes the drive cache too).
            let (_, d_fs) = timed(|| {
                for _ in 0..rounds {
                    df.write_all(&page).unwrap();
                    unsafe {
                        libc::fsync(df.as_raw_fd());
                    }
                }
            });
            let (_, d_full) = timed(|| {
                for _ in 0..rounds {
                    df.write_all(&page).unwrap();
                    unsafe {
                        libc::fcntl(df.as_raw_fd(), libc::F_FULLFSYNC);
                    }
                }
            });
            println!(
                "macOS: fsync {:.3} ms vs F_FULLFSYNC {:.3} ms per 4 KiB append (fsync alone is NOT durable against power loss)",
                ms(d_fs),
                ms(d_full)
            );
        }

        // ---- sparse file
        let sp = dir.join("sparse.bin");
        {
            let sf = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&sp)
                .unwrap();
            sf.set_len(1 << 30).unwrap();
            let m = sf.metadata().unwrap();
            let alloc = m.blocks() * 512;
            println!(
                "sparse: logical {} MiB, allocated {} KiB",
                m.len() >> 20,
                alloc >> 10
            );
            assert_eq!(m.len(), 1 << 30);
            assert!(alloc < m.len() / 100, "hole must not be allocated");
            sf.write_all_at(&[0xAB; 4096], 512 << 20).unwrap();
            let m2 = sf.metadata().unwrap();
            assert!(m2.blocks() * 512 < (1 << 20) && m2.blocks() >= m.blocks());
            let mut z = [1u8; 16];
            sf.read_exact_at(&mut z, 100 << 20).unwrap();
            assert_eq!(z, [0; 16], "holes read back as zeros");
            // SAFETY: valid fd; SEEK_DATA finds the next allocated region at or after the offset.
            let d = unsafe { libc::lseek(sf.as_raw_fd(), 0, libc::SEEK_DATA) };
            println!(
                "SEEK_DATA from 0 -> {} MiB (first allocated extent; filesystem-dependent granularity)",
                d >> 20
            );
        }
        fs::remove_file(&sp).unwrap();

        // ---- copying: fast paths
        let copy_src = dir.join("copy_src.bin");
        let sz = 64usize << 20;
        {
            let mut w = File::create(&copy_src).unwrap();
            let mut b = vec![0u8; 1 << 20];
            for i in 0..64u64 {
                fill_block(i, &mut b[..BLOCK]);
                w.write_all(&b).unwrap();
            }
        }
        let (n, d_fast) = timed(|| fs::copy(&copy_src, dir.join("c1.bin")).unwrap());
        assert_eq!(n, sz as u64);
        let (_, d_loop) = timed(|| {
            let mut r = File::open(&copy_src).unwrap();
            let mut w = File::create(dir.join("c2.bin")).unwrap();
            let mut b = [0u8; 4096];
            loop {
                let n = r.read(&mut b).unwrap();
                if n == 0 {
                    break;
                }
                w.write_all(&b[..n]).unwrap();
            }
        });
        assert_eq!(
            fs::read(dir.join("c1.bin")).unwrap(),
            fs::read(dir.join("c2.bin")).unwrap()
        );
        println!(
            "copy 64 MiB: fs::copy {:.0} MB/s (copy_file_range/fcopyfile, data stays in kernel) vs 4 KiB read/write loop {:.0} MB/s",
            mbps(sz, d_fast),
            mbps(sz, d_loop)
        );
        #[cfg(target_os = "linux")]
        {
            let (i, o) = (
                File::open(&copy_src).unwrap(),
                File::create(dir.join("c3.bin")).unwrap(),
            );
            // SAFETY: both fds valid; null offsets use and advance the files' own cursors.
            let n = unsafe {
                libc::copy_file_range(
                    i.as_raw_fd(),
                    std::ptr::null_mut(),
                    o.as_raw_fd(),
                    std::ptr::null_mut(),
                    1 << 20,
                    0,
                )
            };
            println!("copy_file_range copied {n} bytes in one syscall");
        }

        // ---- atomic patterns
        let target = dir.join("config.json");
        let atomic_write = |contents: &[u8]| -> io::Result<()> {
            let tmp = target.with_extension("tmp");
            let mut f = File::create(&tmp)?;
            f.write_all(contents)?;
            f.sync_all()?; // data durable before the name flips
            fs::rename(&tmp, &target)?;
            File::open(target.parent().unwrap())?.sync_all() // make the rename itself durable
        };
        atomic_write(b"v1").unwrap();
        atomic_write(b"version two").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"version two");
        assert!(!target.with_extension("tmp").exists());
        let lock = dir.join("lock");
        let _l = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
            .unwrap();
        assert_eq!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        println!(
            "atomic replace via tmp+fsync+rename+dir fsync; create_new (O_EXCL) gives a race-free lock file"
        );

        // ---- directory order, metadata, links
        let d2 = dir.join("d2");
        fs::create_dir(&d2).unwrap();
        for n in ["delta", "alpha", "charlie", "bravo"] {
            File::create(d2.join(n)).unwrap();
        }
        let raw: Vec<String> = fs::read_dir(&d2)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        let mut sorted = raw.clone();
        sorted.sort();
        println!("read_dir order (unspecified): {raw:?}; sorted: {sorted:?}");
        assert_eq!(sorted, ["alpha", "bravo", "charlie", "delta"]);
        let a = d2.join("alpha");
        fs::set_permissions(&a, fs::Permissions::from_mode(0o640)).unwrap();
        let m = fs::metadata(&a).unwrap();
        assert_eq!(m.mode() & 0o777, 0o640);
        println!(
            "stat: mode {:o} ino {} nlink {} mtime age {:?}",
            m.mode() & 0o7777,
            m.ino(),
            m.nlink(),
            m.modified().unwrap().elapsed().unwrap_or_default()
        );
        fs::write(&a, b"payload").unwrap();
        let hard = d2.join("alpha_hard");
        let sym = d2.join("alpha_sym");
        fs::hard_link(&a, &hard).unwrap();
        std::os::unix::fs::symlink(&a, &sym).unwrap();
        assert_eq!(
            fs::metadata(&hard).unwrap().ino(),
            fs::metadata(&a).unwrap().ino()
        );
        assert_eq!(fs::metadata(&a).unwrap().nlink(), 2);
        assert!(fs::symlink_metadata(&sym).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&sym).unwrap(), a);
        fs::remove_file(&a).unwrap();
        assert_eq!(
            fs::read(&hard).unwrap(),
            b"payload",
            "hard link keeps the inode alive"
        );
        assert_eq!(fs::read(&sym).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(fs::symlink_metadata(&sym).is_ok());
        println!(
            "hard link survives removal of the original name; symlink dangles (metadata follows, symlink_metadata does not)"
        );

        // ---- write batching experiment
        println!("write batching (unsynced, same file system):");
        for (size, count) in [(1usize, 262_144usize), (4096, 8192), (1 << 20, 32)] {
            let p = dir.join("batch.bin");
            let buf = vec![0x5a; size];
            let (_, d) = timed(|| {
                let mut f = File::create(&p).unwrap();
                for _ in 0..count {
                    f.write_all(&buf).unwrap();
                }
            });
            assert_eq!(fs::metadata(&p).unwrap().len(), (size * count) as u64);
            println!(
                "  {size:>8} B writes x {count:>6}: {:>8.1} MB/s  ({:.0} ns per call)",
                mbps(size * count, d),
                d.as_nanos() as f64 / count as f64
            );
        }
        println!("\ntotal runtime {:.1}s", total.elapsed().as_secs_f64());
        assert!(total.elapsed() < Duration::from_secs(60));
    }

    #[cfg(target_os = "linux")]
    fn o_direct(path: &std::path::Path) {
        use std::alloc::{Layout, alloc, dealloc};
        use std::os::unix::fs::OpenOptionsExt;
        // O_DIRECT demands aligned address, length and offset (here 4096).
        match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)
        {
            Ok(f) => {
                let layout = Layout::from_size_align(4096, 4096).unwrap();
                // SAFETY: layout has non-zero size; the buffer is freed with the same layout and only read after pread filled it.
                unsafe {
                    let p = alloc(layout);
                    let n = libc::pread(f.as_raw_fd(), p.cast(), 4096, 0);
                    println!(
                        "O_DIRECT aligned pread -> {n} (unaligned buffers would fail with EINVAL)"
                    );
                    dealloc(p, layout);
                }
            }
            Err(e) => println!("O_DIRECT unsupported here ({e}); tmpfs rejects it"),
        }
    }
}

fn main() {
    #[cfg(unix)]
    imp::main();
    #[cfg(not(unix))]
    println!(
        "this recipe uses Unix FileExt/MetadataExt; on Windows see FileExt::seek_read/seek_write"
    );
    println!("OK");
}
