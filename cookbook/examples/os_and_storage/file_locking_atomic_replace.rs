//! RECIPE 178 (os): crash-safe file replacement (temp + fsync + rename + dir fsync) and file locking (flock, PID lock, O_EXCL)
//! LEVEL: intermediate
//! Use in: config/state files, databases' metadata, package managers, single-instance daemons, cross-process mutual exclusion.   Run: cargo run --release --example file_locking_atomic_replace
#![allow(dead_code)]
//!
//! ATOMIC REPLACE: to update `state.json` so a crash leaves either the OLD or the NEW file, never a mix:
//!   1. write the new bytes to a NEW temp file IN THE SAME DIRECTORY (rename is atomic only within a filesystem). Each
//!      writer needs its own name: two writers sharing one can install a mix of both;
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
//! LIMITS OF THIS CODE: it keeps the old file's permission bits, not its owner, ACLs or extended attributes. A target that
//! is a symlink is replaced by a regular file (with the mode of the file the link pointed to). Concurrent replaces each
//! install a whole file and the last rename wins: an update that reads first still needs a lock. An error from the
//! directory sync means the new content is already visible but may not be durable. A crash leaves its temp file behind and
//! nothing here removes it. A name close to the filesystem's length limit leaves no room for the temp suffix.
//!
//! LOCKS (all ADVISORY: they only work if every party cooperates; nothing stops a rogue `write`):
//!  * `flock(2)`: whole-file lock attached to the open file description; auto-released when the last descriptor for that
//!    open file description is closed (`dup` and `fork` share it), as happens when its processes die (no stale locks!). Two separate `open`s conflict, even within one process (so it works between
//!    threads here). Shared (LOCK_SH, many readers) vs exclusive (LOCK_EX); `LOCK_NB` returns EWOULDBLOCK instead of waiting.
//!    Lock a dedicated `.lock` file, NOT the data file you replace by rename (the lock would stay on the old inode).
//!    Not reliable on some network filesystems (NFS).
//!  * PID lock file: create-exclusive, write your pid, delete on exit. Crashes leave the file behind, so a new starter
//!    checks `kill(pid, 0)` (signal 0 = existence check only): ESRCH means dead, so the lock is stale. Caveats: PID reuse,
//!    and two starters may both decide a lock is stale (the check-then-remove is racy; flock avoids that). A crash between
//!    create and write leaves a file with no pid, and every later starter refuses until someone removes it.
//!  * `O_EXCL|O_CREAT` (`create_new`): the kernel guarantees exactly one creator wins. A primitive for lock files and unique names.
//!
//! COMMON USES: editors and package managers saving files atomically (`git` and `apt` use lock files + rename), SQLite and
//! other embedded DBs using file locks, daemons refusing to start twice (pid files), cron-style jobs that must not overlap,
//! `.lock` files in build tools such as cargo's package-cache lock.
//! Read: https://man7.org/linux/man-pages/man2/flock.2.html and https://man7.org/linux/man-pages/man2/fsync.2.html and https://www.postgresql.org/docs/current/wal-reliability.html
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

// ---------- 1. atomic replace ----------
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all() // opening a directory read-only works on Unix; fsync persists its entries
}

/// The directory that holds `target`. A bare name like `state.json` has the empty parent `""`, which cannot be opened.
fn parent_dir(target: &Path) -> &Path {
    match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

fn temp_path(target: &Path, n: u64) -> io::Result<PathBuf> {
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "target has no file name"))?;
    let mut tmp = OsString::from(".");
    tmp.push(name);
    tmp.push(format!(".{}.{n}.tmp", std::process::id()));
    Ok(parent_dir(target).join(tmp))
}

