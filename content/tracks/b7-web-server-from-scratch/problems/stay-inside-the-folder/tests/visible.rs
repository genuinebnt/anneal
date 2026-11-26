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
fn files_inside_are_found() {
    let s = Site::new();
    check!("a.txt and docs/b.txt", (s.read("a.txt"), s.read("docs/b.txt")), (ok("a"), ok("b")));
}

#[test]
fn escapes_are_decoded() {
    check!("docs/deep/c%20d.txt", Site::new().read("docs/deep/c%20d.txt"), ok("c"));
}

#[test]
fn going_up_is_forbidden() {
    check!("../secret.txt", Site::new().read("../secret.txt"), Err(Denied::Forbidden));
}

#[test]
fn a_link_out_is_forbidden() {
    check!("leak", Site::new().read("leak"), Err(Denied::Forbidden));
}

#[test]
fn a_missing_file_and_a_folder_are_not_found() {
    let s = Site::new();
    check!("nope and docs", (s.read("nope"), s.read("docs")), (Err(Denied::NotFound), Err(Denied::NotFound)));
}

#[test]
fn a_broken_escape_is_a_bad_request() {
    check!("a%2", Site::new().read("a%2"), Err(Denied::BadRequest));
}
