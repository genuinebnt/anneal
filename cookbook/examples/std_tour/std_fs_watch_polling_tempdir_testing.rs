//! RECIPE 1070 (std_tour): testing code that touches the filesystem, a polling directory watcher, and single-instance guards, using only std.
//! LEVEL: intermediate
//! Use in: tests of anything that reads or writes files, config hot reload, cron-like daemons that must not run twice.   Run: cargo run --release --example std_fs_watch_polling_tempdir_testing
//! COMMON USES: a throwaway directory per test; fixture trees; golden-file tests; asserting error paths; "reload when the config changes"; one daemon per data directory.
//! METHODS: std::env::temp_dir, std::process::id, fs::create_dir, fs::create_dir_all, fs::remove_dir_all, fs::write, fs::read, fs::read_to_string,
//! fs::read_dir, DirEntry::path, fs::symlink_metadata, Metadata::modified, Metadata::len, fs::rename, fs::remove_file,
//! fs::set_permissions, PermissionsExt::from_mode, File::create_new, File::options, File::set_modified, File::try_lock, File::lock, File::unlock,
//! File::sync_all, File::metadata, std::fs::TryLockError, ErrorKind::{NotFound, AlreadyExists, PermissionDenied, StorageFull, DirectoryNotEmpty},
//! Path::strip_prefix
//! DOCS: https://doc.rust-lang.org/std/fs/struct.File.html
//! DEFINED IN: std: all of it (`std::fs`, `std::env`, `std::process`, `std::os::unix::fs`). `ErrorKind` is imported from `std::io` (its source is in
//!   `core::io` in 1.98.1, which is not a stable path). Nothing here is usable without std.
//! NOT COVERED (and why): native change notification (inotify, FSEvents, ReadDirectoryChangesW) is not in std: use the `notify` crate, and keep a polling
//! fallback for network filesystems. The `tempfile` crate (secure creation, `persist`) is what production code should usually use: see the crate tour.
//! File basics, atomic replace and walking are in `std_fs_file_operations_usecases`. Windows lock semantics were not run.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   isolate a test                               -> a RAII temp directory; never write to fixed paths or change the process cwd
//!   keep the directory to debug a failure        -> skip removal when `thread::panicking()` (print the path)
//!   compare large expected output                -> golden file + explicit "update" switch + first-difference message
//!   test "disk full"                             -> write through `impl Write`; inject a writer that fails with `ErrorKind::StorageFull`
//!   test "permission denied"                     -> chmod in the test (unix), and tolerate running as root
//!   branch on the failure                        -> `err.kind()`; avoid `path.exists()` followed by an operation (a race)
//!   detect changes by polling                    -> snapshot (len, mtime) per path and diff; add a content hash if false negatives matter
//!   test time-based logic on files               -> `File::set_modified` instead of sleeping until the mtime changes
//!   one instance per directory                   -> `File::try_lock` and KEEP the File alive; the OS releases it if the process dies
//!
//! PITFALLS: mtime resolution is filesystem dependent, so two quick writes can share a timestamp; a polling watcher misses changes that revert between
//! polls and sees a rename as remove + create; a lock may be only advisory; "pid file exists" lockfiles go stale after a crash.
//! TRY NEXT:
//!   - [S] Remove the hash from `Entry` and find the assertion that fails. Then explain which real editors or tools preserve mtime.
//!   - [S] Change `TempDir::new` to use only the process id. Which section breaks, and what would happen under `cargo test`'s parallel runner?
//!   - [M] Report renames: pair a Removed and a Created with equal hash and length. Write the case where this guesses wrong.
//!   - [M] Add debouncing: emit a change only after two consecutive polls agree. Test it by driving `poll` by hand.
//!   - [A] PROJECT: combine with `std_sync_primitives_use_cases` and `std_time_instant_duration_systemtime`: a config reloader thread that swaps an `Arc<Config>` and stops promptly on shutdown.
//! Read: https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock ; The Rust Book, test organisation https://doc.rust-lang.org/book/ch11-03-test-organization.html
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs::{self, File, TryLockError};
use std::hash::{DefaultHasher, Hasher};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ------------------------------------------------------------ TempDir
struct TempDir(PathBuf);
impl TempDir {
    /// Unique without reading the clock: process id + a process-wide counter, and `create_dir` (not `_all`) so that an
    /// existing directory (a leftover from an earlier process with the same pid) is detected and skipped, never reused.
    /// This is for tests. It does not defend against a hostile user on a shared temp directory; `tempfile` does more.
    fn new(label: &str) -> io::Result<TempDir> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("ff1070-{label}-{}-{n}", std::process::id()));
            match fs::create_dir(&p) {
                Ok(()) => return Ok(TempDir(p)),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.0.join(p)
    }
    /// Fixture builder: parents are created as needed.
    fn with_files(self, files: &[(&str, &str)]) -> io::Result<TempDir> {
        for (rel, body) in files {
            let p = self.join(rel);
            fs::create_dir_all(p.parent().expect("inside the temp dir"))?;
            fs::write(p, body)?;
        }
        Ok(self)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0); // best effort: Drop has no way to report an error, and must not panic
    }
}

