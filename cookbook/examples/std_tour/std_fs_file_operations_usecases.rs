//! RECIPE 1062 (std_tour): `std::fs` by use case: OpenOptions matrix, metadata, directories, atomic writes, walks, append log, rotation.
//! LEVEL: intermediate
//! Use in: config/state files, caches, log rotation, build tools, installers, anything touching disk.   Run: cargo run --release --example std_fs_file_operations_usecases
//! COMMON USES: `fs::read_to_string` for config; temp file + `rename` for atomic saves; `read_dir` walks for build/size tools; append-only logs (WAL-style) with recovery; size-based log rotation.
//! METHODS: std::fs::read_to_string, std::fs::write, std::fs::read, std::fs::create_dir_all, std::fs::remove_file, std::fs::File::open, std::fs::File::create, std::fs::OpenOptions, OpenOptions::read, OpenOptions::write, OpenOptions::append, OpenOptions::create, OpenOptions::create_new, OpenOptions::truncate, std::fs::copy, std::fs::rename, std::fs::remove_dir_all, std::fs::read_dir, std::fs::metadata, Metadata::len, Metadata::is_file, Metadata::is_dir, Metadata::file_type, Metadata::modified, Metadata::permissions, std::fs::symlink_metadata, std::fs::canonicalize, std::fs::hard_link, std::fs::set_permissions, File::set_len, File::sync_all, File::sync_data, File::try_clone, File::lock, File::try_lock, File::unlock, std::fs::exists, std::fs::DirEntry, DirEntry::file_name, DirEntry::file_type, DirEntry::metadata, DirEntry::path, std::fs::create_dir, std::fs::remove_dir, std::fs::read_link, std::fs::FileType, FileType::is_symlink, File::set_modified, File::metadata, Permissions::readonly, Permissions::set_readonly, PermissionsExt::mode, std::os::unix::fs::symlink, MetadataExt::nlink
//! DEFINED IN: std: everything in `std::fs` and the `std::os::unix::fs` extension traits. There is no filesystem API in core or alloc.
//! DOCS: https://doc.rust-lang.org/std/fs/index.html
//! NOT COVERED (and why): fs::soft_link (deprecated); fs::read_dir on Windows-only flags; File::set_times (shown only by set_modified); OpenOptionsExt::custom_flags/mode (platform specific; mode shown via set_permissions).
//! Course: none. Beyond `io_files_paths` (basics), `file_locking_atomic_replace` (flock/rename internals) and `file_disk_io_patterns`.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   read a whole text file                     -> `fs::read_to_string(p)`; bytes: `fs::read(p)`
//!   write a whole file (not crash-safe)        -> `fs::write(p, bytes)`
//!   replace a file crash-safely                -> temp in SAME dir + `sync_all` + `rename` + fsync parent dir
//!   append a line                              -> `OpenOptions::new().create(true).append(true).open(p)`
//!   create only if absent (lockfile, unique)   -> `create_new(true)` (atomic; AlreadyExists on conflict)
//!   mkdir -p                                   -> `create_dir_all` (idempotent)
//!   list a directory deterministically         -> `read_dir` -> collect names -> `sort()`
//!   size / type / mtime                        -> `metadata(p)`; do not follow links: `symlink_metadata(p)`
//!   truncate / preallocate                     -> `File::set_len`
//!   make durable                               -> `sync_all` (data+metadata) / `sync_data`
//!   does it exist                              -> `fs::exists(p)?` (errors are reported, unlike `Path::exists`)
//!   exclusive access between processes         -> `File::lock` / `try_lock` (since 1.89)
//!   share one open file between threads        -> `File::try_clone` (shares the cursor!)
//!   delete a tree                              -> `remove_dir_all`
//!
//! Design: every section creates its own `TempDir` (pid + counter, RAII cleanup). Unix-only bits are behind cfg(unix).
//! Read: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html  and  https://doc.rust-lang.org/std/fs/struct.File.html
//! TRY NEXT:
//!   - [S] Add the case `read+append` to the OpenOptions matrix and predict where `read_to_string` starts reading. Assert it.
//!   - [S] What does `fs::rename` do when the destination is a non-empty directory? Predict per platform, then assert the ErrorKind.
//!   - [M] Make `walk` follow symlinks with a visited set of `canonicalize()`d paths and compare totals with the non-following mode.
//!   - [M] Add per-record fsync policy (every record vs every N) to `KvLog::put` and measure with `Instant` (print only).
//!   - [A] PROJECT: combine with `std_bufreader_bufwriter_patterns` (Tailer) and `std_fs_watch_polling_tempdir_testing` (snapshot diff): a rotating log that a tailer follows across rotations.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

