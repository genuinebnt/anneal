from y17_parts.common import broken

TYPES = r'''pub type Record = (Vec<u8>, Vec<u8>);

/// A sorted set of records cut into blocks. The first keys are at hand; a block's records are not.
pub trait Blocks {
    fn count(&self) -> usize;
    /// The first key in block `i`. Cheap, but not free.
    fn first_key(&self, i: usize) -> &[u8];
    /// All the records of block `i`, in key order. This is a disk read.
    fn fetch(&self, i: usize) -> Vec<Record>;
}
'''

STARTER = TYPES + r'''
/// Cuts `records`, which are sorted by key with no key twice, into blocks.
pub fn pack(records: &[Record], max_bytes: usize) -> Vec<Vec<Record>> {
    todo!()
}

pub fn get<B: Blocks>(blocks: &B, key: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

/// The records with `from <= key < to`, in key order.
pub fn range<B: Blocks>(blocks: &B, from: &[u8], to: &[u8]) -> Vec<Record> {
    todo!()
}
'''

SOLUTION = TYPES + r'''
/// Cuts `records`, which are sorted by key with no key twice, into blocks.
pub fn pack(records: &[Record], max_bytes: usize) -> Vec<Vec<Record>> {
    let mut blocks: Vec<Vec<Record>> = Vec::new();
    let mut used = 0;
    for record in records {
        let size = record.0.len() + record.1.len();
        match blocks.last_mut() {
            Some(block) if used + size <= max_bytes => {
                block.push(record.clone());
                used += size;
            }
            _ => {
                blocks.push(vec![record.clone()]);
                used = size;
            }
        }
    }
    blocks
}

/// How many blocks start at or before `key`.
fn starting_at_or_before<B: Blocks>(blocks: &B, key: &[u8]) -> usize {
    let (mut low, mut high) = (0, blocks.count());
    while low < high {
        let mid = low + (high - low) / 2;
        if blocks.first_key(mid) <= key {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

pub fn get<B: Blocks>(blocks: &B, key: &[u8]) -> Option<Vec<u8>> {
    // the only block that can hold the key is the last one that starts at or before it
    let block = starting_at_or_before(blocks, key).checked_sub(1)?;
    let mut records = blocks.fetch(block);
    let at = records
        .binary_search_by(|r| r.0.as_slice().cmp(key))
        .ok()?;
    Some(records.swap_remove(at).1)
}

/// The records with `from <= key < to`, in key order.
pub fn range<B: Blocks>(blocks: &B, from: &[u8], to: &[u8]) -> Vec<Record> {
    let mut out = Vec::new();
    if from >= to {
        return out;
    }
    let mut block = starting_at_or_before(blocks, from).saturating_sub(1);
    while block < blocks.count() && blocks.first_key(block) < to {
        out.extend(
            blocks
                .fetch(block)
                .into_iter()
                .filter(|r| r.0.as_slice() >= from && r.0.as_slice() < to),
        );
        block += 1;
    }
    out
}
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + r'''
#[test]
fn packing_by_bytes() {
    let records = [rec("a", "11"), rec("b", "22"), rec("c", "3"), rec("d", "a long value"), rec("e", "")];
    let sizes: Vec<usize> = pack(&records, 6).iter().map(Vec::len).collect();
    check!("record sizes 3, 3, 2, 13, 1 with at most 6 bytes a block", sizes, vec![2, 1, 1, 1]);
}

#[test]
fn get_reads_one_block() {
    let m = three();
    let got = get(&m, b"h");
    check!("blocks start at b, f, m; get h", (got, taken(&m)), (Some(b"4".to_vec()), vec![1]));
}

#[test]
fn a_key_before_everything_reads_nothing() {
    let m = three();
    let got = get(&m, b"a");
    check!("get a", (got, taken(&m)), (None, vec![]));
}

#[test]
fn a_range_reads_only_blocks_that_could_matter() {
    let m = three();
    let got = range(&m, b"e", b"m");
    check!("range e..m", (got, taken(&m)), (vec![rec("f", "3"), rec("h", "4")], vec![0, 1]));
}

