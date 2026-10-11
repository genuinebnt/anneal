//! Hashes an index key with MurmurHash3 (x64, 128-bit, seed 0) and keeps the first 64 bits. The hash is of the key's page encoding.

use super::murmur3::murmur_hash3_x64_128;
use crate::storage::index::fixed_size::FixedSize;

pub struct HashFunction<K> {
    _key: std::marker::PhantomData<K>,
}

impl<K> Default for HashFunction<K> {
    fn default() -> Self {
        HashFunction { _key: std::marker::PhantomData }
    }
}

impl<K: FixedSize> HashFunction<K> {
    pub fn new() -> HashFunction<K> {
        HashFunction::default()
    }

    /// The first 64 bits of the key's MurmurHash3.
    pub fn get_hash(&self, key: &K) -> u64 {
        // @begin 2b-01
        let mut bytes = vec![0u8; K::SIZE];
        key.encode(&mut bytes);
        murmur_hash3_x64_128(&bytes, 0)[0]
        //~ todo!("2b-02: encode the key to its bytes, hash them with murmur_hash3_x64_128 (seed 0), keep the first half")
        // @end
    }
}
