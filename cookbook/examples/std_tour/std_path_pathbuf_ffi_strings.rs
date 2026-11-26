//! RECIPE 1063 (std_tour): `Path`, `PathBuf`, `OsStr`, `OsString`, `CString`, `CStr`: paths, safe joins, platform strings, FFI strings.
//! LEVEL: intermediate
//! Use in: CLIs, servers that serve files (path traversal!), build tools, FFI wrappers, anything crossing OS or C boundaries.   Run: cargo run --release --example std_path_pathbuf_ffi_strings
//! COMMON USES: `base.join(user_input)` safely; extension/stem manipulation for output names; `OsString` for argv/env that may not be UTF-8; `CString::new` before calling C APIs (`open`, `getenv`).
//! METHODS: Path::new, PathBuf::from, Path::join, PathBuf::push, PathBuf::pop, Path::file_name, Path::extension, Path::parent, Path::with_extension, Path::display, Path::to_str, Path::to_string_lossy, Path::file_stem, Path::file_prefix, PathBuf::set_extension, PathBuf::set_file_name, Path::with_file_name, Path::ancestors, Path::components, Path::iter, Path::strip_prefix, Path::starts_with, Path::ends_with, Path::is_absolute, Path::is_relative, Path::has_root, Path::canonicalize, Path::as_os_str, PathBuf::into_os_string, std::path::Component, std::path::MAIN_SEPARATOR, std::path::is_separator, OsStr::new, OsStr::to_str, OsStr::to_string_lossy, OsString::into_string, OsString::push, OsStr::len, OsStrExt::as_bytes, OsStringExt::from_vec, OsStrExt::from_bytes, CString::new, NulError::nul_position, CString::as_bytes_with_nul, CString::as_ptr, CString::into_string, CString::into_bytes, CStr::from_bytes_with_nul, CStr::from_bytes_until_nul, CStr::to_str, CStr::to_string_lossy, CStr::to_bytes, CStr::to_owned, c_char, c_int, CString::into_raw, CString::from_raw, std::path::StripPrefixError, PathBuf::extend, PathBuf::as_path, Path::to_path_buf, Path::exists
//! DEFINED IN: std: `Path`, `PathBuf`, `Component` (`std::path`), `OsStr`, `OsString` (`std::ffi`), `std::os::unix::ffi`.
//!   core: `CStr`, `c_char` (`core::ffi`). alloc: `CString` (`alloc::ffi`). `std::ffi` re-exports all of them.
//! DOCS: https://doc.rust-lang.org/std/path/index.html
//! NOT COVERED (and why): Path::normalize_lexically (unstable in 1.98.1; we hand-roll normalisation, which is the interesting part); Path::as_mut_os_str; Windows-only `PrefixComponent` internals (described and cfg(windows)-asserted, not executable here).
//! Course: none.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   build a path                                -> `base.join("a").join("b")`, or `PathBuf::from_iter([..])`
//!   append in place / go up                     -> `push` / `pop`
//!   file name / extension / name sans ext       -> `file_name()`, `extension()`, `file_stem()` (all return Option)
//!   change extension                            -> `with_extension("md")` (new) / `set_extension` (in place)
//!   show to a user / log                        -> `path.display()` (lossy, never `{:?}` for UX)
//!   need a &str                                 -> `to_str()` (None if not UTF-8) or `to_string_lossy()` (Cow)
//!   is X inside Y                               -> `starts_with` (component-wise!), then `strip_prefix` for the rest
//!   iterate parents                             -> `ancestors()`
//!   untrusted relative path under a base        -> reject absolute + `..`, normalise lexically, then canonicalize check
//!   resolve symlinks / `..` on disk             -> `canonicalize` (path must exist)
//!   non-UTF-8 bytes (unix)                      -> `OsStrExt::as_bytes`, `OsStringExt::from_vec`
//!   pass a string to C                          -> `CString::new(s)?` then `.as_ptr()` (keep the CString alive)
//!   read a C string                             -> `CStr::from_ptr` (unsafe) or `CStr::from_bytes_until_nul`
//!   C literal                                   -> `c"hello"` (since 1.77)
//!
//! Design: pure path logic never touches the disk, except the symlink-escape check. Windows behaviour is described in comments
//! and asserted under `#[cfg(windows)]`.
//! Read: https://doc.rust-lang.org/std/path/struct.Path.html  and  https://doc.rust-lang.org/std/ffi/struct.CString.html
//! TRY NEXT:
//!   - [S] Add `"a/b/../../../x"` and `"./../x"` to the attack list in `safe_join` and predict which branch rejects each.
//!   - [S] What does `Path::new("a//b/./c/").components().count()` return? Predict, then assert.
//!   - [M] Add a `Cow<Path>`-returning variant of `normalize` that avoids allocation when the input is already normal (see std_cow_borrow_toowned).
//!   - [M] Make `safe_join` also reject Windows reserved names (`CON`, `NUL`) and trailing dots behind `cfg(windows)`.
//!   - [A] PROJECT: combine with `std_fs_file_operations_usecases` (walk) and `std_net_tcp_udp_ip_addr`: a loopback static-file server that serves a directory using `safe_join` and rejects traversal with 403.
#![allow(dead_code)]

