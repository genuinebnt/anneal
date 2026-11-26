//! RECIPE 178 (os): crash-safe file replacement (temp + fsync + rename + dir fsync) and file locking (flock, PID lock, O_EXCL)
//! LEVEL: intermediate
//! Use in: config/state files, databases' metadata, package managers, single-instance daemons, cross-process mutual exclusion.   Run: cargo run --release --example file_locking_atomic_replace
#![allow(dead_code)]
//!
//! ATOMIC REPLACE: to update `state.json` so a crash leaves either the OLD or the NEW file, never a mix:
//!   1. write the new bytes to a temp file IN THE SAME DIRECTORY (rename is atomic only within a filesystem);
//!   2. `fsync` the temp file (data durable BEFORE it becomes visible);
//!   3. `rename(tmp, target)` (atomic replace on POSIX);
//!   4. `fsync` the DIRECTORY (the rename is a directory-entry change; without this it may be lost on power failure
//!      even though the file data was synced).
//! WHAT FSYNC GUARANTEES: when it returns Ok, the kernel has asked the device to persist that file's data (and the
//! metadata needed to find it) and the device reported done. WHAT IT DOES NOT: it does not make a multi-file
//! change atomic; it does not protect other files or the directory entry (hence step 4); on macOS plain `fsync`
//! only reaches the drive's cache and `fcntl(F_FULLFSYNC)` is needed for a real flush (Rust's `sync_all` already
//! does this on macOS, noted below); drives or VMs that lie about flushes defeat it; and a failed fsync may have
//! dropped dirty pages, so never "retry fsync and carry on" (PostgreSQL's 2018 "fsyncgate"): treat it as fatal.
//! Without any fsync, `rename` still gives atomicity against process crashes, just not durability on power loss.
//!
//! LOCKS (all ADVISORY: they only work if every party cooperates; nothing stops a rogue `write`):
//!  * `flock(2)`: whole-file lock attached to the open file description; auto-released when the descriptor is closed
//!    or the process dies (no stale locks!). Two separate `open`s conflict, even within one process (so it works between
//!    threads here). Shared (LOCK_SH, many readers) vs exclusive (LOCK_EX); `LOCK_NB` returns EWOULDBLOCK instead of waiting.
//!    Lock a dedicated `.lock` file, NOT the data file you replace by rename (the lock would stay on the old inode).
//!    Not reliable on some network filesystems (NFS).
//!  * PID lock file: create-exclusive, write your pid, delete on exit. Crashes leave the file behind, so a new starter
//!    checks `kill(pid, 0)` (signal 0 = existence check only): ESRCH means dead, so the lock is stale. Caveats: PID reuse,
//!    and two starters may both decide a lock is stale (the check-then-remove is racy; flock avoids that).
//!  * `O_EXCL|O_CREAT` (`create_new`): the kernel guarantees exactly one creator wins. A primitive for lock files and unique names.
//!
//! COMMON USES: editors and package managers saving files atomically (`git` and `apt` use lock files + rename), SQLite and
//! other embedded DBs using file locks, daemons refusing to start twice (pid files), cron-style jobs that must not overlap,
//! `.lock` files in build tools such as cargo's package-cache lock.
//! Read: https://man7.org/linux/man-pages/man2/flock.2.html and https://man7.org/linux/man-pages/man2/fsync.2.html and https://www.postgresql.org/docs/current/wal-reliability.html
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

// ---------- 1. atomic replace ----------
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all() // opening a directory read-only works on Unix; fsync persists its entries
}

fn atomic_replace(target: &Path, data: &[u8]) -> io::Result<()> {
    let dir = target.parent().expect("target has a parent directory");
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        target.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?; // (2) data durable before it becomes visible (macOS: Rust uses F_FULLFSYNC here)
        drop(f);
        fs::rename(&tmp, target)?; // (3) atomic swap
        sync_dir(dir) // (4) make the rename itself durable
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp); // do not leave temp litter on failure
    }
    result
}