// ------------------------------------------------------------ 1. Isolation and cleanup
fn s01_tempdir() -> io::Result<()> {
    let (a, b) = (TempDir::new("iso")?, TempDir::new("iso")?);
    assert_ne!(a.path(), b.path());
    let kept;
    {
        let t = TempDir::new("fx")?.with_files(&[
            ("etc/app.conf", "port=1\n"),
            ("data/a/b.txt", "x"),
            ("empty", ""),
        ])?;
        assert_eq!(fs::read_to_string(t.join("etc/app.conf"))?, "port=1\n");
        assert!(t.join("data/a").is_dir());
        kept = t.path().to_path_buf();
    }
    assert!(!kept.try_exists()?, "removed on drop");
    // cleanup also happens when the test body panics
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut seen = PathBuf::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let t = TempDir::new("panic")
            .unwrap()
            .with_files(&[("f", "1")])
            .unwrap();
        seen = t.path().to_path_buf();
        panic!("test failure");
    }));
    std::panic::set_hook(old);
    assert!(r.is_err() && !seen.as_os_str().is_empty() && !seen.try_exists()?);
    println!("1. temp directories ok");
    Ok(())
}

// ------------------------------------------------------------ 2. Golden files
#[derive(Debug, PartialEq)]
enum Golden {
    Match,
    Updated,
    Differs {
        line: usize,
        expected: String,
        actual: String,
    },
    Missing,
}

/// `update` is a parameter here; a test suite would set it from an environment variable such as UPDATE_GOLDEN=1.
fn check_golden(path: &Path, actual: &str, update: bool) -> io::Result<Golden> {
    let expected = match fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == ErrorKind::NotFound => None, // only "missing" is tolerated; other errors surface
        Err(e) => return Err(e),
    };
    if expected.as_deref() == Some(actual) {
        return Ok(Golden::Match);
    }
    if update {
        fs::write(path, actual)?;
        return Ok(Golden::Updated);
    }
    let Some(expected) = expected else {
        return Ok(Golden::Missing); // a missing golden file must FAIL, or a typo in its name passes forever
    };
    // split('\n') rather than lines(), so a difference in the final newline or a stray \r is still located
    let (mut e, mut a) = (expected.split('\n'), actual.split('\n'));
    let mut line = 0;
    loop {
        line += 1;
        match (e.next(), a.next()) {
            (x, y) if x == y && x.is_some() => {}
            (x, y) => {
                return Ok(Golden::Differs {
                    line,
                    expected: x.unwrap_or("<end>").into(),
                    actual: y.unwrap_or("<end>").into(),
                });
            }
        }
    }
}

fn render_report(rows: &[(&str, u32)]) -> String {
    let mut rows = rows.to_vec();
    rows.sort(); // golden output must be deterministic: sort anything that came from a HashMap or read_dir
    rows.iter()
        .map(|(n, c)| format!("{n:<8}{c:>4}\n"))
        .collect()
}