use std::ffi::{CStr, CString, OsStr, OsString, c_char, c_int};
use std::fs;
use std::path::{Component, Path, PathBuf};

const COVERED: &[&str] = &[
    "Path::new",
    "PathBuf::from",
    "join",
    "push",
    "pop",
    "file_name",
    "extension",
    "parent",
    "with_extension",
    "display",
    "to_str",
    "to_string_lossy",
    "file_stem",
    "file_prefix",
    "set_extension",
    "set_file_name",
    "with_file_name",
    "ancestors",
    "components",
    "iter",
    "strip_prefix",
    "starts_with",
    "ends_with",
    "is_absolute",
    "is_relative",
    "has_root",
    "canonicalize",
    "as_os_str",
    "into_os_string",
    "Component",
    "MAIN_SEPARATOR",
    "is_separator",
    "OsStr::new",
    "OsString::into_string",
    "OsString::push",
    "OsStrExt",
    "OsStringExt",
    "CString::new",
    "nul_position",
    "as_bytes_with_nul",
    "as_ptr",
    "CString::into_string",
    "from_bytes_with_nul",
    "from_bytes_until_nul",
    "CStr::to_str",
    "CStr::to_string_lossy",
    "CStr::to_bytes",
    "c_char",
    "c_int",
    "into_raw",
    "from_raw",
    "StripPrefixError",
    "extend",
    "as_path",
    "to_path_buf",
    "exists",
    "OsStr::len",
];

fn section(n: u32, t: &str) {
    println!("\n== {n}. {t}");
}

// ---------------------------------------------------------------- 1 everyday

