/// Answers "is this key possibly in the file?". "No" is always right; "yes" is sometimes wrong.
pub struct Filter {
    bits: Vec<u8>,
    probes: u8,
}

/// FNV-1a, 64 bit, then the high half folded into the low half. This function is part of the file format.
fn hash(key: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &byte in key {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ (h >> 32)
}

impl Filter {
    /// A filter holding `keys`.
    pub fn new(keys: &[&[u8]]) -> Filter {
        let bytes = (keys.len() * 10).max(64).div_ceil(8);
        let mut filter = Filter {
            bits: vec![0; bytes],
            probes: 4,
        };
        for key in keys {
            for bit in filter.positions(key) {
                filter.bits[bit / 8] |= 1 << (bit % 8);
            }
        }
        filter
    }

    fn positions(&self, key: &[u8]) -> Vec<usize> {
        let hash = hash(key);
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
        let (&probes, rest) = bytes.split_first()?;
        let len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let bits = &rest[4..4 + len];
        if probes == 0 || len == 0 || bits.len() != len {
            return None;
        }
        Some(Filter {
            bits: bits.to_vec(),
            probes,
        })
    }
}
