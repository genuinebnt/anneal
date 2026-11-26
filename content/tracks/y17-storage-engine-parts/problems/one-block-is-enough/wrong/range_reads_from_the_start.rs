pub type Record = (Vec<u8>, Vec<u8>);

/// A sorted set of records cut into blocks. The first keys are at hand; a block's records are not.
pub trait Blocks {
    fn count(&self) -> usize;
    /// The first key in block `i`. Cheap, but not free.
    fn first_key(&self, i: usize) -> &[u8];
    /// All the records of block `i`, in key order. This is a disk read.
    fn fetch(&self, i: usize) -> Vec<Record>;
}

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
    let mut block = 0;
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
