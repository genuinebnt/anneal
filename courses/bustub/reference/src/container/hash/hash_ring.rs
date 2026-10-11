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
    // @begin 2b-c3
    vnodes: u32,
    /// point on the ring -> node name
    points: BTreeMap<u64, String>,
    nodes: std::collections::BTreeSet<String>,
    //~ _ring: (),
    // @end
}

impl HashRing {
    /// An empty ring in which each node gets `vnodes` points (at least 1).
    pub fn new(vnodes: u32) -> HashRing {
        // @begin 2b-c3
        HashRing { vnodes: vnodes.max(1), points: BTreeMap::new(), nodes: Default::default() }
        //~ todo!("2b-c3: an empty ring")
        // @end
    }

    /// Adds node `name` at `vnodes` points on the ring; returns false if it was already there.
    pub fn add_node(&mut self, name: &str) -> bool {
        // @begin 2b-c3
        if !self.nodes.insert(name.to_owned()) {
            return false;
        }
        for i in 0..self.vnodes {
            self.points.insert(hash64(name_hash(name), i as u64), name.to_owned());
        }
        true
        //~ todo!("2b-c3: put the node on the ring at `vnodes` points")
        // @end
    }

    /// Takes node `name` and all its points off the ring; returns whether it was there.
    pub fn remove_node(&mut self, name: &str) -> bool {
        // @begin 2b-c3
        if !self.nodes.remove(name) {
            return false;
        }
        self.points.retain(|_, n| n != name);
        true
        //~ todo!("2b-c3: take the node's points off the ring")
        // @end
    }

    /// The node owning `key`: the node at the first point at or after the key's point, wrapping around the ring. `None` for an empty ring.
    pub fn node_for(&self, key: u64) -> Option<&str> {
        // @begin 2b-c3
        let h = hash64(key, 0xA5A5);
        self.points.range(h..).next().or_else(|| self.points.iter().next()).map(|(_, n)| n.as_str())
        //~ todo!("2b-c3: the node at the first point at or after the key's point, wrapping")
        // @end
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        // @begin 2b-c3
        self.nodes.len()
        //~ todo!("2b-c3: how many nodes")
        // @end
    }
}
