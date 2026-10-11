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
    _merkle: (),
}

impl MerkleTree {
    /// The tree over `leaves`: the leaves are hashed, then neighbours are paired level by level; an odd one out is paired with itself. An empty tree has
    /// no levels.
    pub fn new(leaves: &[Vec<u8>]) -> MerkleTree {
        todo!("0d-c3: hash the leaves, then pair neighbours level by level; an odd one out is paired with itself")
    }

    /// The hash at the top, which changes if any leaf does; 0 for no leaves.
    pub fn root(&self) -> u64 {
        todo!("0d-c3: the single hash at the top (0 for no leaves)")
    }

    /// The number of leaves.
    pub fn leaves(&self) -> usize {
        todo!("0d-c3: how many leaves")
    }

    /// `(sibling hash, sibling is on the right)` from the leaf up to (not including) the root.
    pub fn proof(&self, index: usize) -> Option<Vec<(u64, bool)>> {
        todo!("0d-c3: at each level the neighbour of the current node, and which side it is on")
    }
}

/// Does `leaf` with `proof` lead to `root`?
pub fn verify(root: u64, leaf: &[u8], proof: &[(u64, bool)]) -> bool {
    todo!("0d-c3: fold the leaf hash with each sibling in order")
}