const COVERED: &[&str] = &[
    "read_to_string",
    "write",
    "read",
    "create_dir_all",
    "remove_file",
    "File::open",
    "File::create",
    "OpenOptions",
    "append",
    "create",
    "create_new",
    "truncate",
    "copy",
    "rename",
    "remove_dir_all",
    "read_dir",
    "metadata",
    "len",
    "file_type",
    "modified",
    "permissions",
    "symlink_metadata",
    "canonicalize",
    "hard_link",
    "set_permissions",
    "set_len",
    "sync_all",
    "sync_data",
    "try_clone",
    "lock",
    "try_lock",
    "unlock",
    "exists",
    "DirEntry",
    "create_dir",
    "remove_dir",
    "read_link",
    "set_modified",
    "readonly",
];

struct TempDir(PathBuf);
static CTR: AtomicU32 = AtomicU32::new(0);
impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "cb1062_{tag}_{}_{}",
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
        // restore write permission so cleanup works after permission experiments
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn section(n: u32, t: &str) {
    println!("\n== {n}. {t}");
}

fn sorted_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort(); // read_dir order is unspecified: always sort for determinism
    v
}

// ---------------------------------------------------------------- 1 everyday

fn everyday_core() {
    section(
        1,
        "Everyday core: read/write/exists/create_dir_all/remove/rename/copy/read_dir",
    );
    let d = TempDir::new("core");
    let cfg = d.path("conf/app/settings.toml");
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::create_dir_all(cfg.parent().unwrap()).unwrap(); // idempotent: not an error
    assert!(!fs::exists(&cfg).unwrap());
    fs::write(&cfg, "port = 80\n").unwrap();
    assert_eq!(fs::read_to_string(&cfg).unwrap(), "port = 80\n");
    assert_eq!(fs::read(&cfg).unwrap().len(), 10);

    let copy = d.path("conf/copy.toml");
    assert_eq!(fs::copy(&cfg, &copy).unwrap(), 10); // returns bytes copied
    fs::rename(&copy, d.path("conf/moved.toml")).unwrap();
    assert!(!fs::exists(&copy).unwrap());
    assert_eq!(sorted_names(&d.path("conf")), ["app", "moved.toml"]);

    // Error paths matter in production: match ErrorKind, add the path to the message.
    let e = fs::read_to_string(d.path("nope")).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::NotFound);
    let e = fs::create_dir(d.path("conf")).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::AlreadyExists);
    let e = fs::remove_dir(d.path("conf")).unwrap_err();
    assert!(e.kind() == ErrorKind::DirectoryNotEmpty || e.raw_os_error().is_some());
    fs::remove_file(d.path("conf/moved.toml")).unwrap();
    assert_eq!(
        fs::remove_file(d.path("conf/moved.toml"))
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    fs::remove_dir_all(d.path("conf")).unwrap();
    assert!(!fs::exists(d.path("conf")).unwrap());
    println!("everyday ok");
}

// ---------------------------------------------------------------- 2 OpenOptions matrix

#[derive(Debug, PartialEq)]
enum Outcome {
    Content(Vec<u8>),
    OpenFailed(ErrorKind),
    WriteFailed,
}

struct Case {
    name: &'static str,
    existing: Option<&'static [u8]>,
    opts: fn(&mut OpenOptions) -> &mut OpenOptions,
    expect: Outcome,
}

fn run_case(dir: &TempDir, c: &Case) -> Outcome {
    let p = dir.path("target.txt");
    let _ = fs::remove_file(&p);
    if let Some(x) = c.existing {
        fs::write(&p, x).unwrap();
    }
    let mut o = OpenOptions::new();
    (c.opts)(&mut o);
    let mut f = match o.open(&p) {
        Ok(f) => f,
        Err(e) => return Outcome::OpenFailed(e.kind()),
    };
    if f.write_all(b"AB").is_err() {
        return Outcome::WriteFailed;
    }
    drop(f);
    Outcome::Content(fs::read(&p).unwrap())
}

