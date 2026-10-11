//! Extra queries over a persistent trie, written against its public node type only.

use std::sync::Arc;

use crate::primer::trie::{Trie, TrieNode};

/// Every key in `trie` that starts with `prefix`, in increasing order.
pub fn keys_with_prefix(trie: &Trie, prefix: &str) -> Vec<String> {
    todo!("0a-c2: follow the prefix down, then collect every value below, children in order")
}

/// The number of nodes in the subtree at `node`, itself included.
fn count(node: &Arc<TrieNode>) -> usize {
    todo!("0a-c3: this node and everything below it")
}

/// The number of nodes of the trie.
pub fn node_count(trie: &Trie) -> usize {
    todo!("0a-c3: nodes reachable from the root")
}

/// The number of nodes `a` and `b` share. A node that is the same allocation shares its whole subtree; otherwise the children with the same
/// letter are compared.
fn shared(a: &Arc<TrieNode>, b: &Arc<TrieNode>) -> usize {
    todo!("0a-c3: a node that is the same allocation shares its whole subtree; otherwise compare the children with the same letter")
}

/// How many nodes the two tries have in common (the same allocation at the same path).
pub fn shared_nodes(a: &Trie, b: &Trie) -> usize {
    todo!("0a-c3: walk both tries together")
}

/// The keys of `trie` that match `pattern` (`?` is any one character), in increasing order.
pub fn keys_matching(trie: &Trie, pattern: &str) -> Vec<String> {
    todo!("0a-c5: descend letter by letter; a ? tries every child, a literal only its own")
}
