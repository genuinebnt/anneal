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
        todo!("2a-01: the little-endian bytes")
    }
    fn decode(bytes: &[u8]) -> i32 {
        todo!("2a-01: from little-endian bytes")
    }
}

impl FixedSize for u32 {
    const SIZE: usize = 4;
    fn encode(&self, out: &mut [u8]) {
        todo!("2a-01: the little-endian bytes")
    }
    fn decode(bytes: &[u8]) -> u32 {
        todo!("2a-01: from little-endian bytes")
    }
}

impl FixedSize for i64 {
    const SIZE: usize = 8;
    fn encode(&self, out: &mut [u8]) {
        todo!("2a-01: the little-endian bytes")
    }
    fn decode(bytes: &[u8]) -> i64 {
        todo!("2a-01: from little-endian bytes")
    }
}

impl FixedSize for PageId {
    const SIZE: usize = 4;
    fn encode(&self, out: &mut [u8]) {
        todo!("2a-01: a page id is its i32")
    }
    fn decode(bytes: &[u8]) -> PageId {
        todo!("2a-01: a page id is its i32")
    }
}

impl FixedSize for Rid {
    const SIZE: usize = 8;
    fn encode(&self, out: &mut [u8]) {
        todo!("2a-01: the page id and the slot in 8 bytes that decode reads back")
    }
    fn decode(bytes: &[u8]) -> Rid {
        todo!("2a-01: the rid that encode wrote")
    }
}

/// A pair of fixed-size values takes the two sizes added. Index entries are `(key, value)` pairs.
impl<A: FixedSize, B: FixedSize> FixedSize for (A, B) {
    const SIZE: usize = A::SIZE + B::SIZE;
    fn encode(&self, out: &mut [u8]) {
        todo!("2a-01: both values in the pair's SIZE bytes, laid out so that decode reads them back")
    }
    fn decode(bytes: &[u8]) -> (A, B) {
        todo!("2a-01: the pair that encode wrote")
    }
}

/// How many entries of `entry_size` bytes fit in a page after `metadata_size` bytes of header.
pub const fn array_size(metadata_size: usize, entry_size: usize) -> usize {
    0 // TODO(2a-01): the space left after the metadata, divided by the entry size
}