fn s02_golden() -> io::Result<()> {
    let t = TempDir::new("golden")?;
    let g = t.join("report.golden");
    let out = render_report(&[("web", 3), ("db", 12)]);
    assert_eq!(check_golden(&g, &out, false)?, Golden::Missing);
    assert_eq!(check_golden(&g, &out, true)?, Golden::Updated);
    assert_eq!(check_golden(&g, &out, false)?, Golden::Match);
    assert_eq!(fs::read_to_string(&g)?, "db        12\nweb        3\n");
    let changed = render_report(&[("web", 4), ("db", 12)]);
    assert_eq!(
        check_golden(&g, &changed, false)?,
        Golden::Differs {
            line: 2,
            expected: "web        3".into(),
            actual: "web        4".into()
        }
    );
    assert_eq!(
        check_golden(&g, out.trim_end(), false)?,
        Golden::Differs {
            line: 3,
            expected: String::new(),
            actual: "<end>".into()
        }
    );
    println!("2. golden files ok");
    Ok(())
}

// ------------------------------------------------------------ 3. Error paths: missing, existing, forbidden, disk full
fn load_or_default(p: &Path) -> io::Result<String> {
    match fs::read_to_string(p) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok("default".into()),
        other => other,
    }
}

/// Atomic save through an arbitrary writer wrapper: on ANY failure the old file is untouched and the temp file removed.
fn save_atomic<W: Write>(path: &Path, data: &[u8], wrap: impl FnOnce(File) -> W) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let file = File::create_new(&tmp)?; // 1.77; refuses to clobber a concurrent writer's temp file
    let result = (|| {
        let mut w = wrap(file.try_clone()?);
        w.write_all(data)?;
        w.flush()?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp); // keep the original error, not a cleanup error
    }
    result
}

struct DiskFull<W> {
    inner: W,
    room: usize,
}
impl<W: Write> Write for DiskFull<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.room == 0 {
            return Err(io::Error::new(
                ErrorKind::StorageFull,
                "simulated: no space left",
            ));
        }
        let n = b.len().min(self.room);
        let n = self.inner.write(&b[..n])?; // short writes first, as a real full disk can do
        self.room -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn s03_errors() -> io::Result<()> {
    let t = TempDir::new("err")?.with_files(&[("cfg", "v1"), ("dir/child", "x")])?;
    assert_eq!(load_or_default(&t.join("nope"))?, "default");
    assert_eq!(load_or_default(&t.join("cfg"))?, "v1");
    assert!(
        load_or_default(&t.join("dir")).is_err(),
        "reading a directory is not 'missing'"
    );
    assert_eq!(
        fs::read(t.join("no/such/parent")).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        File::create(t.join("no/parent")).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        fs::create_dir(t.join("dir")).unwrap_err().kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        File::create_new(t.join("cfg")).unwrap_err().kind(),
        ErrorKind::AlreadyExists
    );
    assert_eq!(
        fs::remove_dir(t.join("dir")).unwrap_err().kind(),
        ErrorKind::DirectoryNotEmpty
    ); // 1.83
    // the raw io::Error does not say WHICH path failed: add that context where you know it
    let p = t.join("missing");
    let e = fs::read(&p)
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", p.display())))
        .unwrap_err();
    assert!(e.kind() == ErrorKind::NotFound && e.to_string().contains("missing"));

    let cfg = t.join("cfg");
    save_atomic(&cfg, b"version two", |f| f)?;
    assert_eq!(fs::read_to_string(&cfg)?, "version two");
    for room in [0usize, 1, 7] {
        let e = save_atomic(&cfg, b"a much longer third version", |f| DiskFull {
            inner: f,
            room,
        })
        .unwrap_err();
        assert_eq!(e.kind(), ErrorKind::StorageFull);
        assert_eq!(
            fs::read_to_string(&cfg)?,
            "version two",
            "old content intact after failing at byte {room}"
        );
        assert!(!cfg.with_extension("tmp").try_exists()?, "no debris");
    }
    println!("3. error paths and injected disk-full ok");
    Ok(())
}