/// Steps 1 and 2 done, 3 and 4 not yet. Dropping it removes the temp file.
struct Staged {
    tmp: PathBuf,
    target: PathBuf,
}
impl Staged {
    fn new(target: &Path, data: &[u8]) -> io::Result<Staged> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let old = match fs::metadata(target) {
            Ok(m) => Some(m.permissions()),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        for _ in 0..100 {
            let tmp = temp_path(target, NEXT.fetch_add(1, Ordering::Relaxed))?;
            // (1) every writer needs its OWN temp file. `create_new` also refuses to follow a symlink someone put there.
            let opened = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(if old.is_some() { 0o600 } else { 0o666 }) // both are narrowed by the umask
                .open(&tmp);
            let mut f = match opened {
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue, // e.g. litter from a crash
                other => other?,
            };
            let staged = Staged {
                tmp,
                target: target.to_owned(),
            };
            if let Some(perms) = old {
                f.set_permissions(perms)?; // a private file must not become readable; set before the data goes in
            }
            f.write_all(data)?;
            f.sync_all()?; // (2) data durable before it becomes visible (macOS: Rust uses F_FULLFSYNC here)
            return Ok(staged);
        }
        Err(io::Error::new(
            ErrorKind::AlreadyExists,
            "no free temp name",
        ))
    }

    /// An error from the LAST step means the new content is already visible, but may not survive a power failure.
    fn commit(self) -> io::Result<()> {
        fs::rename(&self.tmp, &self.target)?; // (3) atomic swap
        sync_dir(parent_dir(&self.target)) // (4) make the rename itself durable
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.tmp); // after a successful rename there is nothing left under this name
    }
}

fn atomic_replace(target: &Path, data: &[u8]) -> io::Result<()> {
    Staged::new(target, data)?.commit()
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
                        // kill(0, 0) and kill(-n, 0) ask about process GROUPS, so those are not a pid
                        .filter(|&pid| pid > 0)
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
    // a "crash" between steps 2 and 3 (nothing runs, not even drop): the target is still the complete old version
    let crashed = Staged::new(&target, br#"{"v":3}"#)?;
    let litter = crashed.tmp.clone();
    std::mem::forget(crashed);
    assert_eq!(fs::read_to_string(&target)?, r#"{"v":2,"more":"data"}"#);
    fs::remove_file(litter)?;
    // two writers of one target, interleaved: each has its own temp file, and each commit installs its own bytes whole
    let first = Staged::new(&target, b"first writer, the longer content")?;
    let second = Staged::new(&target, b"second")?;
    first.commit()?;
    assert_eq!(fs::read(&target)?, b"first writer, the longer content");
    second.commit()?;
    assert_eq!(fs::read(&target)?, b"second");
    // symlinks already sitting at the next temp names are skipped, never written through
    let victim = dir.join("victim");
    fs::write(&victim, b"precious")?;
    let guarded = dir.join("guarded");
    for n in 0..50 {
        let _ = std::os::unix::fs::symlink(&victim, temp_path(&guarded, n)?);
    }
    atomic_replace(&guarded, b"new")?;
    assert_eq!(
        (fs::read(&victim)?, fs::read(&guarded)?),
        (b"precious".to_vec(), b"new".to_vec())
    );
    for n in 0..50 {
        let _ = fs::remove_file(temp_path(&guarded, n)?);
    }
    // the permission bits of the file being replaced are kept
    // (0o755 matters: the temp file starts as 0o600, so a private file alone would pass without the copy)
    for mode in [0o600, 0o755] {
        fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
        atomic_replace(&target, b"secret")?;
        assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o777, mode);
    }
    // a bare file name is relative to the current directory
    let cwd = std::env::current_dir()?;
    std::env::set_current_dir(&dir)?;
    let bare = atomic_replace(Path::new("bare.json"), b"{}");
    std::env::set_current_dir(cwd)?;
    bare?;
    assert_eq!(fs::read(dir.join("bare.json"))?, b"{}");
    // a failed rename removes its temp file (the target here is a directory, so the temp file was already written)
    fs::create_dir(dir.join("a_directory"))?;
    assert!(atomic_replace(&dir.join("a_directory"), b"x").is_err());
    assert!(
        atomic_replace(&dir.join(".."), b"x").is_err(),
        "no file name"
    );
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

    // "-1" and "0" are not pids: kill(2) would answer about process groups
    for bad in ["-1", "0", "junk", ""] {
        fs::write(&pl, bad)?;
        let e = PidLock::acquire(&pl).err().expect("must refuse");
        assert_eq!(e.to_string(), "lock file unreadable", "content {bad:?}");
    }
    fs::remove_file(&pl)?;

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
