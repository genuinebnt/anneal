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
        // @begin 2a-02
        self.data = [0; KEY_SIZE];
        self.data[..8].copy_from_slice(&key.to_le_bytes());
        //~ todo!("2a-02: zero the key, then store the i64 in its first 8 bytes")
        // @end
    }

    /// The first 8 bytes as an `i64`.
    pub fn get_as_integer(&self) -> i64 {
        // @begin 2a-02
        i64::from_le_bytes(self.data[..8].try_into().expect("a key is at least 8 bytes"))
        //~ todo!("2a-02: the first 8 bytes, little-endian")
        // @end
    }
}

impl<const KEY_SIZE: usize> FixedSize for GenericKey<KEY_SIZE> {
    const SIZE: usize = KEY_SIZE;
    fn encode(&self, out: &mut [u8]) {
        // @begin 2a-02
        out.copy_from_slice(&self.data);
        //~ todo!("2a-02: the key's bytes")
        // @end
    }
    fn decode(bytes: &[u8]) -> Self {
        // @begin 2a-02
        let mut key = GenericKey { data: [0; KEY_SIZE] };
        key.data.copy_from_slice(bytes);
        key
        //~ todo!("2a-02: a key made of these bytes")
        // @end
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
        // @begin 2a-02
        lhs.get_as_integer().cmp(&rhs.get_as_integer())
        //~ todo!("2a-02: compare the integers in the first 8 bytes of the keys")
        // @end
    }
}