// ---------- 2. flock RAII guard ----------
struct FlockGuard {
    file: File, // closing the descriptor also releases the lock, even if drop is skipped by a crash
}
impl FlockGuard {
    fn lock(path: &Path, exclusive: bool, blocking: bool) -> io::Result<FlockGuard> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let mut op = if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        };
        if !blocking {
            op |= libc::LOCK_NB;
        }
        loop {
            // SAFETY: `file` owns a valid open descriptor for the duration of the call; flock has no other preconditions.
            let rc = unsafe { libc::flock(file.as_raw_fd(), op) };
            if rc == 0 {
                return Ok(FlockGuard { file });
            }
            let e = io::Error::last_os_error();
            if e.kind() != ErrorKind::Interrupted {
                return Err(e); // EWOULDBLOCK maps to ErrorKind::WouldBlock
            }
        }
    }
}
impl Drop for FlockGuard {
    fn drop(&mut self) {
        // SAFETY: descriptor is still open (owned by self.file); unlocking is always valid.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

// ---------- 3. PID lock file with stale detection ----------
fn process_alive(pid: i32) -> bool {
    // SAFETY: signal 0 performs only the existence/permission check; nothing is delivered.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    // EPERM = exists but belongs to someone else; ESRCH = no such process
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

struct PidLock {
    path: PathBuf,
}
impl PidLock {
    fn acquire(path: &Path) -> io::Result<PidLock> {
        for _ in 0..3 {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    write!(f, "{}", std::process::id())?;
                    f.sync_all()?;
                    return Ok(PidLock {
                        path: path.to_owned(),
                    });
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    match fs::read_to_string(path)
                        .ok()
                        .and_then(|s| s.trim().parse::<i32>().ok())
                    {
                        Some(pid) if process_alive(pid) => {
                            return Err(io::Error::new(
                                ErrorKind::WouldBlock,
                                format!("held by live pid {pid}"),
                            ));
                        }
                        Some(_) => {
                            let _ = fs::remove_file(path); // stale: owner is gone; retry the exclusive create
                        }
                        // empty/garbage: the owner may be between create and write, so be conservative
                        None => {
                            return Err(io::Error::new(
                                ErrorKind::WouldBlock,
                                "lock file unreadable",
                            ));
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("could not acquire pid lock"))
    }
}
impl Drop for PidLock {
    fn drop(&mut self) {
        // only remove it if it is still ours
        if fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            == Some(std::process::id())
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

// ---------- 5. the counter that proves mutual exclusion ----------
/// read-modify-write under an exclusive flock on a SEPARATE lock file (not on the data file)
fn locked_increment(dir: &Path) -> io::Result<()> {
    let _g = FlockGuard::lock(&dir.join("counter.lock"), true, true)?;
    let p = dir.join("counter");
    let n: u64 = fs::read_to_string(&p)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    fs::write(&p, (n + 1).to_string())?; // not atomic by itself; the lock is what makes the sequence safe
    Ok(())
} // guard dropped => unlocked

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--incr" {
        // child-process mode: one locked increment, then exit (invoked in a loop by /bin/sh below)
        return locked_increment(Path::new(&args[2]));
    }
    // watchdog so a bug can never hang the run
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(60));
        eprintln!("watchdog: timed out");
        std::process::exit(2);
    });

    let dir = std::env::temp_dir().join(format!("cb_r178_{}", std::process::id()));
    fs::create_dir_all(&dir)?;

    // 1. atomic replace
    let target = dir.join("state.json");
    atomic_replace(&target, br#"{"v":1}"#)?;
    atomic_replace(&target, br#"{"v":2,"more":"data"}"#)?;
    assert_eq!(fs::read_to_string(&target)?, r#"{"v":2,"more":"data"}"#);
    // a "crash" between steps 1 and 3 leaves a stray temp file but the target is still the complete old version
    fs::write(dir.join(".state.json.99999.tmp"), b"{\"v\":3,TRUNC")?;
    assert_eq!(fs::read_to_string(&target)?, r#"{"v":2,"more":"data"}"#);
    fs::remove_file(dir.join(".state.json.99999.tmp"))?;
    // failure path cleans its temp file: parent directory does not exist => error, nothing left behind
    assert!(atomic_replace(&dir.join("missing_dir/x"), b"x").is_err());
    let leftovers = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0);
    println!("1. atomic replace: old-or-new semantics hold, no temp litter");

    // 2. flock semantics
    let lp = dir.join("res.lock");
    {
        let s1 = FlockGuard::lock(&lp, false, false)?;
        let s2 = FlockGuard::lock(&lp, false, false)?; // shared + shared: fine
        assert_eq!(
            FlockGuard::lock(&lp, true, false).err().map(|e| e.kind()),
            Some(ErrorKind::WouldBlock)
        );
        drop((s1, s2));
        let x = FlockGuard::lock(&lp, true, false)?;
        assert_eq!(
            FlockGuard::lock(&lp, true, false).err().map(|e| e.kind()),
            Some(ErrorKind::WouldBlock)
        );
        assert_eq!(
            FlockGuard::lock(&lp, false, false).err().map(|e| e.kind()),
            Some(ErrorKind::WouldBlock)
        );
        drop(x); // RAII release
        drop(FlockGuard::lock(&lp, true, false)?); // available again
    }
    println!("2. flock: shared/shared ok, exclusive excludes everyone, released on drop");

    // 3. PID lock + stale detection
    let pl = dir.join("daemon.pid");
    {
        let _mine = PidLock::acquire(&pl)?;
        let e = PidLock::acquire(&pl)
            .err()
            .expect("second acquire must fail");
        assert_eq!(e.kind(), ErrorKind::WouldBlock);
        println!("3. second starter refused: {e}");
    }
    assert!(!pl.exists(), "released on drop");
    // fabricate a stale lock: the pid of a child that has already exited and been reaped
    let mut child = Command::new("true").spawn()?;
    let dead_pid = child.id() as i32;
    child.wait()?;
    assert!(!process_alive(dead_pid));
    assert!(process_alive(std::process::id() as i32));
    fs::write(&pl, dead_pid.to_string())?;
    let taken = PidLock::acquire(&pl)?; // detects stale, removes, re-creates
    assert_eq!(fs::read_to_string(&pl)?, std::process::id().to_string());
    drop(taken);
    println!("   stale lock of dead pid {dead_pid} was reclaimed");

    // 4. O_EXCL: of 16 racing threads exactly one creates the file
    let excl = Arc::new(dir.join("only_one"));
    let barrier = Arc::new(Barrier::new(16));
    let wins: usize = (0..16)
        .map(|_| {
            let (p, b) = (excl.clone(), barrier.clone());
            thread::spawn(move || {
                b.wait();
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&*p)
                    .is_ok() as usize
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .sum();
    assert_eq!(wins, 1);
    println!("4. create_new: exactly {wins} of 16 racers won");

    // 5. threads AND processes contending for one lock
    let exe = std::env::current_exe()?;
    let (threads, per_thread, procs, per_proc) = (4, 250, 2, 40);
    // /bin/sh loops, launching this binary once per increment ("$0" = exe, "$1" = dir)
    let script = format!(
        "i=0; while [ $i -lt {per_proc} ]; do \"$0\" --incr \"$1\" || exit 1; i=$((i+1)); done"
    );
    let mut kids: Vec<_> = (0..procs)
        .map(|_| {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(&script)
                .arg(&exe)
                .arg(&dir)
                .spawn()
        })
        .collect::<io::Result<_>>()?;
    let workers: Vec<_> = (0..threads)
        .map(|_| {
            let d = dir.clone();
            thread::spawn(move || {
                for _ in 0..per_thread {
                    locked_increment(&d).unwrap();
                }
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
    for k in kids.iter_mut() {
        assert!(k.wait()?.success());
    }
    let total: u64 = fs::read_to_string(dir.join("counter"))?
        .trim()
        .parse()
        .unwrap();
    let expected = (threads * per_thread + procs * per_proc) as u64;
    assert_eq!(total, expected, "lost updates would mean the lock failed");
    println!(
        "5. {threads} threads x {per_thread} + {procs} processes x {per_proc} increments = {total} (exact)"
    );

    fs::remove_dir_all(&dir)?;
    println!("all locking / atomic-replace assertions passed");
    Ok(())
}
