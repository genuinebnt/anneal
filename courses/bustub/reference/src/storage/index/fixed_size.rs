//! Values that occupy a fixed number of bytes inside a page. A type states its size and its byte encoding through a trait, so that page code can
//! be generic over the key and value types.

use crate::common::config::PageId;
use crate::common::rid::Rid;

pub trait FixedSize: Sized {
    /// How many bytes the value takes in a page.
    const SIZE: usize;

    /// Writes the value into `out`, which is exactly `SIZE` bytes.
    fn encode(&self, out: &mut [u8]);

    /// Reads a value from `bytes`, which is exactly `SIZE` bytes.
    fn decode(bytes: &[u8]) -> Self;
}

impl FixedSize for i32 {
    const SIZE: usize = 4;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-01
        out.copy_from_slice(&self.to_le_bytes());
        //~ todo!("2a-01: the little-endian bytes")
        // @end
    }
    fn decode(bytes: &[u8]) -> i32 {
        // @begin 2a-01
        i32::from_le_bytes(bytes.try_into().expect("4 bytes"))
        //~ todo!("2a-01: from little-endian bytes")
        // @end
    }
}

impl FixedSize for u32 {
    const SIZE: usize = 4;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-01
        out.copy_from_slice(&self.to_le_bytes());
        //~ todo!("2a-01: the little-endian bytes")
        // @end
    }
    fn decode(bytes: &[u8]) -> u32 {
        // @begin 2a-01
        u32::from_le_bytes(bytes.try_into().expect("4 bytes"))
        //~ todo!("2a-01: from little-endian bytes")
        // @end
    }
}

impl FixedSize for i64 {
    const SIZE: usize = 8;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-01
        out.copy_from_slice(&self.to_le_bytes());
        //~ todo!("2a-01: the little-endian bytes")
        // @end
    }
    fn decode(bytes: &[u8]) -> i64 {
        // @begin 2a-01
        i64::from_le_bytes(bytes.try_into().expect("8 bytes"))
        //~ todo!("2a-01: from little-endian bytes")
        // @end
    }
}

impl FixedSize for PageId {
    const SIZE: usize = 4;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-01
        self.0.encode(out);
        //~ todo!("2a-01: a page id is its i32")
        // @end
    }
    fn decode(bytes: &[u8]) -> PageId {
        // @begin 2a-01
        PageId(i32::decode(bytes))
        //~ todo!("2a-01: a page id is its i32")
        // @end
    }
}

impl FixedSize for Rid {
    const SIZE: usize = 8;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-01
        self.get().encode(out);
        //~ todo!("2a-01: a rid is its i64")
        // @end
    }
    fn decode(bytes: &[u8]) -> Rid {
        // @begin 2a-01
        Rid::from_i64(i64::decode(bytes))
        //~ todo!("2a-01: a rid is its i64")
        // @end
    }
}

/// A pair is its first value followed by its second. Index entries are `(key, value)` pairs.
impl<A: FixedSize, B: FixedSize> FixedSize for (A, B) {
    const SIZE: usize = A::SIZE + B::SIZE;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-02
        let (first, second) = out.split_at_mut(A::SIZE);
        self.0.encode(first);
        self.1.encode(second);
        //~ todo!("2a-02: encode the first value into the first A::SIZE bytes and the second into the rest (split_at_mut gives you both halves at once)")
        // @end
    }
    fn decode(bytes: &[u8]) -> (A, B) {
        // @begin 2a-02
        let (first, second) = bytes.split_at(A::SIZE);
        (A::decode(first), B::decode(second))
        //~ todo!("2a-02: decode a pair from its two halves")
        // @end
    }
}

/// How many entries of `entry_size` bytes fit in a page after `metadata_size` bytes of header.
pub const fn array_size(metadata_size: usize, entry_size: usize) -> usize {
    // @begin 2a-02
    (crate::common::config::BUSTUB_PAGE_SIZE - metadata_size) / entry_size
    //~ 0 // TODO(2a-02): the space left after the metadata, divided by the entry size
    // @end
}