#[test]
fn an_empty_range_reads_nothing() {
    let m = three();
    let got = (range(&m, b"h", b"h"), range(&m, b"z", b"a"));
    check!("h..h and z..a", (got, taken(&m)), ((vec![], vec![]), vec![]));
}
'''

HIDDEN = HARNESS + r'''
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
'''

PROBLEM = dict(
    slug="one-block-is-enough", title="One block is enough", level="medium", stage="sorted-data",
    tags=["sparse index", "binary search", "sorted blocks", "range queries", "I/O cost"],
    statement=r'''
A lookup service keeps millions of sorted records on disk. Reading them is slow, so records are grouped into blocks. Only the **first key** of every block stays in memory. Today each lookup reads many blocks.

**`pack(records, max_bytes)`** cuts the records, in order, into blocks. A record's size is its key length plus its value length. A block takes records while their total size stays at most `max_bytes`. A record bigger than that gets a block of its own. No block is empty.

**`get(blocks, key)`** returns the value stored for `key`. It calls `fetch` **at most once**, and not at all when the first keys already show that the key is absent.

**`range(blocks, from, to)`** returns the records with `from <= key < to`. It fetches, in order and once each, exactly the blocks which **could** hold such a key, judging only by the first keys. An empty range (`from >= to`) fetches nothing.

`first_key` is cheap but counted. With 10 000 blocks, `get` may call it at most 32 times, and a `range` of ten records at most 48 times. Keys are compared byte by byte, as `Ord` on `[u8]` does.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "Write down, for block `i`, the smallest and largest keys it could possibly hold, knowing only the first keys. Both functions follow from that."),
        ("rust", "A binary search over `0..count()` that calls `first_key` is a short loop. `saturating_sub` and `checked_sub` express the two ways of handling a key before the first block."),
        ("edge", "A block that starts exactly at `to` holds nothing wanted. A key equal to a first key lives in that block, not the one before."),
    ],
    notes=("Block `i` can only hold keys from its first key up to the next block's first key. So a point lookup needs the last block that starts at or before the key, found by binary search. A range starts there and goes on while blocks start before `to`.", "get: O(log blocks + block); range: O(log blocks + records fetched)", "O(block)"),
    follow_up="1) A missing key still costs one block read. What could you keep in memory to avoid most of those? 2) How do you choose `max_bytes`? Name what gets better and worse as it grows. 3) The index keeps whole keys. How could it keep shorter ones and still choose the same blocks?",
    teaches=["A sorted file needs only one key per block in memory to find any record with one read.", "A range touches the blocks whose possible key interval meets it, and no others.", "Count reads in tests: a correct answer that reads everything is still wrong."],
    wrong={
        "first_key_goes_to_the_block_before": broken(SOLUTION, ("if blocks.first_key(mid) <= key {", "if blocks.first_key(mid) < key {")),
        "reads_a_block_for_a_key_before_everything": broken(SOLUTION, ("let block = starting_at_or_before(blocks, key).checked_sub(1)?;", "if blocks.count() == 0 {\n        return None;\n    }\n    let block = starting_at_or_before(blocks, key).saturating_sub(1);")),
        "range_reads_the_block_that_starts_at_to": broken(SOLUTION, ("blocks.first_key(block) < to {", "blocks.first_key(block) <= to {")),
        "range_reads_from_the_start": broken(SOLUTION, ("let mut block = starting_at_or_before(blocks, from).saturating_sub(1);", "let mut block = 0;")),
        "looks_at_every_first_key": broken(SOLUTION, ("    let (mut low, mut high) = (0, blocks.count());\n    while low < high {\n        let mid = low + (high - low) / 2;\n        if blocks.first_key(mid) <= key {\n            low = mid + 1;\n        } else {\n            high = mid;\n        }\n    }\n    low\n", "    (0..blocks.count())\n        .take_while(|&i| blocks.first_key(i) <= key)\n        .count()\n")),
        "full_means_one_byte_short": broken(SOLUTION, ("used + size <= max_bytes", "used + size < max_bytes")),
    },
)
