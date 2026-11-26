//! A bloom filter per table: "definitely not here" or "maybe here".

use crate::codec::{take, take_u32, Entry, KvError, Result};

pub struct Bloom {
    bits: Vec<u8>,
    k: u8,
}

/// Part of the on-disk format: filters are stored, so changing this hash would make old tables give false negatives.
fn fnv1a(key: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in key {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ (h >> 32) // extra mix: FNV's low bits are weak and we take a modulus
}

impl Bloom {
    /// 10 bits per key (at least 64 bits) and 4 probes. False negatives cannot happen: lookup tests the bits insert set.
    pub fn build(entries: &[Entry]) -> Bloom {
        let nbytes = (entries.len() * 10).max(64).div_ceil(8);
        let mut bloom = Bloom { bits: vec![0; nbytes], k: 4 };
        for (key, _) in entries {
            for p in bloom.probes(key).collect::<Vec<_>>() {
                bloom.bits[p / 8] |= 1 << (p % 8);
            }
        }
        bloom
    }

    /// double hashing: probe_i = h1 + i*h2 stands in for k independent hash functions, from one 64-bit hash
    fn probes(&self, key: &[u8]) -> impl Iterator<Item = usize> + use<> {
        let nbits = (self.bits.len() * 8) as u64;
        let (h1, h2) = (fnv1a(key), fnv1a(key).rotate_left(31) | 1);
        (0..self.k as u64).map(move |i| (h1.wrapping_add(i.wrapping_mul(h2)) % nbits) as usize)
    }

    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.probes(key).all(|p| self.bits[p / 8] & (1 << (p % 8)) != 0)
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.k);
        out.extend((self.bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bits);
    }

    pub(crate) fn decode(buf: &[u8]) -> Result<Bloom> {
        let mut pos = 0;
        let k = take(buf, &mut pos, 1)?[0];
        let n = take_u32(buf, &mut pos)? as usize;
        let bits = take(buf, &mut pos, n)?.to_vec();
        if bits.is_empty() || k == 0 {
            return Err(KvError::Corrupt("empty bloom"));
        }
        Ok(Bloom { bits, k })
    }
}
