//! A **persistent** (copy-on-write) trie that maps strings to values of any type. `put` and `remove` never change a trie: they return a new one
//! that shares every node they did not have to touch with the old one, so old versions stay valid and cost almost nothing to keep.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A value of any type, shared between the versions of a trie.
pub type Value = Arc<dyn Any + Send + Sync>;

/// A node: its children by next character, and a value if a key ends here.
#[derive(Clone, Default)]
pub struct TrieNode {
    pub children: BTreeMap<char, Arc<TrieNode>>,
    pub value: Option<Value>,
}

impl TrieNode {
    /// Does a key end at this node?
    pub fn is_value_node(&self) -> bool {
        self.value.is_some()
    }
}

#[derive(Clone, Default)]
pub struct Trie {
    root: Option<Arc<TrieNode>>,
}

impl Trie {
    /// An empty trie.
    pub fn new() -> Trie {
        Trie { root: None }
    }

    /// A trie with the given root. `put` is how tries are normally made; this builds a shape by hand.
    pub fn from_root(root: Option<Arc<TrieNode>>) -> Trie {
        Trie { root }
    }

    /// The root node; `None` for an empty trie.
    pub fn root(&self) -> Option<&Arc<TrieNode>> {
        self.root.as_ref()
    }

    /// The value under `key`, if there is one **of type `T`**. A key with a value of another type, a key that is only a prefix of other
    /// keys, and a missing key all give `None`.
    pub fn get<T: Any + Send + Sync>(&self, key: &str) -> Option<&T> {
        todo!("0a-01: walk from the root along the key's characters; at the end the node must have a value; downcast it to T")
    }

    /// The value under `key` as a shared pointer, for callers that must keep it alive after the trie is gone (see `TrieStore`).
    pub fn get_shared<T: Any + Send + Sync>(&self, key: &str) -> Option<Arc<T>> {
        todo!("0a-01: as get, but clone the Arc of the value and downcast it with Arc::downcast")
    }

    /// A new trie with `value` under `key` (replacing what was there, whatever its type). Only the nodes on the path from the root to
    /// the key are copied; every other node is shared with `self`. The empty key puts the value in the root.
    pub fn put<T: Any + Send + Sync>(&self, key: &str, value: T) -> Trie {
        todo!("0a-02: copy the path: a recursive helper that, given the old node (if any) and the rest of the key, returns a new node: a clone of the old one (or an empty node), with the child for the next character replaced by the result of the recursion, and the value set at the end of the key")
    }


    /// A new trie without the value under `key`. A node left with no value and no children is removed, and so on up the path, so an
    /// emptied trie has no root. A key that is not in the trie gives a trie equal to `self`.
    pub fn remove(&self, key: &str) -> Trie {
        todo!("0a-02: a recursive helper that returns the node for the new trie (or none if it became empty), or reports the key is missing; copy the nodes on the path, clear the value at the end, and drop children that became empty")
    }

}

