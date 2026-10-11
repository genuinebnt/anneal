//! A Merkle tree over byte strings.

fn fnv(data: &[u8], seed: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// The hash of a leaf.
pub fn leaf_hash(data: &[u8]) -> u64 {
    fnv(data, 0x00)
}

/// The hash of an inner node from its children.
pub fn node_hash(left: u64, right: u64) -> u64 {
    let mut buf = [0u8; 16];
    buf[..8].copy_from_slice(&left.to_le_bytes());
    buf[8..].copy_from_slice(&right.to_le_bytes());
    fnv(&buf, 0x01)
}

pub struct MerkleTree {
    // @begin 0d-c3
    /// levels[0] are the leaf hashes; the last level has one entry (the root); empty for no leaves.
    levels: Vec<Vec<u64>>,
    //~ _merkle: (),
    // @end
}

impl MerkleTree {
    /// The tree over `leaves`: the leaves are hashed, then neighbours are paired level by level; an odd one out is paired with itself. An empty tree has
    /// no levels.
    pub fn new(leaves: &[Vec<u8>]) -> MerkleTree {
        // @begin 0d-c3
        if leaves.is_empty() {
            return MerkleTree { levels: Vec::new() };
        }
        let mut levels = vec![leaves.iter().map(|l| leaf_hash(l)).collect::<Vec<u64>>()];
        while levels.last().unwrap().len() > 1 {
            let prev = levels.last().unwrap();
            let next: Vec<u64> = prev.chunks(2).map(|c| node_hash(c[0], *c.get(1).unwrap_or(&c[0]))).collect();
            levels.push(next);
        }
        MerkleTree { levels }
        //~ todo!("0d-c3: hash the leaves, then pair neighbours level by level; an odd one out is paired with itself")
        // @end
    }

    /// The hash at the top, which changes if any leaf does; 0 for no leaves.
    pub fn root(&self) -> u64 {
        // @begin 0d-c3
        self.levels.last().map_or(0, |l| l[0])
        //~ todo!("0d-c3: the single hash at the top (0 for no leaves)")
        // @end
    }

    /// The number of leaves.
    pub fn leaves(&self) -> usize {
        // @begin 0d-c3
        self.levels.first().map_or(0, Vec::len)
        //~ todo!("0d-c3: how many leaves")
        // @end
    }

    /// `(sibling hash, sibling is on the right)` from the leaf up to (not including) the root.
    pub fn proof(&self, index: usize) -> Option<Vec<(u64, bool)>> {
        // @begin 0d-c3
        if index >= self.leaves() {
            return None;
        }
        let mut i = index;
        let mut out = Vec::new();
        for level in &self.levels[..self.levels.len() - 1] {
            let sib = if i % 2 == 0 { (*level.get(i + 1).unwrap_or(&level[i]), true) } else { (level[i - 1], false) };
            out.push(sib);
            i /= 2;
        }
        Some(out)
        //~ todo!("0d-c3: at each level the neighbour of the current node, and which side it is on")
        // @end
    }
}

/// Does `leaf` with `proof` lead to `root`?
pub fn verify(root: u64, leaf: &[u8], proof: &[(u64, bool)]) -> bool {
    // @begin 0d-c3
    let mut h = leaf_hash(leaf);
    for &(sib, on_right) in proof {
        h = if on_right { node_hash(h, sib) } else { node_hash(sib, h) };
    }
    h == root
    //~ todo!("0d-c3: fold the leaf hash with each sibling in order")
    // @end
}