#[cfg(unix)]
fn s04_permissions() -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    /// Restores a mode on drop, so a failing assertion cannot leave an undeletable fixture behind.
    struct Mode(PathBuf, u32);
    impl Drop for Mode {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(self.1));
        }
    }
    let t = TempDir::new("perm")?.with_files(&[("secret", "s"), ("ro/keep", "k")])?;
    let _restore_file = Mode(t.join("secret"), 0o600);
    let _restore_dir = Mode(t.join("ro"), 0o755);
    fs::set_permissions(t.join("secret"), fs::Permissions::from_mode(0o000))?;
    match fs::read(t.join("secret")) {
        Ok(_) => {
            println!(
                "4. permission checks skipped: this user bypasses file modes (running as root?)"
            );
            return Ok(());
        }
        Err(e) => assert_eq!(e.kind(), ErrorKind::PermissionDenied),
    }
    assert!(
        fs::metadata(t.join("secret")).is_ok(),
        "stat needs search permission on the directory, not read on the file"
    );
    assert!(
        load_or_default(&t.join("secret")).is_err(),
        "must NOT be treated as missing"
    );

    fs::set_permissions(t.join("ro"), fs::Permissions::from_mode(0o555))?;
    assert_eq!(
        File::create(t.join("ro/new")).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    // unix: deleting or renaming is a change to the DIRECTORY, so a read-only directory protects its entries
    assert_eq!(
        fs::remove_file(t.join("ro/keep")).unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
    fs::write(t.join("ro/keep"), "changed")?; // but an existing writable FILE in it can still be modified
    println!("4. unix permission errors ok");
    Ok(())
}
#[cfg(not(unix))]
fn s04_permissions() -> io::Result<()> {
    println!("4. unix permission checks skipped on this platform");
    Ok(())
}

// ------------------------------------------------------------ 5. Polling watcher
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    len: u64,
    modified: SystemTime,
    hash: Option<u64>,
}
type Snapshot = BTreeMap<PathBuf, Entry>;

#[derive(Debug, PartialEq)]
enum Change {
    Created(String),
    Modified(String),
    Removed(String),
}

fn content_hash(p: &Path) -> io::Result<u64> {
    let mut f = File::open(p)?;
    let mut h = DefaultHasher::new(); // fine for change detection within one process; its algorithm is not stable across releases
    let mut buf = [0u8; 8192];
    loop {
        match f.read(&mut buf)? {
            0 => return Ok(h.finish()),
            n => h.write(&buf[..n]),
        }
    }
}

fn snapshot(root: &Path, hash: bool) -> io::Result<Snapshot> {
    let mut out = Snapshot::new();
    let mut todo = vec![root.to_path_buf()];
    // Files can vanish between listing and stat: in a watcher, NotFound is normal and not a failure.
    fn gone<T>(r: io::Result<T>) -> io::Result<Option<T>> {
        match r {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            r => r.map(Some),
        }
    }
    while let Some(dir) = todo.pop() {
        let Some(entries) = gone(fs::read_dir(&dir))? else {
            continue;
        };
        for entry in entries {
            let path = entry?.path();
            let Some(meta) = gone(fs::symlink_metadata(&path))? else {
                continue;
            };
            if meta.is_dir() {
                todo.push(path); // symlinks report is_symlink, not is_dir, so there are no link cycles here
            } else if meta.is_file() {
                let h = if hash {
                    gone(content_hash(&path))?
                } else {
                    None
                };
                let rel = path.strip_prefix(root).expect("under root").to_path_buf();
                out.insert(
                    rel,
                    Entry {
                        len: meta.len(),
                        modified: meta.modified()?,
                        hash: h,
                    },
                );
            }
        }
    }
    Ok(out)
}

fn diff(old: &Snapshot, new: &Snapshot) -> Vec<Change> {
    let name = |p: &Path| {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    };
    let mut out: Vec<Change> = new
        .iter()
        .filter_map(|(p, e)| match old.get(p) {
            None => Some(Change::Created(name(p))),
            Some(before) if before != e => Some(Change::Modified(name(p))),
            Some(_) => None,
        })
        .collect();
    out.extend(
        old.keys()
            .filter(|p| !new.contains_key(*p))
            .map(|p| Change::Removed(name(p))),
    );
    out
}

