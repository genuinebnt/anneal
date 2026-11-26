#![allow(dead_code)] // each test binary uses a different subset

use mini_kv::Store;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A fresh directory per test, removed on drop. `tag` must be unique among the tests that run in parallel.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("mini_kv_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create temp dir");
        TempDir(p)
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub type Model = BTreeMap<Vec<u8>, Vec<u8>>;

/// Everything the store holds, as a map. The test keys are ASCII, so `\xff` is above all of them.
pub fn contents(store: &Store) -> Model {
    store.scan(b"", b"\xff").expect("scan").into_iter().collect()
}

/// `scan` and `get` take different paths through the store, so check both against the model.
pub fn assert_matches(store: &mut Store, model: &Model, keys: &[Vec<u8>], context: &str) {
    assert_eq!(&contents(store), model, "scan, {context}");
    for k in keys {
        assert_eq!(store.get(k).expect("get"), model.get(k).cloned(), "get {k:?}, {context}");
    }
}

pub fn table_files(dir: &Path) -> usize {
    let names = fs::read_dir(dir).expect("read_dir").map(|e| e.expect("entry").file_name());
    names.filter(|n| n.to_string_lossy().ends_with(".sst")).count()
}
