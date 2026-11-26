use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

/// Answers "is this key possibly in the file?". "No" is always right; "yes" is sometimes wrong.
pub struct Filter {
    bits: Vec<u8>,
    probes: u8,
    hasher: RandomState,
}

impl Filter {
    /// A filter holding `keys`.
    pub fn new(keys: &[&[u8]]) -> Filter {
        let bytes = (keys.len() * 10).max(64).div_ceil(8);
        let mut filter = Filter {
            bits: vec![0; bytes],
            probes: 4,
            hasher: RandomState::new(),
        };
        for key in keys {
            for bit in filter.positions(key) {
                filter.bits[bit / 8] |= 1 << (bit % 8);
            }
        }
        filter
    }

    fn positions(&self, key: &[u8]) -> Vec<usize> {
        let hash = self.hasher.hash_one(key);
        let (first, step) = (hash, hash.rotate_left(31) | 1);
        let count = (self.bits.len() * 8) as u64;
        (0..self.probes as u64)
            .map(|i| (first.wrapping_add(i.wrapping_mul(step)) % count) as usize)
            .collect()
    }

    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.positions(key)
            .iter()
            .all(|&bit| self.bits[bit / 8] & (1 << (bit % 8)) != 0)
    }

    /// The bytes to put in the file.
    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![self.probes];
        out.extend((self.bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bits);
        out
    }

    /// The filter that `save` wrote, or `None` if these are not the bytes of a filter.
    pub fn load(bytes: &[u8]) -> Option<Filter> {
        let probes = bytes[0];
        let len = u32::from_le_bytes(bytes[1..5].try_into().ok()?) as usize;
        Some(Filter {
            bits: bytes[5..5 + len].to_vec(),
            probes,
            hasher: RandomState::new(),
        })
    }
}
