//! RECIPE 1116 (crates): tempfile, walkdir, globset, ignore, same-file and dirs: temporary files, tree walking and matching
//! LEVEL: intermediate
//! Use in: build tools, search tools, linters, backup and sync programs, and any test that touches the filesystem.   Run: cargo run --release -p cb-crate-tour --example tempfile_walkdir_glob_ignore
//! COMMON USES: atomic config rewrites, isolated test directories; `ignore` is the directory walker used by ripgrep.
//! METHODS: tempfile::tempdir, tempfile::Builder::{prefix, suffix, tempdir_in}, TempDir::path, TempDir::close, TempDir::keep, NamedTempFile::new_in, NamedTempFile::persist, NamedTempFile::persist_noclobber, NamedTempFile::reopen, tempfile::tempfile, SpooledTempFile, walkdir::WalkDir::new, WalkDir::{min_depth, max_depth, sort_by_file_name, follow_links, contents_first}, IntoIter::filter_entry, IntoIter::skip_current_dir, DirEntry::{depth, path, file_name, file_type}, walkdir::Error::{loop_ancestor, path, depth}, globset::Glob, globset::GlobBuilder::literal_separator, GlobSetBuilder, GlobSet::{is_match, matches}, ignore::WalkBuilder::{hidden, require_git, git_global, add_custom_ignore_filename, overrides}, ignore::overrides::OverrideBuilder, ignore::gitignore::GitignoreBuilder, Gitignore::matched, same_file::is_same_file, dirs::{home_dir, config_dir, cache_dir}, NamedTempFile::{into_temp_path, as_file}, TempPath, tempfile::Builder::{permissions, rand_bytes, make_in}, WalkDir::sort_by_key, globset::escape, GlobBuilder::case_insensitive, ignore::types::TypesBuilder, WalkBuilder::{types, filter_entry, max_depth, max_filesize, build_parallel, threads}, ignore::WalkState, Gitignore::matched_path_or_any_parents
//! DOCS: https://docs.rs/tempfile/3/ https://docs.rs/walkdir/2/ https://docs.rs/globset/0.4/ https://docs.rs/ignore/0.4/ https://docs.rs/same-file/1/ https://docs.rs/dirs/6/
//! CRATES: tempfile = "3", walkdir = "2", globset = "0.4", ignore = "0.4", same-file = "1", dirs = "6"
//!
//! CHEAT SHEET
//!   scratch directory for a test            -> tempfile::tempdir()  (deleted on drop; keep the guard ALIVE)
//!   replace a file atomically               -> NamedTempFile::new_in(SAME directory) -> write -> sync_all -> persist
//!   walk everything                         -> WalkDir (yields Result: permission errors do not stop the walk)
//!   prune subtrees                          -> filter_entry (Iterator::filter only hides, it still descends)
//!   walk like a developer tool              -> ignore::WalkBuilder (.gitignore, hidden files)
//!   many globs against one path             -> GlobSet ; literal text inside a glob -> globset::escape
//!   "only Rust files" (like rg -t rust)     -> ignore::types::TypesBuilder
//!   use every core while walking            -> build_parallel().run(..)  (no ordering guarantee)
//!   hand a temp file name to another tool   -> into_temp_path() (handle closed, still deleted on drop)
//!   user directories                        -> dirs::config_dir() and friends (platform specific, Option)
//!
//! FS HELPERS: `fs_extra`-style recursive copy is about fifteen lines with walkdir (`copy_tree` below).
//! WATCHING: file watching crates are out of scope; the polling snapshot at the end shows the portable fallback.
//! PLATFORM: the symlink sections are compiled only on Unix.
//!
//! Read: https://github.com/BurntSushi/walkdir ; https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore ; https://git-scm.com/docs/gitignore
//!
//! TRY NEXT:
//!   - [S] Replace filter_entry with Iterator::filter in `pruning` and count how many entries are still visited.
//!   - [M] Add a symlink option to copy_tree and defend against loops.
//!   - [A] PROJECT: build a mini ripgrep from ignore::WalkBuilder + regex, then compare its file list with `rg --files`.
#![allow(dead_code)]
use globset::{Glob, GlobBuilder, GlobSetBuilder};
use ignore::WalkBuilder;
use ignore::gitignore::GitignoreBuilder;
use ignore::overrides::OverrideBuilder;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