fn everyday_core() {
    section(
        1,
        "Everyday core: join, file_name, extension, parent, with_extension, display",
    );
    let base = Path::new("/var/app/data");
    let report = base.join("reports").join("q3.csv");
    assert_eq!(report, Path::new("/var/app/data/reports/q3.csv"));
    assert_eq!(report.file_name(), Some(OsStr::new("q3.csv")));
    assert_eq!(report.extension().and_then(OsStr::to_str), Some("csv"));
    assert_eq!(report.file_stem(), Some(OsStr::new("q3")));
    // 1.91: file_stem cuts at the LAST dot, file_prefix at the first one (a leading dot does not count)
    for (name, stem, prefix) in [
        ("pkg.tar.gz", "pkg.tar", "pkg"),
        (".config.toml", ".config", ".config"),
        (".bashrc", ".bashrc", ".bashrc"),
        ("plain", "plain", "plain"),
    ] {
        let p = Path::new(name);
        assert_eq!(
            (p.file_stem(), p.file_prefix()),
            (Some(OsStr::new(stem)), Some(OsStr::new(prefix))),
            "{name}"
        );
    }
    assert_eq!(report.parent(), Some(Path::new("/var/app/data/reports")));
    assert_eq!(
        report.with_extension("parquet"),
        Path::new("/var/app/data/reports/q3.parquet")
    );
    assert_eq!(report.display().to_string(), "/var/app/data/reports/q3.csv");
    assert!(report.is_absolute() && !report.is_relative() && report.has_root());

    // PathBuf mutates in place; pop reports whether anything was removed.
    let mut p = PathBuf::from("/a/b");
    p.push("c");
    p.set_extension("txt");
    assert_eq!(p, Path::new("/a/b/c.txt"));
    p.set_file_name("d.rs");
    assert!(p.pop());
    assert_eq!(p, Path::new("/a/b"));
    let mut root = PathBuf::from("/");
    assert!(!root.pop(), "cannot go above root");
    let p = PathBuf::from_iter(["x", "y", "z.tar.gz"]);
    assert_eq!(p, Path::new("x/y/z.tar.gz"));
    assert_eq!(p.extension().unwrap(), "gz"); // only the LAST extension
    assert_eq!(p.with_extension(""), Path::new("x/y/z.tar"));
    assert_eq!(p.with_file_name("other"), Path::new("x/y/other"));
    // Option-returning accessors are Option for a reason:
    assert_eq!(Path::new("/").file_name(), None);
    assert_eq!(Path::new("/").parent(), None);
    assert_eq!(
        Path::new("a").parent(),
        Some(Path::new("")),
        "empty path, not None: a classic surprise"
    );
    assert_eq!(Path::new(".bashrc").extension(), None);
    assert_eq!(
        Path::new(".bashrc").file_stem(),
        Some(OsStr::new(".bashrc"))
    );
    assert_eq!(Path::new("a.").extension(), Some(OsStr::new("")));
    assert_eq!(
        Path::new("dir/").file_name(),
        Some(OsStr::new("dir")),
        "trailing slash ignored"
    );
    println!("everyday ok");
}

// ---------------------------------------------------------------- 2 join pitfalls

fn join_pitfalls() {
    section(2, "join/push with an absolute argument REPLACES the path");
    assert_eq!(
        Path::new("/srv/www").join("/etc/passwd"),
        Path::new("/etc/passwd")
    ); // the path-traversal bug in one line
    let mut p = PathBuf::from("/srv/www");
    p.push("/etc");
    assert_eq!(p, Path::new("/etc"));
    // The fix when the second part is user input: strip the root first, or reject (see safe_join below).
    let user = "/etc/passwd";
    let fixed = Path::new("/srv/www").join(user.trim_start_matches('/'));
    assert_eq!(fixed, Path::new("/srv/www/etc/passwd"));
    // Joining "" adds a trailing separator but is the same path for comparison.
    assert_eq!(Path::new("a").join(""), Path::new("a"));
    assert_eq!(Path::new("a").join("").to_str(), Some("a/"));

    // Windows: `C:foo` is drive-RELATIVE, `\\server\share` is UNC, and `C:\x` join replaces everything.
    #[cfg(windows)]
    {
        assert_eq!(Path::new(r"C:\a").join(r"D:\b"), Path::new(r"D:\b"));
        assert!(Path::new(r"C:\a").has_root());
        assert!(!Path::new(r"C:a").has_root());
        assert!(matches!(
            Path::new(r"C:\a").components().next(),
            Some(Component::Prefix(_))
        ));
    }
    #[cfg(unix)]
    {
        assert_eq!(std::path::MAIN_SEPARATOR, '/');
        assert!(std::path::is_separator('/') && !std::path::is_separator('\\'));
        assert!(
            Path::new(r"a\b").file_name().unwrap() == r"a\b",
            "backslash is an ordinary char on unix"
        );
    }
    println!("join ok");
}

// ---------------------------------------------------------------- 3 components & comparison

