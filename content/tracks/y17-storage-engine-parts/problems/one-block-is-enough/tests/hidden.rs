use solution::*;

use std::cell::{Cell, RefCell};

struct Mem {
    blocks: Vec<Vec<Record>>,
    fetched: RefCell<Vec<usize>>,
    keys_looked_at: Cell<usize>,
}

impl Blocks for Mem {
    fn count(&self) -> usize {
        self.blocks.len()
    }
    fn first_key(&self, i: usize) -> &[u8] {
        self.keys_looked_at.set(self.keys_looked_at.get() + 1);
        &self.blocks[i][0].0
    }
    fn fetch(&self, i: usize) -> Vec<Record> {
        self.fetched.borrow_mut().push(i);
        self.blocks[i].clone()
    }
}

#[allow(dead_code)]
fn mem(blocks: Vec<Vec<Record>>) -> Mem {
    Mem { blocks, fetched: RefCell::new(Vec::new()), keys_looked_at: Cell::new(0) }
}

#[allow(dead_code)]
fn rec(key: &str, value: &str) -> Record {
    (key.as_bytes().to_vec(), value.as_bytes().to_vec())
}

/// Blocks starting at b, f and m.
#[allow(dead_code)]
fn three() -> Mem {
    mem(vec![vec![rec("b", "1"), rec("d", "2")], vec![rec("f", "3"), rec("h", "4")], vec![rec("m", "5")]])
}

#[allow(dead_code)]
fn taken(m: &Mem) -> Vec<usize> {
    std::mem::take(&mut *m.fetched.borrow_mut())
}

#[test]
fn first_keys_belong_to_their_own_block() {
    for (key, value, block) in [("b", "1", 0), ("f", "3", 1), ("m", "5", 2)] {
        let m = three();
        let got = get(&m, key.as_bytes());
        check!(format!("get {key}"), (got, taken(&m)), (Some(value.as_bytes().to_vec()), vec![block]));
    }
}

#[test]
fn missing_keys_read_at_most_one_block() {
    for (key, want) in [("c", vec![0]), ("e", vec![0]), ("g", vec![1]), ("z", vec![2])] {
        let m = three();
        let got = get(&m, key.as_bytes());
        check!(format!("get {key}"), (got, taken(&m)), (None, want));
    }
}

#[test]
fn no_blocks_at_all() {
    let m = mem(vec![]);
    check!("empty", (get(&m, b"a"), range(&m, b"a", b"z"), taken(&m)), (None, vec![], vec![]));
    check!("pack nothing", pack(&[], 10).len(), 0);
}

#[test]
fn range_edges() {
    let cases: [(&str, &str, Vec<usize>); 7] = [
        ("a", "b", vec![]),
        ("a", "c", vec![0]),
        ("f", "g", vec![1]),
        ("b", "f", vec![0]),
        ("m", "z", vec![2]),
        ("n", "z", vec![2]),
        ("a", "zz", vec![0, 1, 2]),
    ];
    for (from, to, want) in cases {
        let m = three();
        let _ = range(&m, from.as_bytes(), to.as_bytes());
        check!(format!("blocks read for {from}..{to}"), taken(&m), want);
    }
}

#[test]
fn packing_edges() {
    let records = [rec("a", "bb"), rec("c", "dd")];
    check!("exactly full", pack(&records, 6).len(), 1);
    check!("one byte short", pack(&records, 5).len(), 2);
    check!("nothing fits", pack(&records, 0).len(), 2);
    let empty = [rec("", ""), rec("a", "")];
    check!("an empty record takes no room", pack(&empty, 0), vec![vec![rec("", "")], vec![rec("a", "")]]);
}

