"""Helpers shared by the Y17 and Y18 specs (the key-value store series)."""
import pathlib
import subprocess

from author import ROOT


def broken(solution, *pairs):
    """A plausible wrong solution: the reference with each (old, new) applied once. `old` must occur exactly once."""
    out = solution
    for old, new in pairs:
        assert out.count(old) == 1, f"expected exactly one {old!r}, found {out.count(old)}"
        out = out.replace(old, new)
    return out


# A scratch directory for tests that use real files. Each test gets its own, and it is removed afterwards.
TEMP_DIR = r'''
#[allow(dead_code)]
struct TempDir(std::path::PathBuf);

#[allow(dead_code)]
impl TempDir {
    fn new(tag: &str) -> TempDir {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("anneal-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn join(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
'''


def rustfmt(track_folder):
    """Formats every starter and reference as the editor does (crates/rules/tests/content.rs checks the starters)."""
    root = pathlib.Path(ROOT) / track_folder / "problems"
    files = sorted(str(p) for name in ("starter.rs", "solution.rs") for p in root.glob(f"*/{name}"))
    subprocess.run(["rustfmt", "--edition", "2021", *files], check=True)
