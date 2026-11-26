//! What the store does NOT do. Each test passes because the limitation is real. If you fix one, its test fails: replace it
//! with a test of the new behaviour.

mod support;

use mini_kv::Store;
use std::fs;
use support::TempDir;

const BIG: usize = 1 << 20;

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
