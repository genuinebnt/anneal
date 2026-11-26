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