struct Watcher {
    root: PathBuf,
    hash: bool,
    last: Snapshot,
}
impl Watcher {
    fn new(root: &Path, hash: bool) -> io::Result<Watcher> {
        Ok(Watcher {
            root: root.to_path_buf(),
            hash,
            last: snapshot(root, hash)?,
        })
    }
    /// One step. The caller owns the timer, so tests never sleep.
    fn poll(&mut self) -> io::Result<Vec<Change>> {
        let now = snapshot(&self.root, self.hash)?;
        let changes = diff(&self.last, &now);
        self.last = now;
        Ok(changes)
    }
}

fn set_mtime(p: &Path, secs: u64) -> io::Result<()> {
    File::options()
        .write(true)
        .open(p)?
        .set_modified(UNIX_EPOCH + Duration::from_secs(secs)) // 1.75
}

fn s05_watcher() -> io::Result<()> {
    use Change::*;
    let t = TempDir::new("watch")?.with_files(&[("a.txt", "aaaa"), ("sub/b.txt", "b")])?;
    set_mtime(&t.join("a.txt"), 1_000_000)?;
    assert_eq!(
        fs::metadata(t.join("a.txt"))?.modified()?,
        UNIX_EPOCH + Duration::from_secs(1_000_000)
    );
    let mut cheap = Watcher::new(t.path(), false)?;
    let mut thorough = Watcher::new(t.path(), true)?;
    assert_eq!(cheap.poll()?, []);

    fs::write(t.join("sub/c.txt"), "new")?;
    fs::write(t.join("sub/b.txt"), "bigger")?;
    fs::remove_file(t.join("a.txt"))?;
    let want = [
        Modified("sub/b.txt".into()),
        Created("sub/c.txt".into()),
        Removed("a.txt".into()),
    ];
    assert_eq!(cheap.poll()?, want);
    assert_eq!(thorough.poll()?, want);
    assert_eq!(cheap.poll()?, [], "each change is reported once");

    // touch only: metadata changes, content does not
    set_mtime(&t.join("sub/c.txt"), 2_000_000)?;
    assert_eq!(cheap.poll()?, [Modified("sub/c.txt".into())]);
    thorough.poll()?;

    // same length AND restored mtime: invisible to metadata, caught by the hash
    fs::write(t.join("sub/c.txt"), "NEW")?;
    set_mtime(&t.join("sub/c.txt"), 2_000_000)?;
    assert_eq!(
        cheap.poll()?,
        [],
        "false negative of the metadata-only watcher"
    );
    assert_eq!(thorough.poll()?, [Modified("sub/c.txt".into())]);

    // rename = remove + create; a change that is undone between polls is invisible to every poller
    fs::rename(t.join("sub/c.txt"), t.join("d.txt"))?;
    assert_eq!(
        cheap.poll()?,
        [Created("d.txt".into()), Removed("sub/c.txt".into())]
    );
    fs::write(t.join("flash"), "!")?;
    fs::remove_file(t.join("flash"))?;
    assert_eq!(cheap.poll()?, []);
    println!("5. polling watcher ok");
    Ok(())
}

