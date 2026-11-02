/// FNV-1a over the bytes, then a mixing finaliser so nearby inputs land far apart on the ring. (Given: use it for keys and for points.)
pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// A consistent-hash ring with virtual nodes: each node owns `vnodes` points (hash of `"{name}#{i}"`);
/// a key belongs to the first point at or after its hash, wrapping around past the top.
pub struct Ring {
    // TODO: an ordered map from point to node name, and the set of nodes
}

impl Ring {
    pub fn new(vnodes: usize) -> Self {
        todo!()
    }

    /// Adds a node (no effect if already present).
    pub fn add_node(&mut self, name: &str) {
        todo!()
    }

    /// Removes a node and all its points; false if it was not there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        todo!()
    }

    pub fn node_count(&self) -> usize {
        todo!()
    }

    /// The node responsible for `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        todo!()
    }

    /// Up to `n` distinct nodes walking clockwise from the key: the primary first, then its replicas.
    pub fn get_n(&self, key: &str, n: usize) -> Vec<&str> {
        todo!()
    }
}