fn components_section() {
    section(
        3,
        "components, iter, ancestors, strip_prefix, starts_with, ends_with",
    );
    let p = Path::new("/usr/./lib//x/../y/");
    let comps: Vec<Component> = p.components().collect();
    assert_eq!(comps[0], Component::RootDir);
    assert_eq!(comps[1], Component::Normal(OsStr::new("usr")));
    assert_eq!(
        comps.len(),
        6,
        "a '.' in the MIDDLE and duplicate slashes vanish; '..' is kept"
    );
    assert!(comps.contains(&Component::ParentDir));
    assert_eq!(
        Path::new("./a").components().next(),
        Some(Component::CurDir),
        "a LEADING '.' is kept"
    );
    assert_eq!(
        Path::new("a/b/c").iter().collect::<Vec<_>>(),
        ["a", "b", "c"]
    ); // iter() yields &OsStr

    let anc: Vec<&Path> = Path::new("/a/b/c").ancestors().collect();
    assert_eq!(
        anc,
        [
            Path::new("/a/b/c"),
            Path::new("/a/b"),
            Path::new("/a"),
            Path::new("/")
        ]
    );
    // find the project root: nearest ancestor that satisfies a predicate
    let root = Path::new("/w/proj/src/deep")
        .ancestors()
        .find(|a| a.ends_with("proj"));
    assert_eq!(root, Some(Path::new("/w/proj")));

    // starts_with/ends_with compare COMPONENTS, not characters:
    assert!(!Path::new("/data/abc").starts_with("/data/ab"));
    assert!(
        "/data/abc".starts_with("/data/ab"),
        "…while str::starts_with is the buggy substring test"
    );
    assert!(Path::new("/data/ab/c").starts_with("/data/ab"));
    assert!(Path::new("x/y.txt").ends_with("y.txt"));
    assert!(!Path::new("x/ay.txt").ends_with("y.txt"));
    assert_eq!(
        Path::new("/a/b/c").strip_prefix("/a").unwrap(),
        Path::new("b/c")
    );
    let not_under: Result<&Path, std::path::StripPrefixError> =
        Path::new("/a/b").strip_prefix("/x");
    assert!(not_under.is_err());
    // owned <-> borrowed: to_path_buf copies, as_path borrows, and a PathBuf can be extended component by component
    let mut owned: PathBuf = Path::new("/a").to_path_buf();
    owned.extend(["b", "c.txt"]);
    assert_eq!(owned.as_path(), Path::new("/a/b/c.txt"));
    // equality is by components too
    assert_eq!(Path::new("a//b/./c"), Path::new("a/b/c"));
    assert_ne!(Path::new("./a"), Path::new("a"));
    assert_ne!(
        Path::new("a/../b"),
        Path::new("b"),
        "`..` is never resolved without the disk (symlinks!)"
    );
    println!("components ok");
}

// ---------------------------------------------------------------- 4 normalisation + traversal

/// Resolve `.` and `..` lexically. Returns None if `..` climbs above the start of a relative path
/// or above the root, i.e. the input tries to escape.
fn normalize(p: &Path) -> Option<PathBuf> {
    let mut out: Vec<Component> = vec![];
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                _ => return None, // above root or above the relative start
            },
            other => out.push(other),
        }
    }
    Some(out.iter().collect())
}

#[derive(Debug, PartialEq)]
enum Reject {
    Absolute,
    Escapes,
    Nul,
    Empty,
}

/// Join untrusted `rel` under `base`. Pure lexical checks first; callers that touch the disk must
/// ALSO run `confined` (symlinks).
fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, Reject> {
    if rel.is_empty() {
        return Err(Reject::Empty);
    }
    if rel.contains('\0') {
        return Err(Reject::Nul);
    } // the OS would truncate or reject
    let p = Path::new(rel);
    // is_absolute() is False for "C:foo" or "\foo" on Windows: check components, not just is_absolute.
    if p.components()
        .any(|c| matches!(c, Component::RootDir | Component::Prefix(_)))
    {
        return Err(Reject::Absolute);
    }
    let n = normalize(p).ok_or(Reject::Escapes)?;
    Ok(base.join(n))
}