fn make_tree(root: &Path, files: &[(&str, &str)]) {
    for (path, body) in files {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
}
/// Relative paths with forward slashes.
fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn temporary_files() {
    let kept_path;
    {
        let dir = tempfile::tempdir().unwrap();
        kept_path = dir.path().to_path_buf();
        make_tree(dir.path(), &[("a/b.txt", "x")]);
    } // dropped: removed recursively, errors ignored
    assert!(!kept_path.exists());
    // PITFALL: `tempdir().unwrap().path().join("x")` drops the TempDir at the end of the statement.
    let dangling: PathBuf = tempfile::tempdir().unwrap().path().join("x");
    assert!(!dangling.parent().unwrap().exists());
    // close() reports deletion errors; keep() disables deletion.
    let d = tempfile::tempdir().unwrap();
    d.close().unwrap();
    let base = tempfile::tempdir().unwrap();
    let named = tempfile::Builder::new()
        .prefix("job-")
        .suffix(".tmp")
        .tempdir_in(base.path())
        .unwrap();
    let name = named
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(name.starts_with("job-") && name.ends_with(".tmp"), "{name}");
    let kept = named.keep();
    assert!(kept.is_dir());

    // An anonymous file: no path, gone when the handle closes.
    let mut anon = tempfile::tempfile().unwrap();
    anon.write_all(b"secret scratch").unwrap();
    anon.seek(SeekFrom::Start(0)).unwrap();
    let mut s = String::new();
    anon.read_to_string(&mut s).unwrap();
    assert_eq!(s, "secret scratch");
    // reopen() gives an independent handle with its own cursor.
    let mut f = tempfile::NamedTempFile::new_in(base.path()).unwrap();
    f.write_all(b"abc").unwrap();
    let mut again = String::new();
    f.reopen().unwrap().read_to_string(&mut again).unwrap();
    assert_eq!(again, "abc");
    // SpooledTempFile stays in memory until it exceeds the threshold.
    let mut spool = tempfile::SpooledTempFile::new(8);
    spool.write_all(b"1234").unwrap();
    assert!(!spool.is_rolled());
    spool.write_all(b"567890").unwrap();
    assert!(spool.is_rolled());
}

/// Readers see either the whole old file or the whole new one. The temp file must be on the SAME filesystem as the
/// target (so: same directory), because a rename cannot cross filesystems.
fn atomic_write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = target.parent().expect("target has a parent");
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?; // data durable before it becomes visible
    tmp.persist(target).map_err(|e| e.error)?;
    // Full crash durability of the RENAME additionally needs an fsync of the directory on Unix (not done here).
    Ok(())
}
fn atomic_replace() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("config.json");
    atomic_write(&target, &[b'a'; 4096]).unwrap();
    std::thread::scope(|s| {
        let reader = s.spawn(|| {
            for _ in 0..500 {
                let data = fs::read(&target).unwrap();
                assert!(
                    data.len() == 4096 && data.iter().all(|&b| b == data[0]),
                    "torn read: {} bytes",
                    data.len()
                );
            }
        });
        for i in 0..100u8 {
            atomic_write(&target, &[b'a' + i % 26; 4096]).unwrap();
        }
        reader.join().unwrap();
    });
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no temp files left behind"
    );
    // persist_noclobber refuses to overwrite, and hands the temp file back in the error.
    let tmp = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    let err = tmp.persist_noclobber(&target).unwrap_err();
    assert_eq!(err.error.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(err.file.path().exists());
}

