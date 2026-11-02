use std::collections::{BTreeMap, BTreeSet};

/// FNV-1a over the bytes, then a mixing finaliser so nearby inputs land far apart on the ring.
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

/// A consistent-hash ring with virtual nodes: each node owns `vnodes` points; a key belongs to the first point at or after its hash.
pub struct Ring {
    vnodes: usize,
    points: BTreeMap<u64, String>,
    nodes: BTreeSet<String>,
}

impl Ring {
    pub fn new(vnodes: usize) -> Self {
        Ring { vnodes: vnodes.max(1), points: BTreeMap::new(), nodes: BTreeSet::new() }
    }

    /// Adds a node (no effect if already present).
    pub fn add_node(&mut self, name: &str) {
        if !self.nodes.insert(name.to_string()) {
            return;
        }
        for i in 0..self.vnodes {
            self.points.insert(hash64(format!("{name}#{i}").as_bytes()), name.to_string());
        }
    }

    /// Removes a node and all its points; false if it was not there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        if !self.nodes.remove(name) {
            return false;
        }
        self.points.retain(|_, n| n != name);
        true
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The node responsible for `key`: the owner of the first point clockwise from the key's hash (wrapping past the top).
    pub fn get(&self, key: &str) -> Option<&str> {
        let names: Vec<&String> = self.nodes.iter().collect();
        if names.is_empty() {
            return None;
        }
        Some(names[(hash64(key.as_bytes()) % names.len() as u64) as usize].as_str())
    }

    /// Up to `n` distinct nodes walking clockwise from the key: the primary first, then its replicas.
    pub fn get_n(&self, key: &str, n: usize) -> Vec<&str> {
        let h = hash64(key.as_bytes());
        let mut out: Vec<&str> = Vec::new();
        for (_, name) in self.points.range(h..).chain(self.points.range(..h)) {
            if out.len() == n {
                break;
            }
            if !out.contains(&name.as_str()) {
                out.push(name);
            }
        }
        out
    }
}
