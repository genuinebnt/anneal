//! The bytes on disk, the checksum, the merge rule and the bloom filter.

mod support;

use mini_kv::{crc32, decode_entry, encode_entry, merge_sources, Bloom, Entry, KvError, Store, FOOTER};
use std::fs;
use support::TempDir;

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn record_layout() {
    let mut out = Vec::new();
    encode_entry(b"ab", Some(b"xyz"), &mut out);
    assert_eq!(out, [1, 2, 0, 0, 0, b'a', b'b', 3, 0, 0, 0, b'x', b'y', b'z']);
    out.clear();
    encode_entry(b"ab", None, &mut out); // a tombstone
    assert_eq!(out, [0, 2, 0, 0, 0, b'a', b'b', 0, 0, 0, 0]);
    assert_eq!(decode_entry(&out, &mut 0).unwrap(), (b"ab".to_vec(), None));
    for cut in 0..out.len() {
        assert!(decode_entry(&out[..cut], &mut 0).is_err(), "a record cut at {cut} must be an error, not a panic");
    }
}

#[test]
fn merge_keeps_the_newest_version_and_drops_tombstones_only_when_asked() {
    let e = |k: &str, v: Option<&str>| -> Entry { (k.into(), v.map(Into::into)) };
    let newer = vec![e("a", Some("2")), e("b", None)];
    let older = vec![e("a", Some("1")), e("b", Some("x")), e("c", Some("3"))];
    let sources = vec![newer, older];
    assert_eq!(merge_sources(sources.clone(), true), [e("a", Some("2")), e("c", Some("3"))]);
    assert_eq!(merge_sources(sources, false), [e("a", Some("2")), e("b", None), e("c", Some("3"))]);
}

#[test]
fn every_single_byte_change_or_truncation_of_a_table_is_rejected_at_open() {
    let tmp = TempDir::new("corrupt_table");
    let mut store = Store::open(&tmp.0, 1 << 20).unwrap();
    for i in 0..40 {
        store.put(format!("k{i:02}").as_bytes(), b"value").unwrap();
    }
    store.flush().unwrap();
    let path = store.tables()[0].path().to_path_buf();
    drop(store);
    let good = fs::read(&path).unwrap();
    assert!(good.len() > FOOTER);

    for i in 0..good.len() {
        let mut bad = good.clone();
        bad[i] ^= 0xFF;
        fs::write(&path, bad).unwrap();
        assert!(matches!(Store::open(&tmp.0, 1 << 20), Err(KvError::Corrupt(_))), "changed byte {i}");
    }
    for len in 0..good.len() {
        fs::write(&path, &good[..len]).unwrap();
        assert!(matches!(Store::open(&tmp.0, 1 << 20), Err(KvError::Corrupt(_))), "truncated to {len}");
    }
    fs::write(&path, good).unwrap();
    assert!(Store::open(&tmp.0, 1 << 20).is_ok());
}

#[test]
fn bloom_has_no_false_negatives_and_about_one_percent_false_positives() {
    let entries: Vec<Entry> = (0..10_000).map(|i| (format!("key-{i}").into_bytes(), None)).collect();
    let bloom = Bloom::build(&entries);
    assert!(entries.iter().all(|(k, _)| bloom.may_contain(k)), "false negative");
    let false_positives = (0..100_000).filter(|i| bloom.may_contain(format!("absent-{i}").as_bytes())).count();
    let rate = false_positives as f64 / 100_000.0;
    println!("false positives: {false_positives} of 100000 = {:.2}%", rate * 100.0);
    // theory for 10 bits per key and 4 probes, with ideal hashes: (1 - e^(-0.4))^4 = 1.18%
    assert!((0.005..0.02).contains(&rate), "{rate}");
}