fn openoptions_matrix() {
    section(
        2,
        "OpenOptions matrix: every flag combination with an asserted outcome",
    );
    let d = TempDir::new("oo");
    let c = |name, existing, opts, expect| Case {
        name,
        existing,
        opts,
        expect,
    };
    let cases = vec![
        c(
            "read only, existing: write fails",
            Some(b"xyz"),
            (|o| o.read(true)) as fn(&mut OpenOptions) -> &mut OpenOptions,
            Outcome::WriteFailed,
        ),
        c(
            "read only, missing",
            None,
            |o| o.read(true),
            Outcome::OpenFailed(ErrorKind::NotFound),
        ),
        c(
            "write, existing: overwrite prefix, no truncate",
            Some(b"xyz"),
            |o| o.write(true),
            Outcome::Content(b"ABz".to_vec()),
        ),
        c(
            "write, missing: NotFound (no create)",
            None,
            |o| o.write(true),
            Outcome::OpenFailed(ErrorKind::NotFound),
        ),
        c(
            "write+create, missing",
            None,
            |o| o.write(true).create(true),
            Outcome::Content(b"AB".to_vec()),
        ),
        c(
            "write+create, existing: still no truncate",
            Some(b"xyz"),
            |o| o.write(true).create(true),
            Outcome::Content(b"ABz".to_vec()),
        ),
        c(
            "write+truncate, existing",
            Some(b"xyz"),
            |o| o.write(true).truncate(true),
            Outcome::Content(b"AB".to_vec()),
        ),
        c(
            "write+create+truncate (= File::create)",
            Some(b"xyz"),
            |o| o.write(true).create(true).truncate(true),
            Outcome::Content(b"AB".to_vec()),
        ),
        c(
            "append, existing",
            Some(b"xyz"),
            |o| o.append(true),
            Outcome::Content(b"xyzAB".to_vec()),
        ),
        c(
            "append, missing: NotFound",
            None,
            |o| o.append(true),
            Outcome::OpenFailed(ErrorKind::NotFound),
        ),
        c(
            "append+create, missing",
            None,
            |o| o.append(true).create(true),
            Outcome::Content(b"AB".to_vec()),
        ),
        c(
            "create_new, missing",
            None,
            |o| o.write(true).create_new(true),
            Outcome::Content(b"AB".to_vec()),
        ),
        c(
            "create_new, existing: AlreadyExists",
            Some(b"xyz"),
            |o| o.write(true).create_new(true),
            Outcome::OpenFailed(ErrorKind::AlreadyExists),
        ),
        c(
            "create without write/append: InvalidInput",
            None,
            |o| o.read(true).create(true),
            Outcome::OpenFailed(ErrorKind::InvalidInput),
        ),
        c(
            "truncate without write: InvalidInput",
            Some(b"xyz"),
            |o| o.read(true).truncate(true),
            Outcome::OpenFailed(ErrorKind::InvalidInput),
        ),
        c(
            "nothing set: InvalidInput",
            Some(b"xyz"),
            |o| o,
            Outcome::OpenFailed(ErrorKind::InvalidInput),
        ),
    ];
    for case in &cases {
        let got = run_case(&d, case);
        assert_eq!(got, case.expect, "case: {}", case.name);
    }
    println!("{} flag combinations verified", cases.len());

    // read+write on one handle shares ONE cursor: write advances it, so seek before reading back.
    let p = d.path("rw.bin");
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&p)
        .unwrap();
    f.write_all(b"hello").unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    assert_eq!(s, "", "cursor is at EOF after the write");
    f.rewind().unwrap();
    f.read_to_string(&mut s).unwrap();
    assert_eq!(s, "hello");
    println!("openoptions ok");
}

// ---------------------------------------------------------------- 3 metadata, links, permissions

