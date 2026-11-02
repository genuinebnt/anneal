use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        use std::collections::BTreeMap;

        /// The same operations applied to a plain map: the oracle.
        #[derive(Clone, Debug)]
        enum Step {
            Put(Vec<u8>, Vec<u8>),
            Del(Vec<u8>),
        }

        fn model(steps: &[Step]) -> BTreeMap<Vec<u8>, Vec<u8>> {
            let mut m = BTreeMap::new();
            for s in steps {
                match s {
                    Step::Put(k, v) => {
                        m.insert(k.clone(), v.clone());
                    }
                    Step::Del(k) => {
                        m.remove(k);
                    }
                }
            }
            m
        }

        fn contents(s: &Store) -> BTreeMap<Vec<u8>, Vec<u8>> {
            let mut out = BTreeMap::new();
            for k in 0u8..=255 {
                for key in [vec![k], vec![k, k], vec![b'k', k]] {
                    if let Some(v) = s.get(&key) {
                        out.insert(key, v.to_vec());
                    }
                }
            }
            out
        }

        /// Builds a store from the steps and returns it with the log length after each step.
        fn build(steps: &[Step]) -> (Store, Vec<usize>) {
            let mut s = Store::new();
            let mut ends = vec![0];
            for st in steps {
                match st {
                    Step::Put(k, v) => s.put(k, v),
                    Step::Del(k) => s.delete(k),
                }
                ends.push(s.log().len());
            }
            (s, ends)
        }

        fn sample_steps() -> Vec<Step> {
            vec![
                Step::Put(vec![1], b"one".to_vec()),
                Step::Put(vec![2, 2], b"two".to_vec()),
                Step::Put(vec![1], b"uno".to_vec()),
                Step::Del(vec![2, 2]),
                Step::Put(vec![b'k', 9], Vec::new()),
                Step::Put(vec![3], vec![7u8; 40]),
                Step::Del(vec![42]),
            ]
        }

        #[test]
        fn every_truncation_point_recovers_a_consistent_prefix() {
            let steps = sample_steps();
            let (s, ends) = build(&steps);
            let log = s.log().to_vec();
            for cut in 0..=log.len() {
                let (r, n) = Store::recover(&log[..cut]);
                // the number of complete records inside the first `cut` bytes
                let complete = ends.iter().rposition(|&e| e <= cut).unwrap();
                check!(format!("cut {cut}: valid bytes"), n, ends[complete]);
                check!(format!("cut {cut}: contents"), contents(&r), model(&steps[..complete]));
                check!(format!("cut {cut}: the store's own log is the valid prefix"), r.log().to_vec(), log[..n].to_vec());
            }
        }

        #[test]
        fn corrupting_any_byte_never_applies_the_damaged_record_or_later_ones() {
            let steps = sample_steps();
            let (s, ends) = build(&steps);
            let log = s.log().to_vec();
            for pos in 0..log.len() {
                for flip in [0x01u8, 0x80, 0xFF] {
                    let mut bad = log.clone();
                    bad[pos] ^= flip;
                    let (r, n) = Store::recover(&bad);
                    let record = ends.iter().rposition(|&e| e <= pos).unwrap(); // index of the damaged record
                    check!(format!("byte {pos} ^ {flip:#x}: valid bytes stop at the damaged record"), n, ends[record]);
                    check!(format!("byte {pos}: contents are the state before it"), contents(&r), model(&steps[..record]));
                }
            }
        }

        #[test]
        fn writes_after_a_torn_tail_append_cleanly() {
            let steps = sample_steps();
            let (s, ends) = build(&steps);
            let torn = &s.log()[..ends[3] + 5]; // three whole records and 5 stray bytes
            let (mut r, n) = Store::recover(torn);
            check!("valid prefix", n, ends[3]);
            r.put(b"new", b"value");
            let (again, m) = Store::recover(r.log());
            check!("the new record is recovered, nothing is lost", m, r.log().len());
            check!("contents", again.get(b"new").map(|v| v.to_vec()), Some(b"value".to_vec()));
            check!("and the earlier records are still there", again.get(&[1]).map(|v| v.to_vec()), Some(b"uno".to_vec()));
        }

        #[test]
        fn absurd_lengths_do_not_panic_or_allocate() {
            // a 12-byte header that claims a 4 GB key and a 4 GB value, with a checksum that cannot match
            let mut log = vec![0u8; 12];
            log[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
            log[8..12].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
            let (r, n) = within(5000, move || { let (r, n) = Store::recover(&log); (r.len(), n) }).expect("recovery must be quick");
            check!("nothing recovered", (r, n), (0, 0));
        }

        #[test]
        fn compaction_keeps_exactly_the_live_data() {
            let mut s = Store::new();
            for round in 0..50u8 {
                for k in 0..10u8 {
                    s.put(&[k], &[round, k]);
                }
            }
            for k in 0..5u8 {
                s.delete(&[k]);
            }
            let before = s.log().len();
            let live: Vec<(u8, Vec<u8>)> = (5..10u8).map(|k| (k, vec![49, k])).collect();
            s.compact();
            check!("the log shrank a lot", s.log().len() < before / 20, true);
            check!("five records of 13 + 3 bytes", s.log().len(), 5 * (12 + 1 + 2));
            check!("still the same keys", s.len(), 5);
            for (k, v) in &live {
                check!(format!("key {k}"), s.get(&[*k]).map(|x| x.to_vec()), Some(v.clone()));
            }
            let (r, n) = Store::recover(s.log());
            check!("a compacted log recovers", (r.len(), n == s.log().len()), (5, true));
            check!("no tombstones survive", s.log().windows(4).all(|w| w != [255, 255, 255, 255]), true);
        }

#[test]
fn empty_value_is_not_a_delete() {
    check!(r#"vlen 0 vs tombstone"#, { let mut s = Store::new(); s.put(b"k", b""); let a = s.get(b"k").map(|v| v.len()); let (r, _) = Store::recover(s.log()); let b = r.get(b"k").map(|v| v.len()); s.delete(b"k"); (a, b, s.get(b"k").is_none()) }, (Some(0), Some(0), true));
}

#[test]
fn empty_key_is_allowed() {
    check!(r#"klen 0"#, { let mut s = Store::new(); s.put(b"", b"v"); let (r, n) = Store::recover(s.log()); (r.get(b"").map(|v| v.to_vec()), n == s.log().len()) }, (Some(b"v".to_vec()), true));
}

#[test]
fn deleting_a_missing_key_is_logged_and_harmless() {
    check!(r#"a tombstone for nothing"#, { let mut s = Store::new(); s.delete(b"ghost"); let (r, n) = Store::recover(s.log()); (s.log().len() > 0, r.len(), n == s.log().len()) }, (true, 0, true));
}

#[test]
fn delete_then_put_again() {
    check!(r#"the key comes back"#, { let mut s = Store::new(); s.put(b"k", b"1"); s.delete(b"k"); s.put(b"k", b"2"); let (r, _) = Store::recover(s.log()); r.get(b"k").map(|v| v.to_vec()) }, Some(b"2".to_vec()));
}

#[test]
fn empty_log_recovers_to_an_empty_store() {
    check!(r#"nothing to replay"#, { let (r, n) = Store::recover(&[]); (r.len(), n, r.log().len()) }, (0, 0, 0));
}

#[test]
fn big_values() {
    check!(r#"100 KB"#, { let mut s = Store::new(); s.put(b"big", &vec![9u8; 100_000]); let (r, n) = Store::recover(s.log()); (r.get(b"big").map(|v| v.len()), n == s.log().len()) }, (Some(100_000), true));
}

#[test]
fn recover_of_a_log_with_trailing_garbage_keeps_the_prefix() {
    check!(r#"7 stray bytes"#, { let mut s = Store::new(); s.put(b"a", b"1"); let good = s.log().len(); let mut l = s.log().to_vec(); l.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7]); let (r, n) = Store::recover(&l); (r.len(), n == good, r.log().len() == good) }, (1, true, true));
}