// ------------------------------------------------------------ 6. Single-instance guards
/// Holds the lock for as long as it is alive. The file is deliberately NOT deleted on release: if it were, one process
/// could lock the old (unlinked) file while another locks a newly created one, and both would believe they are alone.
struct InstanceLock {
    _file: File,
}
impl InstanceLock {
    fn acquire(path: &Path) -> io::Result<Option<InstanceLock>> {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(InstanceLock { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }
}

/// Fallback for filesystems without usable locks: existence of the file is the lock; the owner must refresh its mtime.
/// WEAKER: two contenders can both judge the same file stale, and the second removes the first's NEW file. A stalled
/// owner can also lose the lock while still running.
struct LeaseFile(PathBuf);
impl LeaseFile {
    fn acquire(path: &Path, now: SystemTime, ttl: Duration) -> io::Result<Option<LeaseFile>> {
        for _ in 0..2 {
            match File::create_new(path) {
                Ok(mut f) => {
                    writeln!(f, "{}", std::process::id())?; // for humans debugging; never trusted
                    return Ok(Some(LeaseFile(path.to_path_buf())));
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let age = now
                        .duration_since(fs::metadata(path)?.modified()?)
                        .unwrap_or(Duration::ZERO);
                    if age <= ttl {
                        return Ok(None);
                    }
                    fs::remove_file(path)?;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}
impl Drop for LeaseFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Child: try the lock; report; if told to hold, keep it until stdin closes (or the process is killed).
fn child_main(path: &Path, hold: bool) -> io::Result<()> {
    let lock = InstanceLock::acquire(path)?;
    println!("{}", if lock.is_some() { "acquired" } else { "busy" });
    io::stdout().flush()?;
    if hold {
        io::stdin().read_to_end(&mut Vec::new())?;
    }
    Ok(())
}

fn spawn_locker(path: &Path, hold: bool) -> io::Result<(std::process::Child, String)> {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--lock-child")
        .arg(path)
        .arg(if hold { "hold" } else { "probe" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut line = String::new();
    io::BufRead::read_line(
        &mut io::BufReader::new(child.stdout.as_mut().expect("piped")),
        &mut line,
    )?;
    Ok((child, line.trim().to_string()))
}

fn probe(path: &Path) -> io::Result<String> {
    let (mut child, said) = spawn_locker(path, false)?;
    assert!(child.wait()?.success());
    Ok(said)
}

fn s06_single_instance() -> io::Result<()> {
    let t = TempDir::new("lock")?;
    let path = t.join("app.lock");
    let first = InstanceLock::acquire(&path)?.expect("free");
    assert!(
        InstanceLock::acquire(&path)?.is_none(),
        "a second open handle conflicts, even in the same process"
    );
    assert_eq!(probe(&path)?, "busy");
    drop(first); // closing the file releases the lock
    assert!(path.try_exists()?);
    assert_eq!(probe(&path)?, "acquired");

    // the important property: a KILLED owner releases the lock, because the OS closes its descriptors
    let (mut owner, said) = spawn_locker(&path, true)?;
    assert_eq!(said, "acquired");
    assert!(InstanceLock::acquire(&path)?.is_none());
    owner.kill()?;
    owner.wait()?;
    let again = InstanceLock::acquire(&path)?.expect("released by the dead process");
    again._file.unlock()?; // explicit release while keeping the handle
    assert!(InstanceLock::acquire(&path)?.is_some());

    // blocking variant: lock() waits for the holder
    let holder = InstanceLock::acquire(&path)?.expect("free");
    let waiter_file = File::options().write(true).open(&path)?;
    let waiter = thread::spawn(move || waiter_file.lock());
    drop(holder);
    waiter.join().expect("no panic")?;

    // lease file: staleness is tested by editing the mtime, not by sleeping
    let lease = t.join("app.lease");
    let ttl = Duration::from_secs(30);
    let a = LeaseFile::acquire(&lease, SystemTime::now(), ttl)?.expect("free");
    assert!(LeaseFile::acquire(&lease, SystemTime::now(), ttl)?.is_none());
    std::mem::forget(a); // simulate a crash: Drop never runs and the file stays behind
    assert!(
        LeaseFile::acquire(&lease, SystemTime::now(), ttl)?.is_none(),
        "still looks alive"
    );
    set_mtime(&lease, 1_000)?;
    let b = LeaseFile::acquire(&lease, SystemTime::now(), ttl)?;
    assert!(b.is_some(), "stale lease taken over");
    drop(b);
    assert!(!lease.try_exists()?);
    println!("6. single-instance guards ok");
    Ok(())
}

fn main() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--lock-child") {
        let path = PathBuf::from(args.next().expect("path"));
        return child_main(&path, args.next().is_some_and(|a| a == "hold"));
    }
    s01_tempdir()?;
    s02_golden()?;
    s03_errors()?;
    s04_permissions()?;
    s05_watcher()?;
    s06_single_instance()?;
    println!("all fs testing sections passed");
    Ok(())
}