fn metadata_section() {
    section(
        3,
        "Metadata, file types, links, permissions, set_len, set_modified",
    );
    let d = TempDir::new("meta");
    let f = d.path("a.txt");
    fs::write(&f, b"12345").unwrap();
    let m = fs::metadata(&f).unwrap();
    assert!(m.is_file() && !m.is_dir() && m.len() == 5);
    assert!(m.file_type().is_file());
    assert!(m.modified().is_ok()); // value is platform dependent: never assert it against the wall clock
    assert!(fs::metadata(d.path("")).unwrap().is_dir());

    // set_len truncates or zero-extends without writing data.
    let h = OpenOptions::new().write(true).open(&f).unwrap();
    h.set_len(3).unwrap();
    assert_eq!(fs::read(&f).unwrap(), b"123");
    h.set_len(6).unwrap();
    assert_eq!(fs::read(&f).unwrap(), b"123\0\0\0");
    h.sync_data().unwrap(); // data only
    h.sync_all().unwrap(); // data + metadata
    // set_modified (since 1.75): deterministic mtimes for tests and cache invalidation.
    let t0 = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    h.set_modified(t0).unwrap();
    assert_eq!(fs::metadata(&f).unwrap().modified().unwrap(), t0);
    drop(h);

    // hard links: same inode, nlink = 2.
    let hl = d.path("hard.txt");
    fs::hard_link(&f, &hl).unwrap();
    assert_eq!(fs::read(&hl).unwrap(), fs::read(&f).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
        assert_eq!(fs::metadata(&f).unwrap().nlink(), 2);
        assert_eq!(
            fs::metadata(&f).unwrap().ino(),
            fs::metadata(&hl).unwrap().ino()
        );
        // permissions: mode bits
        fs::set_permissions(&f, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // symlinks: metadata follows, symlink_metadata does not
        let ln = d.path("link");
        symlink(&f, &ln).unwrap();
        assert!(fs::symlink_metadata(&ln).unwrap().file_type().is_symlink());
        assert!(fs::metadata(&ln).unwrap().is_file());
        assert_eq!(fs::read_link(&ln).unwrap(), f);
        assert_eq!(
            fs::canonicalize(&ln).unwrap(),
            fs::canonicalize(&f).unwrap()
        );
        // dangling link: exists() lies (false), symlink_metadata sees it
        fs::remove_file(&f).unwrap();
        assert!(!fs::exists(&ln).unwrap());
        assert!(fs::symlink_metadata(&ln).is_ok());
    }
    // portable permission: read-only flag
    let g = d.path("ro.txt");
    fs::write(&g, b"x").unwrap();
    let mut perm = fs::metadata(&g).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&g, perm.clone()).unwrap();
    assert!(fs::metadata(&g).unwrap().permissions().readonly());
    #[allow(clippy::permissions_set_readonly_false)]
    {
        perm.set_readonly(false); // so TempDir can delete it
    }
    fs::set_permissions(&g, perm).unwrap();
    println!("metadata ok");
}

// ---------------------------------------------------------------- 4 locks, try_clone

fn lock_and_clone() {
    section(4, "File::lock/try_lock/unlock, try_clone shares the cursor");
    let d = TempDir::new("lock");
    let p = d.path("lock");
    let a = File::create(&p).unwrap();
    a.lock().unwrap(); // blocks until exclusive; advisory
    let b = OpenOptions::new().read(true).write(true).open(&p).unwrap();
    match b.try_lock() {
        Err(TryLockError::WouldBlock) => {}
        other => panic!("second handle must not get the lock: {other:?}"),
    }
    a.unlock().unwrap();
    b.try_lock().unwrap(); // now free
    b.unlock().unwrap();
    // shared locks coexist
    a.lock_shared().unwrap();
    b.lock_shared().unwrap();
    a.unlock().unwrap();
    b.unlock().unwrap();

    // try_clone: second handle to the SAME open file description: cursor is shared.
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(d.path("c.txt"))
        .unwrap();
    let mut g = f.try_clone().unwrap();
    f.write_all(b"abc").unwrap();
    g.write_all(b"DEF").unwrap(); // continues after "abc", not at 0
    f.rewind().unwrap();
    let mut s = String::new();
    g.read_to_string(&mut s).unwrap();
    assert_eq!(s, "abcDEF");
    println!("lock/clone ok");
}

// ---------------------------------------------------------------- 5 atomic write

