use solution::*;

#[allow(dead_code, unused_imports)]
use std::fs;
#[allow(dead_code, unused_imports)]
use std::os::unix::fs::symlink;
#[allow(dead_code, unused_imports)]
use std::path::{Path, PathBuf};

/// base/public is the served folder; everything else in base is private. Removed when dropped.
#[allow(dead_code, unused_imports)]
struct Site {
    base: PathBuf,
    root: PathBuf,
}

#[allow(dead_code, unused_imports)]
impl Site {
    fn new() -> Site {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let base = std::env::temp_dir().join(format!("folder-{}-{n}-{nanos}", std::process::id()));
        let root = base.join("public");
        fs::create_dir_all(root.join("docs/deep")).unwrap();
        fs::create_dir_all(base.join("public-keys")).unwrap();
        for (path, text) in [("public/a.txt", "a"), ("public/docs/b.txt", "b"), ("public/docs/deep/c d.txt", "c"), ("public/%2e", "literal"), ("secret.txt", "S"), ("public-keys/key", "S")] {
            fs::write(base.join(path), text).unwrap();
        }
        symlink(base.join("secret.txt"), root.join("leak")).unwrap();
        symlink(base.join("public-keys"), root.join("neighbour")).unwrap();
        symlink("../../secret.txt", root.join("docs/relative-leak")).unwrap();
        symlink(&base, root.join("up")).unwrap();
        symlink(root.join("docs"), root.join("shortcut")).unwrap();
        symlink(root.join("nowhere"), root.join("dangling")).unwrap();
        Site { base, root }
    }

    /// The text of the resolved file, or the refusal.
    fn read(&self, raw: &str) -> Result<String, Denied> {
        let path = resolve(&self.root, raw)?;
        Ok(fs::read_to_string(&path).unwrap_or_else(|e| panic!("resolve returned {path:?}, which cannot be read: {e}")))
    }
}

#[allow(dead_code, unused_imports)]
impl Drop for Site {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[allow(dead_code, unused_imports)]
fn ok(text: &str) -> Result<String, Denied> {
    Ok(text.to_string())
}

#[test]
fn every_spelling_of_going_up() {
    let s = Site::new();
    for raw in ["..", "%2e%2e/secret.txt", "%2E%2e%2Fsecret.txt", "..%2fsecret.txt", "docs/../a.txt", "docs/%2e%2e/a.txt", "docs%2f%2e%2e%2fa.txt", "docs/%2e%2e/%2e%2e/secret.txt", "docs/..", "./../secret.txt", "//../secret.txt"] {
        check!(raw, s.read(raw), Err(Denied::Forbidden));
    }
}

#[test]
fn backslashes_and_nul() {
    let s = Site::new();
    for raw in ["..\\secret.txt", "docs%5c..%5c..%5csecret.txt", "a.txt%00", "%00", "a\\b"] {
        check!(raw, s.read(raw), Err(Denied::Forbidden));
    }
}

#[test]
fn every_link_that_leads_out() {
    let s = Site::new();
    for raw in ["neighbour/key", "docs/relative-leak", "up/secret.txt", "up/public-keys/key", "up", "neighbour"] {
        check!(raw, s.read(raw), Err(Denied::Forbidden));
    }
}

#[test]
fn links_that_stay_inside_work() {
    let s = Site::new();
    check!("shortcut/b.txt", s.read("shortcut/b.txt"), ok("b"));
    check!("out and back in through a link", s.read("up/public/a.txt"), ok("a"));
}

#[test]
fn the_answer_is_the_real_location() {
    let s = Site::new();
    let want = fs::canonicalize(s.root.join("docs/b.txt")).unwrap();
    check!("shortcut/b.txt", resolve(&s.root, "shortcut/b.txt"), Ok(want));
}

#[test]
fn a_root_that_is_a_link() {
    let s = Site::new();
    let linked = s.base.join("served");
    symlink(&s.root, &linked).unwrap();
    check!("through a linked root", resolve(&linked, "a.txt").map(|p| fs::read_to_string(p).unwrap()), ok("a"));
    check!("still no way out", resolve(&linked, "leak"), Err(Denied::Forbidden));
}

#[test]
fn a_root_that_does_not_exist() {
    check!("no root", resolve(Path::new("/no/such/folder/anywhere"), "a"), Err(Denied::NotFound));
}

#[test]
fn decoding_happens_once() {
    let s = Site::new();
    check!("%252e is the file named %2e", s.read("%252e"), ok("literal"));
    check!("%252e%252e is only a name", s.read("%252e%252e/secret.txt"), Err(Denied::NotFound));
}

#[test]
fn broken_escapes_and_bytes() {
    let s = Site::new();
    for raw in ["%", "%g0", "%0g", "a%+1", "%-1", "x/%2", "%ff", "%c3%28"] {
        check!(raw, s.read(raw), Err(Denied::BadRequest));
    }
}

#[test]
fn absolute_names_stay_inside() {
    let s = Site::new();
    check!("/a.txt", s.read("/a.txt"), ok("a"));
    check!("%2fa.txt", s.read("%2fa.txt"), ok("a"));
    check!("/etc/passwd", s.read("/etc/passwd"), Err(Denied::NotFound));
    let outside = s.base.join("secret.txt");
    check!("the full path of a private file", s.read(outside.to_str().unwrap()), Err(Denied::NotFound));
}

#[test]
fn empty_names_and_dead_links() {
    let s = Site::new();
    for raw in ["", "/", ".", "./.", "dangling", "a.txt/x"] {
        check!(raw, s.read(raw), Err(Denied::NotFound));
    }
}

#[test]
fn random_segments_never_reach_outside() {
    let s = Site::new();
    let mut rng = anneal_prelude::Rng::new(71201);
    let pieces = ["a.txt", "docs", "deep", "b.txt", "..", "%2e%2e", ".", "", "leak", "up", "public", "public-keys", "neighbour", "key", "secret.txt", "shortcut", "%2f", "%5c"];
    let root = fs::canonicalize(&s.root).unwrap();
    for _ in 0..2000 {
        let n = 1 + rng.below(5);
        let raw = (0..n).map(|_| *rng.pick(&pieces)).collect::<Vec<_>>().join("/");
        if let Ok(path) = resolve(&s.root, &raw) {
            check!(format!("{raw} resolved to {path:?}"), path.starts_with(&root) && path.is_file(), true);
            check!(format!("{raw} reads private data"), fs::read_to_string(&path).unwrap() == "S", false);
        } else if !raw.contains("..") && !raw.contains("%2e%2e") && !raw.contains("%5c") {
            // without any going up, only links out are forbidden
            if resolve(&s.root, &raw) == Err(Denied::Forbidden) {
                check!(format!("{raw} refused as forbidden without a link out"), raw.split(['/', '%']).any(|p| ["leak", "up", "neighbour"].contains(&p)), true);
            }
        }
    }
}
