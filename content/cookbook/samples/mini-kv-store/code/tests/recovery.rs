//! What reopening the directory gives back after each kind of interruption that these tests can produce.

mod support;

use mini_kv::Store;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use support::{assert_matches, contents, table_files, Model, TempDir};

const NO_AUTO_FLUSH: usize = 1 << 30;

/// A short history with an overwrite, deletes (one of a missing key), an empty key and an empty value.
const HISTORY: &[(&str, Option<&str>)] = &[
    ("apple", Some("red")),
    ("banana", Some("yellow")),
    ("apple", Some("green")),
    ("banana", None),
    ("", Some("the empty key")),
    ("cherry", Some("")),
    ("never-written", None),
    ("banana", Some("back")),
];

struct Log {
    bytes: Vec<u8>,
    /// `ends[i]` is the log length after the first `i` operations; `states[i]` is the contents at that point.
    ends: Vec<usize>,
    states: Vec<Model>,
}

fn write_history(tag: &str) -> Log {
    let tmp = TempDir::new(tag);
    let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    let (mut ends, mut states, mut model) = (vec![0], vec![Model::new()], Model::new());
    for &(k, v) in HISTORY {
        match v {
            Some(v) => {
                store.put(k.as_bytes(), v.as_bytes()).unwrap();
                model.insert(k.into(), v.into());
            }
            None => {
                store.delete(k.as_bytes()).unwrap();
                model.remove(k.as_bytes());
            }
        }
        ends.push(store.wal_len() as usize);
        states.push(model.clone());
    }
    let bytes = fs::read(tmp.join("wal.log")).unwrap();
    // without this, a log that is not in the file yet would make the loops over `bytes` pass by doing nothing
    assert_eq!(bytes.len(), *ends.last().unwrap(), "the whole log is in the file");
    Log { bytes, ends, states }
}

fn records_before(ends: &[usize], offset: usize) -> usize {
    ends.iter().rposition(|&end| end <= offset).unwrap()
}

#[test]
fn log_cut_at_every_byte_recovers_exactly_the_complete_records() {
    let log = write_history("cut_source");
    let tmp = TempDir::new("cut");
    for cut in 0..=log.bytes.len() {
        let dir = tmp.join(&cut.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("wal.log"), &log.bytes[..cut]).unwrap();
        let n = records_before(&log.ends, cut);

        let mut store = Store::open(&dir, NO_AUTO_FLUSH).unwrap();
        assert_eq!(contents(&store), log.states[n], "cut at byte {cut}");
        assert_eq!(store.wal_len() as usize, log.ends[n], "cut at byte {cut}: the partial record is gone");

        // a write made after recovery must survive the next reopen: it must not sit behind leftover bytes
        store.put(b"after", b"recovery").unwrap();
        drop(store);
        let mut expected = log.states[n].clone();
        expected.insert(b"after".to_vec(), b"recovery".to_vec());
        let store = Store::open(&dir, NO_AUTO_FLUSH).unwrap();
        assert_eq!(contents(&store), expected, "cut at byte {cut}, second reopen");
    }
}

#[test]
fn one_flipped_bit_anywhere_in_the_log_keeps_only_the_records_before_it() {
    let log = write_history("flip_source");
    let tmp = TempDir::new("flip");
    for byte in 0..log.bytes.len() {
        for bit in 0..8 {
            let mut damaged = log.bytes.clone();
            damaged[byte] ^= 1 << bit;
            fs::write(tmp.join("wal.log"), damaged).unwrap();
            let n = records_before(&log.ends, byte);
            let store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
            assert_eq!(contents(&store), log.states[n], "byte {byte} bit {bit}");
        }
    }
}

#[test]
fn a_flush_that_fails_leaves_the_log_intact() {
    let tmp = TempDir::new("failed_flush");
    let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    store.put(b"a", b"1").unwrap();
    store.put(b"b", b"2").unwrap();
    // a directory where the temp file should go makes the table write fail
    fs::create_dir(tmp.join("sst_000001.tmp")).unwrap();
    assert!(store.flush().is_err());
    drop(store);
    fs::remove_dir(tmp.join("sst_000001.tmp")).unwrap();
    // if the log had been reset before the table was written, both writes would be gone
    let store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    assert_eq!(contents(&store).len(), 2);
}

#[test]
fn crash_after_the_table_is_written_but_before_the_log_is_reset() {
    let tmp = TempDir::new("flush_gap");
    let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    store.put(b"a", b"1").unwrap();
    store.put(b"a", b"2").unwrap();
    store.put(b"b", b"3").unwrap();
    store.delete(b"b").unwrap();
    let log_before_flush = fs::read(tmp.join("wal.log")).unwrap();
    store.flush().unwrap();
    drop(store);
    // rebuild the on-disk state that such a crash leaves: the new table AND the whole old log
    fs::write(tmp.join("wal.log"), log_before_flush).unwrap();

    let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    let expected = Model::from([(b"a".to_vec(), b"2".to_vec())]);
    assert_matches(&mut store, &expected, &[b"a".to_vec(), b"b".to_vec()], "table and log both present");
}

#[test]
fn leftover_temp_file_is_removed_and_ignored() {
    let tmp = TempDir::new("leftover_tmp");
    let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    store.put(b"a", b"1").unwrap();
    store.flush().unwrap();
    drop(store);
    fs::write(tmp.join("sst_000002.tmp"), b"half a table").unwrap();
    let store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
    assert!(!tmp.join("sst_000002.tmp").exists());
    assert_eq!(contents(&store).len(), 1);
}

