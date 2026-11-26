/// A binary search tree in one `Vec`. Links are indices into `nodes`; the root is `nodes[0]`.
#[derive(Debug)]
pub struct Node {
    pub key: i32,
    pub left: Option<u32>,
    pub right: Option<u32>,
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
        let new = u32::try_from(self.nodes.len()).expect("too many nodes");
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
                Some(next) => i = next as usize,
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
            let node = &self.nodes[i as usize];
            if key == node.key {
                return true;
            }
            link = if key < node.key {
                node.left
            } else {
                node.right
            };
        }
        false
    }

    /// The keys in ascending order.
    pub fn in_order(&self) -> Vec<i32> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack = Vec::new();
        let mut link = if self.nodes.is_empty() { None } else { Some(0) };
        loop {
            while let Some(i) = link {
                stack.push(i);
                link = self.nodes[i as usize].left;
            }
            let Some(i) = stack.pop() else { break };
            out.push(self.nodes[i as usize].key);
            link = self.nodes[i as usize].right;
        }
        out
    }
}