fn is_hidden(e: &DirEntry) -> bool {
    e.depth() > 0 && e.file_name().to_str().is_some_and(|n| n.starts_with('.'))
}
fn walking_and_pruning() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_tree(
        root,
        &[
            ("src/main.rs", "fn main(){}"),
            ("src/util/mod.rs", "//"),
            ("target/debug/big.bin", "0000000000"),
            (".git/config", "x"),
            ("README.md", "hi"),
        ],
    );
    let list = |it: walkdir::IntoIter| it.map(|e| rel(root, e.unwrap().path())).collect::<Vec<_>>();
    let all = list(
        WalkDir::new(root)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter(),
    );
    assert_eq!(
        all[..3],
        [".git", ".git/config", "README.md"],
        "depth first, siblings sorted by name"
    );
    assert_eq!(
        list(
            WalkDir::new(root)
                .min_depth(1)
                .max_depth(1)
                .sort_by_file_name()
                .into_iter()
        ),
        [".git", "README.md", "src", "target"]
    );

    // filter_entry prunes: the closure is never asked about anything under a rejected directory.
    let mut asked = vec![];
    let files: Vec<String> = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            asked.push(rel(root, e.path()));
            !is_hidden(e) && e.file_name() != "target"
        })
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| rel(root, e.path()))
        .collect();
    assert_eq!(files, ["README.md", "src/main.rs", "src/util/mod.rs"]);
    assert!(
        !asked
            .iter()
            .any(|p| p.starts_with("target/") || p.starts_with(".git/")),
        "{asked:?}"
    );
    // skip_current_dir does the same from inside a loop.
    let mut it = WalkDir::new(root).sort_by_file_name().into_iter();
    let mut seen = 0;
    while let Some(e) = it.next() {
        if e.unwrap().file_name() == "src" {
            it.skip_current_dir();
        }
        seen += 1;
    }
    assert_eq!(
        seen,
        all.len() + 1 - 3,
        "main.rs, util and util/mod.rs skipped"
    );
    // contents_first yields children BEFORE their directory: the order needed to delete a tree.
    let post: Vec<String> = list(
        WalkDir::new(root.join("src"))
            .contents_first(true)
            .sort_by_file_name()
            .into_iter(),
    );
    assert_eq!(post, ["src/main.rs", "src/util/mod.rs", "src/util", "src"]);
    // Aggregate: bytes per extension.
    let mut sizes = BTreeMap::new();
    for e in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let ext = e
            .path()
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("(none)")
            .to_string();
        *sizes.entry(ext).or_insert(0) += e.metadata().unwrap().len();
    }
    assert_eq!((sizes["rs"], sizes["bin"]), (13, 10));
    // Errors are items, with the path attached.
    let e = WalkDir::new(root.join("nope"))
        .into_iter()
        .next()
        .unwrap()
        .unwrap_err();
    assert_eq!(
        (e.depth(), e.path()),
        (0, Some(root.join("nope").as_path()))
    );
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<u64> {
    let mut bytes = 0;
    for entry in WalkDir::new(from) {
        let entry = entry?;
        let dest = to.join(entry.path().strip_prefix(from).expect("under the root"));
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else if entry.file_type().is_file() {
            bytes += fs::copy(entry.path(), &dest)?;
        } // symlinks are skipped
    }
    Ok(bytes)
}
fn copying_and_identity() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    make_tree(a.path(), &[("x/y/z.txt", "12345"), ("top", "1")]);
    assert_eq!(copy_tree(a.path(), b.path()).unwrap(), 6);
    assert_eq!(
        fs::read_to_string(b.path().join("x/y/z.txt")).unwrap(),
        "12345"
    );
    // Different spellings, same file. Equal CONTENT does not make two files the same.
    let p = a.path().join("top");
    assert!(same_file::is_same_file(&p, a.path().join("x/../top")).unwrap());
    assert!(!same_file::is_same_file(&p, b.path().join("top")).unwrap());
    let hard = a.path().join("hard");
    fs::hard_link(&p, &hard).unwrap();
    assert!(
        same_file::is_same_file(&p, &hard).unwrap(),
        "a hard link is the same file"
    );
}

#[cfg(unix)]
fn symlinks() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_tree(root, &[("real/f", "x")]);
    symlink(root.join("real"), root.join("link")).unwrap();
    let count = |follow| {
        WalkDir::new(root)
            .follow_links(follow)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() == "f")
            .count()
    };
    assert_eq!(
        (count(false), count(true)),
        (1, 2),
        "links are not followed by default"
    );
    symlink(root, root.join("real/loop")).unwrap();
    let errors: Vec<_> = WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(Result::err)
        .collect();
    assert!(
        !errors.is_empty() && errors.iter().all(|e| e.loop_ancestor().is_some()),
        "cycles are detected, not followed forever"
    );
}
#[cfg(not(unix))]
fn symlinks() {}

