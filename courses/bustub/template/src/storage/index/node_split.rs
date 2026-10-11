//! Splitting a full node of a B+ tree in two: which keys go where, and which key is pushed up to the parent.

/// The result of splitting a node: the left node's keys, the separator for the parent, the right node's keys.
#[derive(Debug, PartialEq, Eq)]
pub struct Split {
    pub left: Vec<i64>,
    pub separator: i64,
    pub right: Vec<i64>,
}

/// Splits the sorted `keys` of a full node (at least 2 keys). The left node gets the first `(len + 1) / 2` keys.
///
/// - A **leaf** keeps every key (the keys live in leaves): the separator is a *copy* of the right node's first key, which stays in the right node.
/// - An **inner** node's separator is *moved up*: it is the first key that would have gone to the right, and it is in neither node.
pub fn split_keys(keys: &[i64], is_leaf: bool) -> Split {
    let mid = keys.len().div_ceil(2);
    let left = keys[..mid].to_vec();
    let separator = keys[mid];
    // TODO(2c-c2): this looks right and has a bug; find it and fix it
    let right = keys[mid..].to_vec();
    Split { left, separator, right }
}