fn traversal_section() {
    section(4, "Lexical normalisation and path-traversal checks");
    let n = |s: &str| normalize(Path::new(s));
    assert_eq!(n("a/./b/../c"), Some(PathBuf::from("a/c")));
    assert_eq!(n("a/b/../../c"), Some(PathBuf::from("c")));
    assert_eq!(n("../x"), None);
    assert_eq!(n("a/../../x"), None);
    assert_eq!(n("/a/../.."), None, "cannot go above root");
    assert_eq!(n("/a/b/.."), Some(PathBuf::from("/a")));

    let base = Path::new("/srv/files");
    let attacks = [
        "../../etc/passwd",
        "/etc/passwd",
        "a/../../b",
        "..",
        "a/b/../../../c",
        "./../x",
    ];
    for a in attacks {
        assert!(safe_join(base, a).is_err(), "must reject {a:?}");
    }
    assert_eq!(
        safe_join(base, "docs/./a/../b.txt"),
        Ok(PathBuf::from("/srv/files/docs/b.txt"))
    );
    assert_eq!(safe_join(base, "x\0y"), Err(Reject::Nul));
    assert_eq!(safe_join(base, ""), Err(Reject::Empty));
    assert_eq!(safe_join(base, "/etc"), Err(Reject::Absolute));
    // URL-encoded input must be decoded BEFORE the check, never after: "..%2f" is a plain name here.
    assert!(
        safe_join(base, "..%2fetc").is_ok(),
        "a literal name; decode first if your input is a URL path"
    );
    // Oracle: for random relative inputs, the result stays under base iff normalize succeeded.
    let mut x = 0x9E3779B97F4A7C15u64;
    let parts = ["a", "b", "..", ".", "c", ""];
    for _ in 0..2000 {
        let s: Vec<&str> = (0..6)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                parts[(x % 6) as usize]
            })
            .collect();
        let rel = s.join("/");
        if rel.is_empty() {
            continue;
        }
        if let Ok(j) = safe_join(base, &rel) {
            assert!(j.starts_with(base), "{rel:?} escaped: {j:?}");
            assert!(!j.components().any(|c| c == Component::ParentDir));
        }
    }

    // Lexical checks are NOT enough: a symlink inside base can point outside. Confirm on disk with canonicalize.
    fn confined(base: &Path, joined: &Path) -> std::io::Result<bool> {
        Ok(joined.canonicalize()?.starts_with(base.canonicalize()?))
    }
    let root = std::env::temp_dir().join(format!("cb1063_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("base/inner")).unwrap();
    fs::create_dir_all(root.join("outside")).unwrap();
    fs::write(root.join("outside/secret"), b"s").unwrap();
    fs::write(root.join("base/inner/ok"), b"o").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("outside"), root.join("base/escape")).unwrap();
        let b = root.join("base");
        let lexical = safe_join(&b, "escape/secret").unwrap(); // passes the lexical test...
        assert!(
            !confined(&b, &lexical).unwrap(),
            "...but canonicalize reveals the escape"
        );
        assert!(confined(&b, &safe_join(&b, "inner/ok").unwrap()).unwrap());
        assert_eq!(
            confined(&b, &safe_join(&b, "inner/missing").unwrap())
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    fs::remove_dir_all(&root).unwrap();
    println!("traversal ok");
}

// ---------------------------------------------------------------- 5 OsStr / non-UTF-8

fn osstr_section() {
    section(5, "OsStr/OsString and non-UTF-8 names");
    let s = OsStr::new("report.txt");
    assert_eq!(s.to_str(), Some("report.txt"));
    assert_eq!(s.len(), 10);
    let mut o = OsString::from("dir");
    o.push("/file");
    assert_eq!(o.clone().into_string(), Ok("dir/file".to_string()));
    assert_eq!(Path::new(&o), Path::new("dir/file")); // AsRef<Path> for OsString
    let p = PathBuf::from(o).into_os_string();
    assert_eq!(p, OsStr::new("dir/file"));

    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let raw = b"caf\xe9.txt"; // Latin-1 e-acute: not valid UTF-8, perfectly valid on a unix filesystem
        let name = OsStr::from_bytes(raw);
        assert!(name.to_str().is_none());
        assert_eq!(name.to_string_lossy(), "caf\u{fffd}.txt");
        assert_eq!(name.as_bytes(), raw);
        let p = Path::new(name);
        assert!(p.to_str().is_none());
        assert_eq!(
            p.display().to_string(),
            "caf\u{fffd}.txt",
            "display is lossy: never round-trip through it"
        );
        assert_eq!(
            p.extension(),
            Some(OsStr::new("txt")),
            "path logic works on bytes"
        );
        // into_string returns the original OsString on failure: no data lost.
        let owned = OsString::from_vec(raw.to_vec());
        let back = owned.into_string().unwrap_err();
        assert_eq!(back.as_bytes(), raw);
        // argv/env of a child that is not UTF-8: use args_os/var_os instead of args()/var() (they panic/Err).
        assert!(std::env::var_os("CB1063_DOES_NOT_EXIST").is_none());
    }
    println!("osstr ok");
}