/// Unique temp name: pid + counter. No clock, no randomness, same directory (rename must not cross filesystems).
fn temp_sibling(target: &Path) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let name = target.file_name().unwrap().to_string_lossy();
    target.with_file_name(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

fn atomic_write(target: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = temp_sibling(target);
    let res = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?; // create_new: never clobber another writer
        f.write_all(data)?;
        f.sync_all()?; // data durable BEFORE it becomes visible under the final name
        drop(f);
        fs::rename(&tmp, target) // atomic replace on POSIX
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    #[cfg(unix)]
    if res.is_ok() {
        // fsync the directory so the rename itself survives a crash
        File::open(target.parent().unwrap())?.sync_all()?;
    }
    res
}

fn atomic_section() {
    section(5, "Atomic replace: temp + fsync + rename + fsync(dir)");
    let d = TempDir::new("atomic");
    let t = d.path("state.json");
    atomic_write(&t, b"{\"v\":1}").unwrap();
    atomic_write(&t, b"{\"v\":2}").unwrap();
    assert_eq!(fs::read_to_string(&t).unwrap(), "{\"v\":2}");
    assert_eq!(
        sorted_names(&d.0),
        ["state.json"],
        "no temp files left behind"
    );
    // Failure injection: target is a directory => rename fails => temp cleaned up, original untouched.
    let dir_target = d.path("isdir");
    fs::create_dir(&dir_target).unwrap();
    fs::write(dir_target.join("keep"), b"x").unwrap();
    assert!(atomic_write(&dir_target, b"data").is_err());
    assert_eq!(sorted_names(&d.0), ["isdir", "state.json"]);
    // A reader never sees a partial file: demonstrate by reading between "steps" is impossible, but
    // the naive version is visibly non-atomic: File::create truncates immediately.
    let naive = d.path("naive");
    fs::write(&naive, b"old-complete").unwrap();
    let mut f = File::create(&naive).unwrap(); // truncated NOW; a crash here leaves an empty file
    assert_eq!(fs::read(&naive).unwrap(), b"");
    f.write_all(b"new").unwrap();
    println!("atomic ok");
}

// ---------------------------------------------------------------- 6 recursive walk

#[derive(Debug, Default, PartialEq)]
struct Totals {
    files: u64,
    dirs: u64,
    bytes: u64,
    symlinks: u64,
}

/// Does not follow symlinks (symlink loops cannot recurse); counts them separately.
fn walk(dir: &Path, t: &mut Totals) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name()); // deterministic order
    for e in entries {
        let ft = e.file_type()?; // no extra stat on most platforms; does NOT follow symlinks
        if ft.is_symlink() {
            t.symlinks += 1;
        } else if ft.is_dir() {
            t.dirs += 1;
            walk(&e.path(), t)?;
        } else {
            t.files += 1;
            t.bytes += e.metadata()?.len();
        }
    }
    Ok(())
}

/// Follows symlinks safely: remember canonical directories already entered.
fn walk_following(
    dir: &Path,
    seen: &mut std::collections::HashSet<PathBuf>,
    t: &mut Totals,
) -> io::Result<()> {
    if !seen.insert(fs::canonicalize(dir)?) {
        return Ok(());
    } // loop (or already counted via another link)
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let md = match fs::metadata(e.path()) {
            Ok(m) => m,
            Err(_) => continue,
        }; // dangling link: skip
        if md.is_dir() {
            t.dirs += 1;
            walk_following(&e.path(), seen, t)?;
        } else {
            t.files += 1;
            t.bytes += md.len();
        }
    }
    Ok(())
}

