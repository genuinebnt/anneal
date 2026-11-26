//! What the store does NOT do. Each test passes because the limitation is real. If you fix one, its test fails: replace it
//! with a test of the new behaviour.

mod support;

use mini_kv::{crc32, KvError, Store};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use support::TempDir;

const BIG: usize = 1 << 20;

fn append_raw(tmp: &TempDir, bytes: &[u8]) {
    OpenOptions::new().append(true).open(tmp.join("wal.log")).unwrap().write_all(bytes).unwrap();
}

#[test]
fn a_zero_filled_log_tail_stops_the_store_from_opening() {
    let tmp = TempDir::new("zero_tail");
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    store.put(b"a", b"1").unwrap();
    drop(store);
    append_raw(&tmp, &[0; 64]);
    // eight zero bytes read as a frame with len 0 and crc 0, and crc32 of nothing IS 0
    assert_eq!(crc32(b""), 0);
    assert!(matches!(Store::open(&tmp.0, BIG), Err(KvError::Corrupt(_))));
}

#[test]
fn bytes_left_by_a_partial_append_hide_every_later_write() {
    // Stand-in for a `write_all` that fails part-way (a full disk, say): the junk is appended by hand.
    let tmp = TempDir::new("partial_append");
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    store.put(b"before", b"1").unwrap();
    append_raw(&tmp, &[9, 0, 0, 0, 7]); // the start of a frame
    store.put(b"after", b"2").unwrap(); // returns Ok
    drop(store);
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    assert_eq!(store.get(b"before").unwrap(), Some(b"1".to_vec()));
    assert_eq!(store.get(b"after").unwrap(), None);
}

#[test]
fn an_error_from_put_does_not_mean_the_write_did_not_happen() {
    let tmp = TempDir::new("put_error");
    let mut store = Store::open(&tmp.0, 1).unwrap(); // every write triggers a flush
    fs::create_dir(tmp.join("sst_000001.tmp")).unwrap(); // and the flush will fail
    assert!(store.put(b"k", b"v").is_err());
    assert_eq!(store.get(b"k").unwrap(), Some(b"v".to_vec()));
    drop(store);
    fs::remove_dir(tmp.join("sst_000001.tmp")).unwrap();
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    assert_eq!(store.get(b"k").unwrap(), Some(b"v".to_vec()));
}

#[test]
fn a_table_damaged_after_open_is_served_without_an_error() {
    let tmp = TempDir::new("late_damage");
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    store.put(b"k", b"value").unwrap();
    store.flush().unwrap();
    let path = store.tables()[0].path().to_path_buf();
    let mut bytes = fs::read(&path).unwrap();
    bytes[1 + 4 + 1 + 4] ^= 0x01; // first byte of the first record's value: tag, klen, "k", vlen, value
    fs::write(&path, bytes).unwrap();
    assert_eq!(store.get(b"k").unwrap(), Some(b"walue".to_vec()));
}

#[test]
fn scan_with_reversed_bounds_panics_when_the_memtable_has_entries() {
    let tmp = TempDir::new("reversed_scan");
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    assert!(store.scan(b"z", b"a").unwrap().is_empty(), "empty memtable: no panic");
    store.put(b"m", b"1").unwrap();
    // the panic comes from BTreeMap::range, which documents it
    assert!(panic::catch_unwind(AssertUnwindSafe(|| store.scan(b"z", b"a"))).is_err());
}

#[test]
fn two_stores_on_one_directory_lose_data() {
    let tmp = TempDir::new("two_writers");
    let mut first = Store::open(&tmp.0, BIG).unwrap();
    let mut second = Store::open(&tmp.0, BIG).unwrap(); // nothing stops this: there is no lock
    first.put(b"first", b"1").unwrap();
    first.flush().unwrap(); // writes sst_000001
    second.put(b"second", b"2").unwrap();
    second.flush().unwrap(); // also writes sst_000001, over the other one
    let mut store = Store::open(&tmp.0, BIG).unwrap();
    assert_eq!(store.get(b"first").unwrap(), None);
    assert_eq!(store.get(b"second").unwrap(), Some(b"2".to_vec()));
}

#[test]
fn overwriting_one_key_still_triggers_flushes() {
    // Not a bug: the threshold counts bytes written, which also bounds the log. The memtable holds one key here.
    let tmp = TempDir::new("overwrites");
    let mut store = Store::open(&tmp.0, 1000).unwrap();
    let mut longest_log = 0;
    for _ in 0..500 {
        store.put(b"same", &[7; 20]).unwrap();
        longest_log = longest_log.max(store.wal_len());
    }
    assert!(store.tables().len() > 10);
    assert!(longest_log < 1500, "{longest_log}");
}