#[test]
fn binary_keys() {
    let m = mem(pack(&[(vec![], vec![1]), (vec![0], vec![2]), (vec![0, 0], vec![3]), (vec![255], vec![4])], 0));
    check!("empty key, zero bytes and 0xff", (get(&m, &[]), get(&m, &[0]), get(&m, &[0, 0]), get(&m, &[255]), get(&m, &[1])), (Some(vec![1]), Some(vec![2]), Some(vec![3]), Some(vec![4]), None));
    taken(&m);
    check!("from the empty key to [0, 0]", (range(&m, &[], &[0, 0]).len(), taken(&m)), (2, vec![0, 1]));
}

#[test]
fn many_blocks_few_looks() {
    let records: Vec<Record> = (0..20_000u32).map(|i| ((i * 2).to_be_bytes().to_vec(), vec![1])).collect();
    let m = mem(pack(&records, 10));
    let n = m.count();
    // a few looks per halving of 10 000 blocks
    for probe in [0u32, 1, 2, 19_999, 20_000, 39_998, 39_999, 50_000] {
        m.keys_looked_at.set(0);
        let _ = get(&m, &probe.to_be_bytes());
        check!(format!("{n} blocks, get {probe}: looked at {} first keys, at most 32 allowed", m.keys_looked_at.get()), m.keys_looked_at.get() <= 32, true);
    }
    m.keys_looked_at.set(0);
    taken(&m);
    let got = range(&m, &30_000u32.to_be_bytes(), &30_020u32.to_be_bytes());
    check!(format!("a short range near the end: {} looks", m.keys_looked_at.get()), m.keys_looked_at.get() <= 48, true);
    check!("ten records, two to a block", (got.len(), taken(&m).len() <= 6), (10, true));
}

#[test]
fn random_tables_match_a_plain_search() {
    let mut rng = anneal_prelude::Rng::new(170501);
    for case in 0..500 {
        let mut set = std::collections::BTreeMap::new();
        let n = rng.below(30);
        for _ in 0..n {
            let len = rng.below(3);
            let key = rng.string(len, "abc").into_bytes();
            let vlen = rng.below(4);
            set.insert(key, rng.string(vlen, "xy").into_bytes());
        }
        let records: Vec<Record> = set.into_iter().collect();
        let max = rng.below(12);
        let blocks = pack(&records, max);
        check!(format!("case {case}: packing keeps every record in order"), blocks.concat(), records.clone());
        for (i, block) in blocks.iter().enumerate() {
            let size: usize = block.iter().map(|r| r.0.len() + r.1.len()).sum();
            check!(format!("case {case}: block {i} fits or is one record, max {max}"), !block.is_empty() && (size <= max || block.len() == 1), true);
            if let Some(next) = blocks.get(i + 1) {
                check!(format!("case {case}: block {i} had no room for the next record"), size + next[0].0.len() + next[0].1.len() > max, true);
            }
        }
        let firsts: Vec<Vec<u8>> = blocks.iter().map(|b| b[0].0.clone()).collect();
        let m = mem(blocks);
        for _ in 0..8 {
            let (a, b) = (rng.below(3), rng.below(3));
            let from = rng.string(a, "abcd").into_bytes();
            let to = rng.string(b, "abcd").into_bytes();
            let want = records.iter().find(|r| r.0 == from).map(|r| r.1.clone());
            let got = get(&m, &from);
            let reads = taken(&m);
            let expected_reads: Vec<usize> = firsts.iter().rposition(|f| *f <= from).into_iter().collect();
            check!(format!("case {case}: get {from:?} with first keys {firsts:?}"), (got, reads), (want, expected_reads));
            let want: Vec<Record> = records.iter().filter(|r| r.0 >= from && r.0 < to).cloned().collect();
            let got = range(&m, &from, &to);
            // block i can hold keys from its first key up to, not including, the next one's
            let expected_reads: Vec<usize> = (0..firsts.len()).filter(|&i| from < to && firsts[i] < to && firsts.get(i + 1).is_none_or(|next| *next > from)).collect();
            check!(format!("case {case}: range {from:?}..{to:?} with first keys {firsts:?}"), (got, taken(&m)), (want, expected_reads));
        }
    }
}