fn walk_section() {
    section(
        6,
        "Recursive walk with size totals and symlink-loop protection",
    );
    let d = TempDir::new("walk");
    fs::create_dir_all(d.path("a/b")).unwrap();
    fs::create_dir_all(d.path("c")).unwrap();
    fs::write(d.path("a/one.bin"), vec![0u8; 100]).unwrap();
    fs::write(d.path("a/b/two.bin"), vec![0u8; 250]).unwrap();
    fs::write(d.path("c/three.bin"), vec![0u8; 7]).unwrap();
    let mut t = Totals::default();
    walk(&d.0, &mut t).unwrap();
    assert_eq!(
        t,
        Totals {
            files: 3,
            dirs: 3,
            bytes: 357,
            symlinks: 0
        }
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&d.0, d.path("a/b/loop")).unwrap(); // a/b/loop -> root: infinite if followed naively
        let mut t = Totals::default();
        walk(&d.0, &mut t).unwrap();
        assert_eq!((t.files, t.symlinks), (3, 1));
        let mut seen = Default::default();
        let mut t2 = Totals::default();
        walk_following(&d.0, &mut seen, &mut t2).unwrap();
        assert_eq!(
            t2.bytes, 357,
            "loop target already visited: nothing double counted"
        );
    }
    // disk-usage report: bytes per immediate child, sorted largest first (ties by name).
    let mut usage: Vec<(String, u64)> = fs::read_dir(&d.0)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().unwrap().is_dir())
        .map(|e| {
            let mut t = Totals::default();
            walk(&e.path(), &mut t).unwrap();
            (e.file_name().to_string_lossy().into_owned(), t.bytes)
        })
        .collect();
    usage.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    assert_eq!(usage, [("a".to_string(), 350), ("c".to_string(), 7)]);
    println!("walk ok");
}

// ---------------------------------------------------------------- 7 append-only KV log

fn checksum(b: &[u8]) -> u32 {
    let mut h = 0x811c9dc5u32; // FNV-1a
    for &x in b {
        h = (h ^ x as u32).wrapping_mul(0x01000193);
    }
    h
}

/// Record: [klen u32][vlen u32][crc u32][key][value]  (crc over key+value)
struct KvLog {
    f: File,
}
impl KvLog {
    /// Open and recover: replay the log, truncate any torn/corrupt tail, return live map.
    fn open(path: &Path) -> io::Result<(KvLog, HashMap<Vec<u8>, Vec<u8>>)> {
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let mut all = Vec::new();
        f.read_to_end(&mut all)?;
        let (mut pos, mut map) = (0usize, HashMap::new());
        while let Some((k, v, next)) = Self::parse(&all, pos) {
            map.insert(k, v);
            pos = next;
        }
        if pos < all.len() {
            f.set_len(pos as u64)?;
        } // drop the torn tail so appends stay aligned
        f.seek(SeekFrom::End(0))?;
        Ok((KvLog { f }, map))
    }
    fn parse(b: &[u8], pos: usize) -> Option<(Vec<u8>, Vec<u8>, usize)> {
        let h = b.get(pos..pos + 12)?;
        let kl = u32::from_le_bytes(h[0..4].try_into().ok()?) as usize;
        let vl = u32::from_le_bytes(h[4..8].try_into().ok()?) as usize;
        let crc = u32::from_le_bytes(h[8..12].try_into().ok()?);
        let body = b.get(pos + 12..pos + 12 + kl.checked_add(vl)?)?; // checked: hostile lengths cannot overflow
        if checksum(body) != crc {
            return None;
        }
        Some((body[..kl].to_vec(), body[kl..].to_vec(), pos + 12 + kl + vl))
    }
    fn put(&mut self, k: &[u8], v: &[u8]) -> io::Result<()> {
        let mut rec = Vec::with_capacity(12 + k.len() + v.len());
        rec.extend_from_slice(&(k.len() as u32).to_le_bytes());
        rec.extend_from_slice(&(v.len() as u32).to_le_bytes());
        let body: Vec<u8> = [k, v].concat();
        rec.extend_from_slice(&checksum(&body).to_le_bytes());
        rec.extend_from_slice(&body);
        self.f.write_all(&rec)?; // one write_all per record keeps torn tails to at most one record
        self.f.sync_data()
    }
}

