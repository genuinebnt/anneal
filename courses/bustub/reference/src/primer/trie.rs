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
        // @begin 0a-01
        let mut node = self.root.as_ref()?;
        for c in key.chars() {
            node = node.children.get(&c)?;
        }
        node.value.as_ref()?.downcast_ref::<T>()
        //~ todo!("0a-01: walk from the root along the key's characters; at the end the node must have a value; downcast it to T")
        // @end
    }

    /// The value under `key` as a shared pointer, for callers that must keep it alive after the trie is gone (see `TrieStore`).
    pub fn get_shared<T: Any + Send + Sync>(&self, key: &str) -> Option<Arc<T>> {
        // @begin 0a-01
        let mut node = self.root.as_ref()?;
        for c in key.chars() {
            node = node.children.get(&c)?;
        }
        node.value.clone()?.downcast::<T>().ok()
        //~ todo!("0a-01: as get, but clone the Arc of the value and downcast it with Arc::downcast")
        // @end
    }

    /// A new trie with `value` under `key` (replacing what was there, whatever its type). Only the nodes on the path from the root to
    /// the key are copied; every other node is shared with `self`. The empty key puts the value in the root.
    pub fn put<T: Any + Send + Sync>(&self, key: &str, value: T) -> Trie {
        // @begin 0a-02
        let chars: Vec<char> = key.chars().collect();
        let value: Value = Arc::new(value);
        Trie { root: Some(Self::put_at(self.root.as_ref(), &chars, value)) }
        //~ todo!("0a-02: a new trie that copies only the path to the key and shares the rest; the empty key sets the root's value")
        // @end
    }

    // @begin 0a-02
    fn put_at(node: Option<&Arc<TrieNode>>, key: &[char], value: Value) -> Arc<TrieNode> {
        let mut new = node.map(|n| (**n).clone()).unwrap_or_default();
        match key.split_first() {
            None => new.value = Some(value),
            Some((c, rest)) => {
                let child = Self::put_at(new.children.get(c), rest, value);
                new.children.insert(*c, child);
            }
        }
        Arc::new(new)
    }
    // @end

    /// A new trie without the value under `key`. A node left with no value and no children is removed, and so on up the path, so an
    /// emptied trie has no root. A key that is not in the trie gives a trie equal to `self`.
    pub fn remove(&self, key: &str) -> Trie {
        // @begin 0a-02
        let chars: Vec<char> = key.chars().collect();
        let Some(root) = &self.root else { return self.clone() };
        match Self::remove_at(root, &chars) {
            Removal::Missing => self.clone(),
            Removal::Replaced(node) => Trie { root: node },
        }
        //~ todo!("0a-02: a new trie without the key's value, pruning nodes left with no value and no children; a missing key leaves the trie equal")
        // @end
    }

    // @begin 0a-02
    fn remove_at(node: &Arc<TrieNode>, key: &[char]) -> Removal {
        let mut new = (**node).clone();
        match key.split_first() {
            None => {
                if new.value.is_none() {
                    return Removal::Missing;
                }
                new.value = None;
            }
            Some((c, rest)) => {
                let Some(child) = node.children.get(c) else { return Removal::Missing };
                match Self::remove_at(child, rest) {
                    Removal::Missing => return Removal::Missing,
                    Removal::Replaced(Some(c2)) => {
                        new.children.insert(*c, c2);
                    }
                    Removal::Replaced(None) => {
                        new.children.remove(c);
                    }
                }
            }
        }
        if new.value.is_none() && new.children.is_empty() {
            Removal::Replaced(None)
        } else {
            Removal::Replaced(Some(Arc::new(new)))
        }
    }
    // @end
}

// @begin 0a-02
enum Removal {
    /// No value under the key: nothing to do.
    Missing,
    /// The new node for this position (`None`: nothing left of it).
    Replaced(Option<Arc<TrieNode>>),
}
// @end
