//! A consistent-hashing ring with virtual nodes.

use std::collections::BTreeMap;

/// A deterministic 64-bit mix of a key and a salt (so that tests do not depend on a hasher's seed).
pub fn hash64(key: u64, salt: u64) -> u64 {
    let mut x = key ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn name_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

pub struct HashRing {
    _ring: (),
}

impl HashRing {
    /// An empty ring in which each node gets `vnodes` points (at least 1).
    pub fn new(vnodes: u32) -> HashRing {
        todo!("2b-c3: an empty ring")
    }

    /// Adds node `name` at `vnodes` points on the ring; returns false if it was already there.
    pub fn add_node(&mut self, name: &str) -> bool {
        todo!("2b-c3: put the node on the ring at `vnodes` points")
    }

    /// Takes node `name` and all its points off the ring; returns whether it was there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        todo!("2b-c3: take the node's points off the ring")
    }

    /// The node owning `key`: the node at the first point at or after the key's point, wrapping around the ring. `None` for an empty ring.
    pub fn node_for(&self, key: u64) -> Option<&str> {
        todo!("2b-c3: the node at the first point at or after the key's point, wrapping")
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        todo!("2b-c3: how many nodes")
    }
}