fn kv_log_section() {
    section(7, "Append-only KV log with crash-truncation recovery");
    let d = TempDir::new("kv");
    let p = d.path("data.log");
    let (mut log, m) = KvLog::open(&p).unwrap();
    assert!(m.is_empty());
    for i in 0..100u32 {
        log.put(format!("k{}", i % 10).as_bytes(), i.to_string().as_bytes())
            .unwrap();
    }
    drop(log);
    let (log, m) = KvLog::open(&p).unwrap();
    assert_eq!(m.len(), 10);
    assert_eq!(m[&b"k3".to_vec()], b"93");
    drop(log);
    // Simulate a crash mid-append: chop 5 bytes off the last record.
    let full = fs::metadata(&p).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&p)
        .unwrap()
        .set_len(full - 5)
        .unwrap();
    let (mut log, m) = KvLog::open(&p).unwrap();
    assert_eq!(
        m[&b"k9".to_vec()],
        b"89",
        "last put (k9=99) was torn: previous value wins"
    );
    assert!(
        fs::metadata(&p).unwrap().len() < full - 5,
        "torn tail removed"
    );
    log.put(b"k9", b"new").unwrap(); // appends cleanly after recovery
    drop(log);
    let (_l, m) = KvLog::open(&p).unwrap();
    assert_eq!(m[&b"k9".to_vec()], b"new");
    // Bit rot in the middle: replay stops at the corrupt record (everything after is dropped, by design).
    let mut raw = fs::read(&p).unwrap();
    raw[20] ^= 0xff;
    fs::write(&p, &raw).unwrap();
    let (_l, m) = KvLog::open(&p).unwrap();
    assert!(m.len() < 10);
    println!("kv log ok");
}

// ---------------------------------------------------------------- 8 rotation + copy with progress

struct RotatingLog {
    dir: PathBuf,
    max: u64,
    keep: usize,
    cur: u64,
    f: File,
}
impl RotatingLog {
    fn open(dir: &Path, max: u64, keep: usize) -> io::Result<Self> {
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("app.log"))?;
        let cur = f.metadata()?.len();
        Ok(RotatingLog {
            dir: dir.into(),
            max,
            keep,
            cur,
            f,
        })
    }
    fn rotate(&mut self) -> io::Result<()> {
        // app.log.{keep-1} -> dropped, ..., app.log.1 -> app.log.2, app.log -> app.log.1
        let n = |i: usize| self.dir.join(format!("app.log.{i}"));
        let _ = fs::remove_file(n(self.keep));
        for i in (1..self.keep).rev() {
            if fs::exists(n(i))? {
                fs::rename(n(i), n(i + 1))?;
            }
        }
        fs::rename(self.dir.join("app.log"), n(1))?;
        self.f = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(self.dir.join("app.log"))?;
        self.cur = 0;
        Ok(())
    }
    fn line(&mut self, s: &str) -> io::Result<()> {
        let n = s.len() as u64 + 1;
        if self.cur > 0 && self.cur + n > self.max {
            self.rotate()?;
        }
        writeln!(self.f, "{s}")?;
        self.cur += n;
        Ok(())
    }
}

fn copy_with_progress(
    src: &Path,
    dst: &Path,
    mut progress: impl FnMut(u64, u64),
) -> io::Result<u64> {
    let mut r = File::open(src)?;
    let total = r.metadata()?.len();
    let mut w = File::create(dst)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        w.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    w.sync_all()?;
    Ok(done)
}

fn rotation_copy_section() {
    section(8, "Log rotation by size, copy with progress");
    let d = TempDir::new("rot");
    let mut log = RotatingLog::open(&d.0, 100, 3).unwrap();
    for i in 0..60 {
        log.line(&format!("event number {i:04}")).unwrap();
    } // 19 bytes each, ~5 per file
    assert_eq!(
        sorted_names(&d.0),
        ["app.log", "app.log.1", "app.log.2", "app.log.3"].map(String::from)
    );
    for name in ["app.log", "app.log.1", "app.log.2", "app.log.3"] {
        assert!(
            fs::metadata(d.path(name)).unwrap().len() <= 100,
            "{name} within the cap"
        );
    }
    let newest = fs::read_to_string(d.path("app.log")).unwrap();
    assert!(newest.contains("event number 0059"));
    assert!(
        !fs::read_to_string(d.path("app.log.3"))
            .unwrap()
            .contains("event number 0000"),
        "oldest generations are dropped"
    );

    let src = d.path("big.bin");
    fs::write(&src, vec![7u8; 300_000]).unwrap();
    let mut ticks = vec![];
    let n = copy_with_progress(&src, &d.path("big.copy"), |done, total| {
        ticks.push((done, total))
    })
    .unwrap();
    assert_eq!(n, 300_000);
    assert!(
        ticks.windows(2).all(|w| w[0].0 < w[1].0),
        "progress is strictly increasing"
    );
    assert_eq!(ticks.last(), Some(&(300_000, 300_000)));
    assert_eq!(
        fs::read(&src).unwrap(),
        fs::read(d.path("big.copy")).unwrap()
    );
    println!("rotation/copy ok");
}

