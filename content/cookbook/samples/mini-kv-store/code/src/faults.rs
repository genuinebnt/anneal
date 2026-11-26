//! Failures that cannot be caused from outside the crate: a write, a delete or a directory sync that does not work.

use crate::codec::{KvError, Result};
use crate::sstable::{sync_dir, SsTable};
use crate::store::Store;
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const BIG: usize = 1 << 20;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("mini_kv_fault_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn table_files(dir: &Path) -> usize {
    let names = fs::read_dir(dir).expect("read_dir").map(|e| e.expect("entry").file_name());
    names.filter(|n| n.to_string_lossy().ends_with(".sst")).count()
}

#[test]
fn a_partial_append_is_cut_off_and_later_writes_survive() -> Result<()> {
    for keep in [0, 1, 7, 8, 12] {
        let tmp = TempDir::new(&format!("partial_{keep}"));
        let mut store = Store::open(&tmp.0, BIG)?;
        store.put(b"before", b"1")?;
        let failed = store.wal.append_with(b"lost", Some(b"x".as_slice()), |file, frame| {
            file.write_all(&frame[..keep])?;
            Err(io::Error::other("injected: disk full"))
        });
        assert!(matches!(failed, Err(KvError::Io(_))));
        store.put(b"after", b"2")?;
        drop(store);
        let mut store = Store::open(&tmp.0, BIG)?;
        assert_eq!(store.get(b"before")?, Some(b"1".to_vec()));
        assert_eq!(store.get(b"after")?, Some(b"2".to_vec()), "{keep} bytes kept");
        assert_eq!(store.get(b"lost")?, None);
    }
    Ok(())
}

#[test]
fn a_log_that_cannot_be_repaired_takes_no_more_writes() -> Result<()> {
    let tmp = TempDir::new("stuck");
    let mut store = Store::open(&tmp.0, BIG)?;
    let log = tmp.0.join("wal.log");
    // a read-only handle makes the repair (set_len) fail
    let failed = store.wal.append_with(b"k", None, |file, _| {
        *file = File::open(&log)?;
        Err(io::Error::other("injected"))
    });
    assert!(matches!(failed, Err(KvError::Io(_))));
    assert!(matches!(store.put(b"later", b"v"), Err(KvError::LogUnusable)));
    assert_eq!(store.get(b"later")?, None);
    Ok(())
}

#[test]
fn a_failed_delete_during_compaction_changes_no_answer() -> Result<()> {
    for ok in 0..3 {
        let tmp = TempDir::new(&format!("delete_{ok}"));
        let mut store = Store::open(&tmp.0, BIG)?;
        store.put(b"gone", b"old")?;
        store.flush()?;
        store.put(b"kept", b"yes")?;
        store.flush()?;
        store.delete(b"gone")?;
        store.flush()?;
        let want = vec![(b"kept".to_vec(), b"yes".to_vec())];
        let mut done = 0;
        let failed = store.compact_with(|path| {
            if done == ok {
                return Err(io::Error::other("injected"));
            }
            done += 1;
            fs::remove_file(path)
        });
        assert!(matches!(failed, Err(KvError::Io(_))));
        assert_eq!(store.scan(b"", b"\xff")?, want, "open store, {ok} deleted");
        assert_eq!(store.tables().len(), table_files(&tmp.0));
        drop(store);
        assert_eq!(Store::open(&tmp.0, BIG)?.scan(b"", b"\xff")?, want, "reopened, {ok} deleted");
    }
    Ok(())
}

thread_local!(static SYNCS: RefCell<Vec<(usize, u64)>> = Default::default());

/// Notes (number of tables, bytes in the log) at each directory sync.
fn observed_sync(dir: &Path) -> io::Result<()> {
    let seen = (table_files(dir), fs::metadata(dir.join("wal.log"))?.len());
    SYNCS.with(|all| all.borrow_mut().push(seen));
    sync_dir(dir)
}

fn syncs_so_far() -> Vec<(usize, u64)> {
    SYNCS.with(|all| std::mem::take(&mut *all.borrow_mut()))
}

#[test]
fn the_directory_is_synced_after_the_rename_and_after_each_delete() -> Result<()> {
    let tmp = TempDir::new("sync_order");
    let mut store = Store::open(&tmp.0, BIG)?;
    store.dir_sync = observed_sync;
    let mut logs = Vec::new();
    for key in [b"a", b"b", b"c"] {
        store.put(key, b"v")?;
        logs.push(store.wal_len());
        store.flush()?;
    }
    assert_eq!(syncs_so_far(), vec![(1, logs[0]), (2, logs[1]), (3, logs[2])], "table in place, log still full");
    store.compact()?;
    let tables: Vec<usize> = syncs_so_far().iter().map(|s| s.0).collect();
    assert_eq!(tables, [4, 3, 2, 1], "the new table, then every delete");
    Ok(())
}

#[test]
fn a_failed_directory_sync_stops_the_step_that_depends_on_it() -> Result<()> {
    let tmp = TempDir::new("sync_fails");
    let mut store = Store::open(&tmp.0, BIG)?;
    store.dir_sync = |_| Err(io::Error::other("injected: sync failed"));
    store.put(b"k", b"v")?;
    let log = store.wal_len();
    assert!(matches!(store.flush(), Err(KvError::Io(_))));
    assert_eq!(store.wal_len(), log, "the table's name may not be durable: keep the log");
    assert_eq!(store.get(b"k")?, Some(b"v".to_vec()));
    store.dir_sync = sync_dir;
    store.flush()?;
    store.put(b"j", b"w")?;
    store.flush()?;
    // syncs work until the first old table is gone
    store.dir_sync = |dir| {
        if SsTable::path_for(dir, 1).exists() {
            sync_dir(dir)
        } else {
            Err(io::Error::other("injected"))
        }
    };
    let mut deletes = 0;
    let failed = store.compact_with(|path| {
        deletes += 1;
        fs::remove_file(path)
    });
    assert!(matches!(failed, Err(KvError::Io(_))));
    assert_eq!(deletes, 1, "no further delete after a failed sync");
    assert_eq!(store.tables().len(), table_files(&tmp.0));
    assert_eq!(store.get(b"k")?, Some(b"v".to_vec()));
    assert_eq!(store.get(b"j")?, Some(b"w".to_vec()));
    Ok(())
}