fn globs() {
    let m = |g: &str, p: &str| Glob::new(g).unwrap().compile_matcher().is_match(p);
    assert!(m("*.rs", "main.rs"));
    // PITFALL: in globset's default, `*` also matches '/'.
    assert!(m("*.rs", "src/deep/main.rs"));
    let strict = GlobBuilder::new("*.rs")
        .literal_separator(true)
        .build()
        .unwrap()
        .compile_matcher();
    assert!(strict.is_match("main.rs") && !strict.is_match("src/main.rs"));
    assert!(
        m("src/**/*.rs", "src/main.rs"),
        "** matches zero or more directories"
    );
    assert!(m("*.{rs,toml}", "Cargo.toml") && !m("*.{rs,toml}", "x.md"));
    assert!(m("file?.[ch]", "file1.c") && !m("file?.[!ch]", "file1.c"));
    assert!(Glob::new("a[").is_err() && Glob::new("{a").is_err());
    let patterns = ["**/*.rs", "target/**", "**/test_*"];
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).unwrap());
    }
    let set = b.build().unwrap();
    assert_eq!(set.matches("target/x/test_a.rs"), [0, 1, 2]);
    assert!(!set.is_match("README.md"));
    // A set must agree with matching each glob on its own.
    for path in [
        "a.rs",
        "target",
        "target/a",
        "x/test_",
        "x/y/test_b.txt",
        "",
    ] {
        let one_by_one: Vec<usize> = (0..patterns.len())
            .filter(|&i| m(patterns[i], path))
            .collect();
        assert_eq!(set.matches(path), one_by_one, "{path}");
    }
}

fn ignore_rules() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_tree(
        root,
        &[
            (".gitignore", "*.log\n/build/\n!keep.log\nsecret?.txt\n"),
            ("a.log", ""),
            ("keep.log", ""),
            ("build/out", ""),
            ("sub/build/out", ""),
            ("sub/b.log", ""),
            ("sub/.gitignore", "!b.log\n"),
            ("secret1.txt", ""),
            (".hidden", ""),
            ("main.rs", ""),
            (".cbignore", "main.rs\n"),
        ],
    );
    let walk = |b: &mut WalkBuilder| {
        let mut v: Vec<String> = b
            .build()
            .map(Result::unwrap)
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| rel(root, e.path()))
            .collect();
        v.sort();
        v
    };
    // Make the result independent of this machine: no global or parent-directory ignore files.
    let base = || {
        let mut b = WalkBuilder::new(root);
        // require_git(false): honour .gitignore even though this temp dir is not a git repository.
        b.require_git(false)
            .git_global(false)
            .parents(false)
            .ignore(false);
        b
    };
    assert_eq!(
        walk(&mut base()),
        ["keep.log", "main.rs", "sub/b.log", "sub/build/out"],
        "negation, anchoring, nested override, hidden skipped"
    );
    // Without require_git(false) the .gitignore files are NOT applied outside a repository.
    let mut default = WalkBuilder::new(root);
    default.git_global(false).parents(false).ignore(false);
    assert!(walk(&mut default).contains(&"a.log".to_string()));
    let with_hidden = walk(base().hidden(false));
    assert!(
        with_hidden.contains(&".hidden".to_string()) && !with_hidden.contains(&"a.log".to_string())
    );
    assert!(!walk(base().add_custom_ignore_filename(".cbignore")).contains(&"main.rs".to_string()));
    // Overrides behave like command-line --glob filters: a positive glob means "only these".
    let mut o = OverrideBuilder::new(root);
    o.add("*.rs").unwrap();
    assert_eq!(walk(base().overrides(o.build().unwrap())), ["main.rs"]);

    // The matcher without a walk.
    let mut g = GitignoreBuilder::new(root);
    for line in ["*.log", "!keep.log", "build/"] {
        g.add_line(None, line).unwrap();
    }
    let g = g.build().unwrap();
    assert!(g.matched(root.join("x.log"), false).is_ignore());
    assert!(g.matched(root.join("keep.log"), false).is_whitelist());
    assert!(
        g.matched(root.join("build"), true).is_ignore()
            && g.matched(root.join("build"), false).is_none(),
        "trailing / = directories only"
    );
}