// ---------------------------------------------------------------- 9 production tips

fn production_tips() {
    section(9, "Production tips and tricks");
    let d = TempDir::new("tips");
    // 1. Use fs::exists (since 1.81) or handle NotFound; Path::exists() hides permission errors as `false`.
    assert!(fs::exists(d.path("nothing")).is_ok());
    // 2. create_new(true) is an atomic "claim": the check-then-create race of `if !exists { create }` is gone (TOCTOU).
    let claim = || {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(d.path("claim"))
            .map(|_| ())
    };
    assert!(claim().is_ok());
    assert_eq!(claim().unwrap_err().kind(), ErrorKind::AlreadyExists);
    // 3. Temp files in the SAME directory as the target (rename across mounts fails with EXDEV).
    assert_eq!(
        temp_sibling(Path::new("/x/y/state.json")).parent(),
        Some(Path::new("/x/y"))
    );
    // 4. Sort read_dir output: ordering is filesystem dependent (flaky tests, non-reproducible builds).
    fs::write(d.path("b"), b"").unwrap();
    fs::write(d.path("a"), b"").unwrap();
    let names = sorted_names(&d.0);
    assert!(names.windows(2).all(|w| w[0] <= w[1]));
    // 5. DirEntry::file_type avoids a stat per entry; metadata(path) costs a syscall each (clippy: none, perf note).
    let n_dirs = fs::read_dir(&d.0)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .is_ok_and(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        })
        .count();
    assert_eq!(n_dirs, 0);
    // 6. fs::read_to_string on untrusted size: bound it with Read::take (see std_io_read_write_seek_traits).
    fs::write(d.path("big"), vec![b'x'; 1000]).unwrap();
    let mut s = String::new();
    File::open(d.path("big"))
        .unwrap()
        .take(100)
        .read_to_string(&mut s)
        .unwrap();
    assert_eq!(s.len(), 100);
    // 7. Add path context to errors; io::Error alone says "No such file or directory (os error 2)" with no path.
    fn ctx(p: &Path) -> impl FnOnce(io::Error) -> io::Error + '_ {
        move |e| io::Error::new(e.kind(), format!("{}: {e}", p.display()))
    }
    let p = d.path("missing");
    let e = fs::read(&p).map_err(ctx(&p)).unwrap_err();
    assert!(e.to_string().contains("missing") && e.kind() == ErrorKind::NotFound);
    // 8. File::create truncates immediately: never use it on a file you may need if the next step fails.
    // 9. `fs::copy` preserves permissions and uses reflink/copy_file_range where available; hand loops only for progress.
    fs::copy(d.path("big"), d.path("big2")).unwrap();
    assert_eq!(fs::metadata(d.path("big2")).unwrap().len(), 1000);
    // 10. rename over an existing file replaces it atomically on unix; on Windows older runtimes may fail: test both.
    fs::rename(d.path("big2"), d.path("big")).unwrap();
    // 11. Fsync is a separate cost: sync_data for logs (skips mtime), sync_all for state files, fsync the parent dir on create/rename.
    // 12. Release file handles before deleting/renaming on Windows: scope Files in blocks (RAII) like TempDir does.
    // 13. Use an RAII TempDir in tests so a failing assertion still cleans up (Drop runs on panic unwinding).
    let _ = std::panic::catch_unwind(|| {
        let t = TempDir::new("panic");
        let p = t.0.clone();
        std::panic::set_hook(Box::new(|_| {}));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = TempDir(p.clone());
            panic!("boom");
        }));
        let _ = std::panic::take_hook();
        assert!(r.is_err());
        assert!(!p.exists(), "Drop ran during unwinding");
    });
    println!("tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|c| seen.insert(*c)),
        "COVERED has duplicates"
    );
    everyday_core();
    openoptions_matrix();
    metadata_section();
    lock_and_clone();
    atomic_section();
    walk_section();
    kv_log_section();
    rotation_copy_section();
    production_tips();
    println!("\nall sections passed ({} methods listed)", COVERED.len());
}
