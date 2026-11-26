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
    todo!()
}

pub fn get<B: Blocks>(blocks: &B, key: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

/// The records with `from <= key < to`, in key order.
pub fn range<B: Blocks>(blocks: &B, from: &[u8], to: &[u8]) -> Vec<Record> {
    todo!()
}