/// Portable change detection by polling: compare two snapshots of (path -> length, content).
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| (rel(root, e.path()), fs::read(e.path()).unwrap()))
        .collect()
}
fn polling_and_user_dirs() {
    let d = tempfile::tempdir().unwrap();
    make_tree(d.path(), &[("keep", "1"), ("edit", "1"), ("gone", "1")]);
    let before = snapshot(d.path());
    fs::write(d.path().join("edit"), "2").unwrap(); // same length: size alone would miss it
    fs::remove_file(d.path().join("gone")).unwrap();
    fs::write(d.path().join("new"), "").unwrap();
    let after = snapshot(d.path());
    let added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    let removed: Vec<&String> = before.keys().filter(|k| !after.contains_key(*k)).collect();
    let changed: Vec<&String> = after
        .iter()
        .filter(|(k, v)| before.get(*k).is_some_and(|old| old != *v))
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        (added, removed, changed),
        (
            vec![&"new".to_string()],
            vec![&"gone".to_string()],
            vec![&"edit".to_string()]
        )
    );
    // dirs: platform conventions. Values depend on the user and OS, so they are printed, never asserted.
    println!(
        "dirs (machine dependent): home={} config={} cache={}",
        dirs::home_dir().is_some(),
        dirs::config_dir().is_some(),
        dirs::cache_dir().is_some()
    );
}

#[cfg(unix)]
fn temp_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    // Temp files are private: owner read and write only. Good for secrets ...
    let tmp = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    assert_eq!(mode(tmp.path()), 0o600);
    // ... and a PITFALL for atomic replacement: persist() renames the 0600 file over the target, so the target's
    // permissions are LOST. A config that other users or a web server could read suddenly becomes unreadable.
    let target = dir.path().join("public.conf");
    fs::write(&target, "v1").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    atomic_write(&target, b"v2").unwrap();
    assert_eq!(mode(&target), 0o600);
    // FIX: copy the old permissions onto the temp file BEFORE the rename.
    fn atomic_write_keeping_mode(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut tmp = tempfile::NamedTempFile::new_in(target.parent().expect("has a parent"))?;
        tmp.write_all(bytes)?;
        match fs::metadata(target) {
            Ok(m) => tmp.as_file().set_permissions(m.permissions())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        tmp.as_file().sync_all()?;
        tmp.persist(target).map_err(|e| e.error)?;
        Ok(())
    }
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    atomic_write_keeping_mode(&target, b"v3").unwrap();
    assert_eq!(
        (mode(&target), fs::read_to_string(&target).unwrap().as_str()),
        (0o640, "v3")
    );
    // Or ask for a mode at creation time (still limited by the process umask).
    let locked = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o400))
        .tempfile_in(dir.path())
        .unwrap();
    assert_eq!(mode(locked.path()), 0o400);

    // make(): get tempfile's unique naming and retry loop for things that are not regular files, such as symlinks.
    let link = tempfile::Builder::new()
        .prefix("link-")
        .rand_bytes(10)
        .make_in(dir.path(), |p| std::os::unix::fs::symlink(&target, p))
        .unwrap();
    let name = link.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(name.len(), "link-".len() + 10);
    assert_eq!(fs::read_to_string(link.path()).unwrap(), "v3");
    let link_path = link.path().to_path_buf();
    drop(link);
    assert!(
        fs::symlink_metadata(&link_path).is_err() && target.exists(),
        "only the link is removed"
    );
}
#[cfg(not(unix))]
fn temp_file_permissions() {}

fn temp_paths_and_walk_pitfalls() {
    let dir = tempfile::tempdir().unwrap();
    // into_temp_path closes the handle but keeps ownership of the file: needed when another program must open it
    // exclusively (Windows) or when you want to stop holding descriptors.
    let mut f = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    f.write_all(b"input for a child process").unwrap();
    let temp_path = f.into_temp_path();
    let path = temp_path.to_path_buf();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "input for a child process"
    );
    drop(temp_path);
    assert!(!path.exists());

    // PITFALL: filter_entry is ALSO asked about the root. Here the root is named like a hidden directory, so a
    // "skip hidden" rule without a depth check prunes the whole walk and silently returns nothing.
    let root = dir.path().join(".cache");
    make_tree(&root, &[("b/2", ""), ("a/1", ""), (".git/x", "")]);
    let naive = |e: &DirEntry| !e.file_name().to_string_lossy().starts_with('.');
    assert_eq!(
        WalkDir::new(&root).into_iter().filter_entry(naive).count(),
        0
    );
    assert!(
        WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
            .count()
            > 1
    );
    // sort_by_key for any order you can compute, here reverse by name.
    let names: Vec<String> = WalkDir::new(&root)
        .min_depth(1)
        .max_depth(1)
        .sort_by_key(|e| std::cmp::Reverse(e.file_name().to_owned()))
        .into_iter()
        .map(|e| rel(&root, e.unwrap().path()))
        .collect();
    assert_eq!(names, ["b", "a", ".git"]);

    // A user-supplied name inside a glob must be escaped.
    let user = "report[2024]{final}.txt";
    let glob = |g: &str| Glob::new(g).unwrap().compile_matcher();
    assert!(
        !glob(user).is_match(user),
        "unescaped, [2024] is a character class"
    );
    assert!(glob(&globset::escape(user)).is_match(user));
    assert!(
        GlobBuilder::new("*.JPG")
            .case_insensitive(true)
            .build()
            .unwrap()
            .compile_matcher()
            .is_match("a.jpg")
    );
}