// ---------------------------------------------------------------- 6 CString/CStr

fn cstring_section() {
    section(6, "CString/CStr for FFI");
    let c = CString::new("hello").unwrap();
    assert_eq!(c.clone().into_bytes(), b"hello"); // into_bytes drops the trailing nul (into_bytes_with_nul keeps it)
    assert_eq!(c.as_bytes(), b"hello");
    assert_eq!(c.as_bytes_with_nul(), b"hello\0");
    assert_eq!(c.to_str(), Ok("hello"));
    // interior NUL is an error that tells you where:
    let e = CString::new("ab\0cd").unwrap_err();
    assert_eq!(e.nul_position(), 2);
    assert_eq!(e.into_vec(), b"ab\0cd"); // bytes returned, nothing lost

    // C literal (since 1.77) is a &'static CStr with no allocation or runtime check.
    let lit: &CStr = c"static";
    assert_eq!(lit.to_bytes(), b"static");
    assert_eq!(lit.count_bytes(), 6);

    // Parse a C string out of a byte buffer.
    let buf = b"name\0garbage-after";
    assert_eq!(
        CStr::from_bytes_until_nul(buf).unwrap().to_str().unwrap(),
        "name"
    );
    assert!(
        CStr::from_bytes_with_nul(buf).is_err(),
        "interior nul before the end"
    );
    assert!(
        CStr::from_bytes_with_nul(b"abc").is_err(),
        "missing terminator"
    );
    assert_eq!(
        CStr::from_bytes_with_nul(b"abc\0").unwrap().to_bytes(),
        b"abc"
    );
    let bad = CStr::from_bytes_with_nul(b"\xff\xfe\0").unwrap();
    assert!(bad.to_str().is_err());
    assert_eq!(bad.to_string_lossy(), "\u{fffd}\u{fffd}");
    let owned: CString = lit.to_owned();
    assert_eq!(owned.into_string().unwrap(), "static");

    // Calling C: as_ptr is valid only while the CString lives. c_char is i8 on x86 linux/macos, u8 on aarch64 linux!
    let n = {
        let s = CString::new("four").unwrap();
        // SAFETY: `s.as_ptr()` points to a NUL-terminated buffer owned by `s`, which outlives the call.
        unsafe { libc::strlen(s.as_ptr()) }
    };
    assert_eq!(n, 4);
    let p: *const c_char = c"x".as_ptr();
    let _ = p;
    let code: c_int = 3;
    assert_eq!(code as i64, 3);

    // into_raw/from_raw: hand ownership to C and take it back (never free with C `free`).
    let raw = CString::new("handoff").unwrap().into_raw();
    // SAFETY: `raw` came from `CString::into_raw` just above and has not been modified or freed.
    let back = unsafe { CString::from_raw(raw) };
    assert_eq!(back.to_str().unwrap(), "handoff");

    // Reading a C string written by C (libc::getenv-like): from_ptr is unsafe; we simulate with a buffer.
    let src = CString::new("from-c").unwrap();
    let p = src.as_ptr();
    // SAFETY: `p` points at the NUL-terminated string owned by `src`, alive for this scope; not mutated meanwhile.
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    assert_eq!(s, "from-c");

    // Path -> CString on unix (what `std::fs` does internally for every syscall): bytes, then NUL check.
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = CString::new(Path::new("/tmp/x y").as_os_str().as_bytes()).unwrap();
        assert_eq!(c.as_bytes_with_nul(), b"/tmp/x y\0");
        // std itself rejects NUL in paths with InvalidInput: no C-string truncation attacks.
        let e = fs::File::open("a\0b").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
    }
    println!("cstring ok");
}

