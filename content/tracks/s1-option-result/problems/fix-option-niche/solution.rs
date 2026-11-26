use std::num::NonZeroU32;

/// A binary search tree in one `Vec`. Links are indices into `nodes`; the root is `nodes[0]`.
#[derive(Debug)]
pub struct Node {
    pub key: i32,
    // The root is nodes[0] and is never a child, so a child index is never 0.
    pub left: Option<NonZeroU32>,
    pub right: Option<NonZeroU32>,
}

#[derive(Debug, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    pub fn new() -> Tree {
        Tree { nodes: Vec::new() }
    }

    /// Inserts `key`. Returns false if it was already there.
    pub fn insert(&mut self, key: i32) -> bool {
        if self.nodes.is_empty() {
            self.nodes.push(Node {
                key,
                left: None,
                right: None,
            });
            return true;
        }
        let new = u32::try_from(self.nodes.len())
            .ok()
            .and_then(NonZeroU32::new)
            .expect("too many nodes");
        let mut i = 0;
        loop {
            let node = &mut self.nodes[i];
            let link = if key < node.key {
                &mut node.left
            } else if key > node.key {
                &mut node.right
            } else {
                return false;
            };
            match *link {
                Some(next) => i = next.get() as usize,
                None => {
                    *link = Some(new);
                    break;
                }
            }
        }
        self.nodes.push(Node {
            key,
            left: None,
            right: None,
        });
        true
    }

    pub fn contains(&self, key: i32) -> bool {
        let mut link = if self.nodes.is_empty() { None } else { Some(0) };
        while let Some(i) = link {
            let node = &self.nodes[i];
            if key == node.key {
                return true;
            }
            link = (if key < node.key {
                node.left
            } else {
                node.right
            })
            .map(|n| n.get() as usize);
        }
        false
    }

    /// The keys in ascending order.
    pub fn in_order(&self) -> Vec<i32> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack = Vec::new();
        let mut link: Option<usize> = if self.nodes.is_empty() { None } else { Some(0) };
        loop {
            while let Some(i) = link {
                stack.push(i);
                link = self.nodes[i].left.map(|n| n.get() as usize);
            }
            let Some(i) = stack.pop() else { break };
            out.push(self.nodes[i].key);
            link = self.nodes[i].right.map(|n| n.get() as usize);
        }
        out
    }
}