fn ignore_types_depth_and_parallel() {
    use ignore::WalkState;
    use ignore::types::TypesBuilder;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    make_tree(
        root,
        &[
            (".gitignore", "build/\n"),
            ("a.rs", "x"),
            ("b.toml", "x"),
            ("c.proto", "x"),
            ("deep/er/d.rs", "x"),
            ("build/gen.rs", "x"),
            ("big.rs", "0123456789"),
            ("vendor/v.rs", "x"),
        ],
    );
    let base = || {
        let mut b = WalkBuilder::new(root);
        b.require_git(false)
            .git_global(false)
            .parents(false)
            .ignore(false);
        b
    };
    let files = |b: &mut WalkBuilder| {
        let mut v: Vec<String> = b
            .build()
            .map(Result::unwrap)
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .map(|e| rel(root, e.path()))
            .collect();
        v.sort();
        v
    };
    // File types, as in `rg -t rust -t proto`: predefined definitions plus your own.
    let mut types = TypesBuilder::new();
    types.add_defaults();
    types.add("idl", "*.proto").unwrap();
    types.select("rust").select("idl");
    let mut b = base();
    b.types(types.build().unwrap());
    assert_eq!(
        files(&mut b),
        ["a.rs", "big.rs", "c.proto", "deep/er/d.rs", "vendor/v.rs"]
    );
    assert_eq!(
        files(base().max_depth(Some(1))),
        ["a.rs", "b.toml", "big.rs", "c.proto"]
    );
    assert!(!files(base().max_filesize(Some(5))).contains(&"big.rs".to_string()));
    // filter_entry composes with the ignore rules and prunes whole subtrees.
    assert!(
        !files(base().filter_entry(|e| e.file_name() != "vendor"))
            .iter()
            .any(|p| p.starts_with("vendor/"))
    );
    // A NEGATED override excludes, and leaves everything else in.
    let mut o = OverrideBuilder::new(root);
    o.add("!*.rs").unwrap();
    assert_eq!(
        files(base().overrides(o.build().unwrap())),
        ["b.toml", "c.proto"]
    );

    // Parallel walk: one visitor closure per thread; results arrive in no particular order.
    let found = std::sync::Mutex::new(Vec::new());
    base().threads(4).build_parallel().run(|| {
        Box::new(|entry| {
            let entry = entry.unwrap();
            if entry.file_name() == "vendor" {
                return WalkState::Skip; // do not descend
            }
            if entry.file_type().is_some_and(|t| t.is_file()) {
                found.lock().unwrap().push(rel(root, entry.path()));
            }
            WalkState::Continue
        })
    });
    let mut found = found.into_inner().unwrap();
    found.sort();
    let mut serial = files(&mut base());
    serial.retain(|p| !p.starts_with("vendor/"));
    assert_eq!(found, serial);

    // PITFALL: Gitignore::matched looks only at the path you give it. A file INSIDE an ignored directory does not match
    // `build/` by itself (a walker never gets there, but a file watcher does).
    let mut g = GitignoreBuilder::new(root);
    g.add_line(None, "build/").unwrap();
    let g = g.build().unwrap();
    let inner = root.join("build/gen.rs");
    assert!(g.matched(&inner, false).is_none());
    assert!(g.matched_path_or_any_parents(&inner, false).is_ignore());
}

pub fn main() {
    temporary_files();
    atomic_replace();
    walking_and_pruning();
    copying_and_identity();
    symlinks();
    globs();
    ignore_rules();
    polling_and_user_dirs();
    temp_file_permissions();
    temp_paths_and_walk_pitfalls();
    ignore_types_depth_and_parallel();
    println!("filesystem crates: all sections passed");
}