// ---------------------------------------------------------------- 7 production tips

fn production_tips() {
    section(7, "Production tips and tricks");
    // 1. Accept `impl AsRef<Path>` in public APIs: callers pass &str, String, &Path, PathBuf, OsStr.
    fn ext_of(p: impl AsRef<Path>) -> Option<String> {
        p.as_ref().extension()?.to_str().map(str::to_owned)
    }
    assert_eq!(ext_of("a.rs").as_deref(), Some("rs"));
    assert_eq!(ext_of(PathBuf::from("b.md")).as_deref(), Some("md"));
    assert_eq!(ext_of(OsString::from("c.toml")).as_deref(), Some("toml"));
    // 2. Store `PathBuf`, borrow `&Path`: never `&PathBuf` in signatures (clippy::ptr_arg).
    fn takes(p: &Path) -> usize {
        p.components().count()
    }
    let owned = PathBuf::from("a/b");
    assert_eq!(takes(&owned), 2);
    // 3. Case-insensitive extension compare without allocating: eq_ignore_ascii_case on OsStr.
    let is_png = |p: &Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
    assert!(is_png(Path::new("A.PNG")) && !is_png(Path::new("a.jpg")));
    // 4. Never build paths with format!("{}/{}", ..): breaks on Windows, double slashes, absolute second part.
    assert_eq!(format!("{}/{}", "/base/", "x"), "/base//x");
    assert_eq!(Path::new("/base/").join("x"), Path::new("/base/x"));
    // 5. Never use `{:?}` of a Path for users (escaped quotes); use display(). Debug is for logs of exact bytes.
    assert_eq!(format!("{:?}", Path::new("a b")), "\"a b\"");
    assert_eq!(format!("{}", Path::new("a b").display()), "a b");
    // 6. Prefer `to_string_lossy()` (Cow, no allocation when valid) for display text; `to_str()?` when invalid must be an error.
    assert!(matches!(
        Path::new("ok").to_string_lossy(),
        std::borrow::Cow::Borrowed(_)
    ));
    // 7. Do not use `exists()` as a check-then-act guard (TOCTOU); open and handle the error (clippy: none; see std_fs_file_operations_usecases).
    assert!(!Path::new("/definitely/not/here").exists());
    // 8. Temp names: with_extension on a dotted base replaces the last extension only: "a.tar.gz".with_extension("x") = "a.tar.x".
    assert_eq!(
        Path::new("a.tar.gz").with_extension("x"),
        Path::new("a.tar.x")
    );
    //    To ADD an extension, push to the OsString: foo.rs -> foo.rs.bak
    let mut o = OsString::from("foo.rs");
    o.push(".bak");
    assert_eq!(PathBuf::from(o), Path::new("foo.rs.bak"));
    // 9. Use `std::env::args_os()`/`var_os()` for filesystem arguments; `args()` panics on invalid UTF-8.
    assert!(std::env::args_os().count() >= 1);
    // 10. Keep CString alive across the call: `CString::new(s).unwrap().as_ptr()` as one expression dangles
    //     (clippy::temporary_cstring_as_ptr denies this). Bind it to a variable.
    let keep = CString::new("ok").unwrap();
    let p = keep.as_ptr();
    // SAFETY: `keep` is still alive and NUL-terminated.
    assert_eq!(unsafe { CStr::from_ptr(p) }.to_bytes(), b"ok");
    // 11. Path::new("") joins as nothing; `components().next().is_none()` is the "empty path" test.
    assert!(Path::new("").components().next().is_none());
    println!("tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|c| seen.insert(*c)),
        "COVERED has duplicates"
    );
    everyday_core();
    join_pitfalls();
    components_section();
    traversal_section();
    osstr_section();
    cstring_section();
    production_tips();
    println!("\nall sections passed ({} methods listed)", COVERED.len());
}