/// Four tables in which later ones overwrite and delete keys from earlier ones.
fn four_layers(store: &mut Store, model: &mut Model, keys: &[Vec<u8>]) {
    for layer in 0..4 {
        for (i, k) in keys.iter().enumerate() {
            if (i + layer) % 3 == 0 {
                store.delete(k).unwrap();
                model.remove(k);
            } else if (i + layer) % 3 == 1 {
                let v = format!("layer{layer}-{i}").into_bytes();
                store.put(k, &v).unwrap();
                model.insert(k.clone(), v);
            }
        }
        store.flush().unwrap();
    }
}

#[test]
fn crash_during_compaction_after_any_number_of_deletes() {
    let keys: Vec<Vec<u8>> = (0..40).map(|i| format!("k{i:02}").into_bytes()).collect();
    for removed in 0..=4 {
        let tmp = TempDir::new(&format!("compaction_crash_{removed}"));
        let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
        let mut model = Model::new();
        four_layers(&mut store, &mut model, &keys);

        store.crash_during_compaction(removed).unwrap();
        assert_eq!(table_files(&tmp.0), 4 - removed + 1, "the new table plus the old ones not yet deleted");

        let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
        assert_matches(&mut store, &model, &keys, &format!("crash after {removed} deletes"));
        store.compact().unwrap();
        assert_matches(&mut store, &model, &keys, "after finishing the job");
    }
}

#[test]
fn crash_while_compacting_to_nothing_does_not_bring_keys_back() {
    for removed in 0..=2 {
        let tmp = TempDir::new(&format!("compaction_empty_{removed}"));
        let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
        store.put(b"a", b"1").unwrap();
        store.flush().unwrap();
        store.delete(b"a").unwrap();
        store.flush().unwrap();
        store.crash_during_compaction(removed).unwrap();
        let mut store = Store::open(&tmp.0, NO_AUTO_FLUSH).unwrap();
        assert_matches(&mut store, &Model::new(), &[b"a".to_vec()], &format!("{removed} deletes"));
    }
}

// ---------- a real process, really killed ----------

const CHILD_DIR: &str = "MINI_KV_CRASH_DIR";

/// Operation `i`: a put of a new key, except that every 7th deletes an earlier one.
fn child_op(i: usize) -> (Vec<u8>, Option<Vec<u8>>) {
    if i % 7 == 6 {
        (format!("k{:06}", i - 3).into_bytes(), None)
    } else {
        (format!("k{i:06}").into_bytes(), Some(format!("v{i}").into_bytes()))
    }
}

fn apply_to(model: &mut Model, i: usize) {
    match child_op(i) {
        (k, Some(v)) => model.insert(k, v),
        (k, None) => model.remove(&k),
    };
}

/// Not a test of its own: the parent below runs this test binary again with `CHILD_DIR` set.
#[test]
fn crash_child() {
    let Some(dir) = std::env::var_os(CHILD_DIR) else { return };
    let mut store = Store::open(dir.as_ref(), 600).unwrap();
    let mut out = std::io::stdout();
    for i in 0..100_000 {
        match child_op(i) {
            (k, Some(v)) => store.put(&k, &v).unwrap(),
            (k, None) => store.delete(&k).unwrap(),
        }
        if i % 90 == 89 {
            store.compact().unwrap();
        }
        // printed only after the call returned: this is the acknowledgement
        writeln!(out, "ack {i}").unwrap();
        out.flush().unwrap();
    }
}

#[test]
fn killed_process_loses_no_acknowledged_write() {
    for round in 0..12 {
        let tmp = TempDir::new(&format!("kill_{round}"));
        let kill_after = 20 + round * 37;
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_child", "--nocapture"])
            .env(CHILD_DIR, &tmp.0)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut acked = None;
        // `lines` must outlive the kill: closing the pipe first would let the child die of a write error instead
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        for line in lines.by_ref() {
            // the test harness prints its own lines too; "ack N" can share a line with them
            let line = line.unwrap();
            if let Some((_, n)) = line.rsplit_once("ack ") {
                acked = Some(n.parse::<usize>().unwrap());
                if acked >= Some(kill_after) {
                    break;
                }
            }
        }
        child.kill().unwrap(); // SIGKILL on Unix: no destructors, no cleanup
        let status = child.wait().unwrap();
        #[cfg(unix)]
        assert_eq!(std::os::unix::process::ExitStatusExt::signal(&status), Some(9), "the child must die by the kill");
        let acked = acked.expect("the child acknowledged something");
        assert!(acked >= kill_after, "the child stopped early, at {acked}");

        let store = Store::open(&tmp.0, 600).unwrap();
        let recovered = contents(&store);
        // The child ran on while we were killing it, so more than `acked` may be there. What is there must be the state
        // after some number of operations `n`, with every acknowledged one included (n > acked).
        let mut model = Model::new();
        let found = (0..100_000).any(|i| {
            apply_to(&mut model, i);
            i >= acked && model == recovered
        });
        assert!(
            found,
            "round {round}: recovered state is not a prefix that includes all {} acknowledged writes",
            acked + 1
        );
    }
}
