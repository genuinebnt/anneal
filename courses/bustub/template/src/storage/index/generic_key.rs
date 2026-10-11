//! An index key that is just `KEY_SIZE` opaque bytes, and a comparator that reads the first 8 of them as a little-endian `i64`.

use std::cmp::Ordering;

use super::fixed_size::FixedSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenericKey<const KEY_SIZE: usize> {
    pub data: [u8; KEY_SIZE],
}

impl<const KEY_SIZE: usize> Default for GenericKey<KEY_SIZE> {
    fn default() -> Self {
        GenericKey { data: [0; KEY_SIZE] }
    }
}

impl<const KEY_SIZE: usize> GenericKey<KEY_SIZE> {
    /// Fills the key with zeros and stores `key` in the first 8 bytes, little-endian.
    pub fn set_from_integer(&mut self, key: i64) {
        todo!("2a-02: zero the key, then store the i64 in its first 8 bytes")
    }

    /// The first 8 bytes as an `i64`.
    pub fn get_as_integer(&self) -> i64 {
        todo!("2a-02: the first 8 bytes, little-endian")
    }
}

impl<const KEY_SIZE: usize> FixedSize for GenericKey<KEY_SIZE> {
    const SIZE: usize = KEY_SIZE;
    fn encode(&self, out: &mut [u8]) {
        todo!("2a-02: the key's bytes")
    }
    fn decode(bytes: &[u8]) -> Self {
        todo!("2a-02: a key made of these bytes")
    }
}

/// Orders keys, returning an [`Ordering`].
pub trait KeyComparator<K> {
    fn compare(&self, lhs: &K, rhs: &K) -> Ordering;
}

/// Compares `GenericKey`s by the integer in their first 8 bytes (signed).
#[derive(Clone, Copy, Debug, Default)]
pub struct GenericComparator<const KEY_SIZE: usize>;

impl<const KEY_SIZE: usize> KeyComparator<GenericKey<KEY_SIZE>> for GenericComparator<KEY_SIZE> {
    fn compare(&self, lhs: &GenericKey<KEY_SIZE>, rhs: &GenericKey<KEY_SIZE>) -> Ordering {
        todo!("2a-02: compare the integers in the first 8 bytes of the keys")
    }
}
