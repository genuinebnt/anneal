//! The model test: thousands of seeded random operations, checked against a `BTreeMap`, with simulated crashes
//! (drop the store, sometimes cut 1 to 4 bytes off the last log record, reopen).

mod support;

use mini_kv::{Key, Store};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::path::Path;
use support::{table_files, TempDir};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn rand_key(rng: &mut Rng) -> Key {
    format!("k{:03}", rng.below(200)).into_bytes()
}

/// Every key stored in a table must pass its bloom filter. Returns (false positives, probes) for absent keys.
fn check_blooms(store: &Store) -> (usize, usize) {
    let (mut fp, mut probes) = (0, 0);
    for t in store.tables() {
        for (k, _) in t.read_all().unwrap() {
            assert!(t.bloom().may_contain(&k), "bloom false negative for {k:?}");
        }
        for i in 0..200 {
            probes += 1;
            fp += t.bloom().may_contain(format!("absent-{i}").as_bytes()) as usize;
        }
    }
    (fp, probes)
}

/// The newest write, remembered while it is still only in the log, so that a crash can tear it.
struct LastMutation {
    key: Key,
    prev: Option<Vec<u8>>,
    before: u64,
    after: u64,
}

#[derive(Default, Debug)]
struct Tally {
    crashes: usize,
    torn: usize,
    flushes: usize,
    compactions: usize,
    max_tables: usize,
    fp: usize,
    probes: usize,
}

fn model_test(dir: &Path, seed: u64, ops: usize, tally: &mut Tally) {
    const THRESHOLD: usize = 1500; // tiny on purpose: forces many flushes and tables
    let mut rng = Rng(seed);
    let mut store = Store::open(dir, THRESHOLD).unwrap();
    let mut model: BTreeMap<Key, Vec<u8>> = BTreeMap::new();
    let mut last: Option<LastMutation> = None;
    for step in 0..ops {
        let at = format!("seed {seed:#x} step {step}");
        let key = rand_key(&mut rng);
        let (prev, before) = (model.get(&key).cloned(), store.wal_len());
        match rng.below(100) {
            0..65 => {
                if rng.below(65) < 45 {
                    let val: Vec<u8> = (0..rng.below(60)).map(|_| rng.next() as u8).collect();
                    store.put(&key, &val).unwrap();
                    model.insert(key.clone(), val);
                } else {
                    store.delete(&key).unwrap();
                    model.remove(&key);
                }
                // only a record still sitting in the log (no flush happened) can be torn by a crash
                let after = store.wal_len();
                last = (after > before).then_some(LastMutation { key, prev, before, after });
            }
            65..85 => assert_eq!(store.get(&key).unwrap(), model.get(&key).cloned(), "get {key:?}, {at}"),
            85..90 => {
                let (mut a, mut b) = (rand_key(&mut rng), rand_key(&mut rng));
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                let want: Vec<_> = model.range(a.clone()..b.clone()).map(|(k, v)| (k.clone(), v.clone())).collect();
                assert_eq!(store.scan(&a, &b).unwrap(), want, "scan {a:?}..{b:?}, {at}");
            }
            90..93 => {
                store.flush().unwrap();
                (tally.flushes, last) = (tally.flushes + 1, None);
            }
            93..95 => {
                let n = store.tables().len();
                store.compact().unwrap();
                if n >= 2 {
                    assert!(store.tables().len() < n, "compaction must reduce the file count, {at}");
                    assert_eq!(table_files(dir), store.tables().len(), "old files deleted, {at}");
                }
                (tally.compactions, last) = (tally.compactions + 1, None);
            }
            95..99 => {
                drop(store); // "crash": no flush, no goodbye
                let mut expect_wal = None;
                if let Some(l) = last.take().filter(|_| rng.below(2) == 0) {
                    // tear the last record only: cut 1..=4 bytes off its end (never into earlier records)
                    let cut = 1 + rng.below(4.min((l.after - l.before - 1) as usize)) as u64;
                    OpenOptions::new().write(true).open(dir.join("wal.log")).unwrap().set_len(l.after - cut).unwrap();
                    match l.prev {
                        Some(v) => model.insert(l.key, v),
                        None => model.remove(&l.key),
                    };
                    (expect_wal, tally.torn) = (Some(l.before), tally.torn + 1);
                }
                store = Store::open(dir, THRESHOLD).unwrap();
                if let Some(len) = expect_wal {
                    assert_eq!(store.wal_len(), len, "recovery truncates the torn record, {at}");
                }
                (tally.crashes, last) = (tally.crashes + 1, None);
            }
            _ => {
                let (fp, probes) = check_blooms(&store);
                (tally.fp, tally.probes) = (tally.fp + fp, tally.probes + probes);
            }
        }
        tally.max_tables = tally.max_tables.max(store.tables().len());
    }
    // final audit: reopen cleanly and compare everything
    drop(store);
    let mut store = Store::open(dir, THRESHOLD).unwrap();
    let all = store.scan(b"", b"\xff").unwrap();
    assert_eq!(all, model.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>(), "final scan");
    for i in 0..200 {
        let k = format!("k{i:03}").into_bytes();
        assert_eq!(store.get(&k).unwrap(), model.get(&k).cloned(), "final get {k:?}");
    }
    let (fp, probes) = check_blooms(&store);
    (tally.fp, tally.probes) = (tally.fp + fp, tally.probes + probes);
    let stats = store.stats();
    assert!(stats.bloom_skips + stats.table_reads > 0 || store.tables().is_empty());
    let before = store.tables().len();
    store.compact().unwrap();
    assert!(store.tables().len() <= 1 && (before < 2 || store.tables().len() < before));
}

#[test]
fn random_operations_and_crashes_match_a_btreemap() {
    let tmp = TempDir::new("model");
    let mut total = Tally::default();
    for seed in [0xC0FFEE_u64, 0xBADC0DE, 0x5EED_1234] {
        model_test(&tmp.join(&format!("seed{seed}")), seed, 7000, &mut total);
    }
    println!("{total:?}");
    // a random test that never reaches a path proves nothing about it
    assert!(total.crashes > 50 && total.torn > 10, "crash paths exercised: {total:?}");
    assert!(total.flushes > 50 && total.compactions > 50 && total.max_tables >= 3, "{total:?}");
    let fp_rate = total.fp as f64 / total.probes as f64;
    assert!(fp_rate < 0.05, "bloom false-positive rate too high: {fp_rate}");
}
