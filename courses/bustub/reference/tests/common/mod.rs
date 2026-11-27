//! Helpers shared by the tests. Not part of the course: nothing here for you to write.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use bustub::common::config::{PageData, BUSTUB_PAGE_SIZE};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory under the system temp dir, removed when dropped. Tests run in parallel, so each gets its own.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bustub-rs-{}-{tag}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub fn dir(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A page whose bytes are all `byte`.
pub fn page_of(byte: u8) -> PageData {
    [byte; BUSTUB_PAGE_SIZE]
}

/// A page that starts with `text` and is zero after it, like `strncpy` into a zeroed buffer.
pub fn page_with(text: &str) -> PageData {
    let mut p = [0u8; BUSTUB_PAGE_SIZE];
    p[..text.len()].copy_from_slice(text.as_bytes());
    p
}

/// A page whose contents depend on `seed`, so a page read from the wrong place is noticed.
pub fn pattern(seed: usize) -> PageData {
    let mut p = [0u8; BUSTUB_PAGE_SIZE];
    for (i, b) in p.iter_mut().enumerate() {
        *b = (seed.wrapping_mul(31).wrapping_add(i * 7) % 251) as u8 + 1;
    }
    p
}
