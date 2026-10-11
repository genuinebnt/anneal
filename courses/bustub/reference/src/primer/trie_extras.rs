//! Extra queries over your persistent trie, written against its public node type only.

use std::sync::Arc;

use crate::primer::trie::{Trie, TrieNode};

/// Every key in `trie` that starts with `prefix`, in increasing order.
pub fn keys_with_prefix(trie: &Trie, prefix: &str) -> Vec<String> {
    // @begin 0a-c2
    fn walk(node: &Arc<TrieNode>, path: &mut String, out: &mut Vec<String>) {
        if node.value.is_some() {
            out.push(path.clone());
        }
        for (c, child) in &node.children {
            path.push(*c);
            walk(child, path, out);
            path.pop();
        }
    }
    let Some(mut node) = trie.root() else { return Vec::new() };
    for c in prefix.chars() {
        match node.children.get(&c) {
            Some(child) => node = child,
            None => return Vec::new(),
        }
    }
    let mut out = Vec::new();
    walk(node, &mut prefix.to_owned(), &mut out);
    out
    //~ todo!("0a-c2: follow the prefix down, then collect every value below, children in order")
    // @end
}

/// The number of nodes in the subtree at `node`, itself included.
fn count(node: &Arc<TrieNode>) -> usize {
    // @begin 0a-c3
    1 + node.children.values().map(count).sum::<usize>()
    //~ todo!("0a-c3: this node and everything below it")
    // @end
}

/// The number of nodes of the trie.
pub fn node_count(trie: &Trie) -> usize {
    // @begin 0a-c3
    trie.root().map_or(0, count)
    //~ todo!("0a-c3: nodes reachable from the root")
    // @end
}

/// The number of nodes `a` and `b` share. A node that is the same allocation shares its whole subtree; otherwise the children with the same
/// letter are compared.
fn shared(a: &Arc<TrieNode>, b: &Arc<TrieNode>) -> usize {
    // @begin 0a-c3
    if Arc::ptr_eq(a, b) {
        return count(a);
    }
    a.children.iter().map(|(c, ca)| b.children.get(c).map_or(0, |cb| shared(ca, cb))).sum()
    //~ todo!("0a-c3: a node that is the same allocation shares its whole subtree; otherwise compare the children with the same letter")
    // @end
}

/// How many nodes the two tries have in common (the same allocation at the same path).
pub fn shared_nodes(a: &Trie, b: &Trie) -> usize {
    // @begin 0a-c3
    match (a.root(), b.root()) {
        (Some(x), Some(y)) => shared(x, y),
        _ => 0,
    }
    //~ todo!("0a-c3: walk both tries together")
    // @end
}

/// The keys of `trie` that match `pattern` (`?` is any one character), in increasing order.
pub fn keys_matching(trie: &Trie, pattern: &str) -> Vec<String> {
    // @begin 0a-c5
    fn go(node: &Arc<TrieNode>, pat: &[char], path: &mut String, out: &mut Vec<String>) {
        match pat.split_first() {
            None => {
                if node.value.is_some() {
                    out.push(path.clone());
                }
            }
            Some((&'?', rest)) => {
                for (c, child) in &node.children {
                    path.push(*c);
                    go(child, rest, path, out);
                    path.pop();
                }
            }
            Some((c, rest)) => {
                if let Some(child) = node.children.get(c) {
                    path.push(*c);
                    go(child, rest, path, out);
                    path.pop();
                }
            }
        }
    }
    let pat: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    if let Some(root) = trie.root() {
        go(root, &pat, &mut String::new(), &mut out);
    }
    out
    //~ todo!("0a-c5: descend letter by letter; a ? tries every child, a literal only its own")
    // @end
}
