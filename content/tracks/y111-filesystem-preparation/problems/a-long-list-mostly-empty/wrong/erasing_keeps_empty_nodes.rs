/// The name of a node in the pool. 0 is never a node: in a slot it means "nothing".
pub type NodeId = u32;

/// Where the nodes live. There is a limited number of them.
pub trait Pool {
    /// How many slots every node has, at least 2.
    fn fan(&self) -> usize;
    /// A new node with every slot 0, or `None` if none is left.
    fn alloc(&mut self) -> Option<NodeId>;
    fn free(&mut self, node: NodeId);
    fn get(&self, node: NodeId, slot: usize) -> u32;
    fn set(&mut self, node: NodeId, slot: usize, value: u32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    OutOfRange,
    /// The pool had too few nodes left. Nothing was changed.
    Full,
}

/// A list of `fan` to the power `levels` numbers, all 0 at first, kept as a tree of pool nodes.
pub struct List {
    levels: u32,
    root: NodeId,
}

/// How many indices one slot of a node at this height covers (a node of height 1 holds the values).
fn span(fan: usize, height: u32) -> u128 {
    // in 128 bits: a slot near the root of a tall tree can cover more than a u64 counts
    (fan as u128).saturating_pow(height - 1)
}

impl List {
    /// `levels` is at least 1: the height of the tree.
    pub fn new(levels: u32) -> List {
        List { levels, root: 0 }
    }

    /// The node that holds the tree, or 0 when every value is 0.
    pub fn root(&self) -> NodeId {
        self.root
    }

    fn in_range(&self, fan: usize, index: u64) -> bool {
        (fan as u64)
            .checked_pow(self.levels)
            .is_none_or(|capacity| index < capacity)
    }

    /// The slot to follow from a node of this height.
    fn slot(fan: usize, height: u32, index: u64) -> usize {
        (u128::from(index) / span(fan, height) % fan as u128) as usize
    }

    pub fn get<P: Pool>(&self, pool: &P, index: u64) -> u32 {
        if !self.in_range(pool.fan(), index) {
            return 0;
        }
        let mut node = self.root;
        let mut height = self.levels;
        while node != 0 && height > 0 {
            node = pool.get(node, List::slot(pool.fan(), height, index));
            height -= 1;
        }
        node
    }

    pub fn set<P: Pool>(&mut self, pool: &mut P, index: u64, value: u32) -> Result<(), SetError> {
        let fan = pool.fan();
        if !self.in_range(fan, index) {
            return Err(SetError::OutOfRange);
        }
        // the nodes already on the way down, from the root
        let mut path = Vec::new();
        let mut node = self.root;
        let mut height = self.levels;
        while node != 0 {
            path.push((node, List::slot(fan, height, index)));
            if height == 1 {
                break;
            }
            node = pool.get(node, path[path.len() - 1].1);
            height -= 1;
        }
        let missing = self.levels as usize - path.len();
        if value == 0 {
            if missing > 0 {
                return Ok(()); // it is 0 already
            }
        } else {
            // take every node that is needed before linking any of them
            let mut new = Vec::new();
            for _ in 0..missing {
                match pool.alloc() {
                    Some(n) => new.push(n),
                    None => {
                        new.into_iter().for_each(|n| pool.free(n));
                        return Err(SetError::Full);
                    }
                }
            }
            for n in new {
                match path.last() {
                    Some(&(parent, slot)) => pool.set(parent, slot, n),
                    None => self.root = n,
                }
                let height = self.levels - path.len() as u32;
                path.push((n, List::slot(fan, height, index)));
            }
        }
        let (leaf, slot) = path[path.len() - 1];
        pool.set(leaf, slot, value);
        Ok(())
    }

    fn is_empty<P: Pool>(pool: &P, node: NodeId) -> bool {
        (0..pool.fan()).all(|slot| pool.get(node, slot) == 0)
    }

    /// Frees the nodes of a path that have nothing left, from the bottom up.
    fn prune<P: Pool>(&mut self, pool: &mut P, path: &[(NodeId, usize)]) {
        for i in (0..path.len()).rev() {
            let node = path[i].0;
            if !List::is_empty(pool, node) {
                return;
            }
            pool.free(node);
            match i.checked_sub(1) {
                Some(above) => pool.set(path[above].0, path[above].1, 0),
                None => self.root = 0,
            }
        }
    }

    /// Makes every value at `len` and after it 0.
    pub fn cut<P: Pool>(&mut self, pool: &mut P, len: u64) {
        if self.root != 0 && List::cut_node(pool, self.root, self.levels, 0, u128::from(len)) {
            pool.free(self.root);
            self.root = 0;
        }
    }

    /// Cuts below `node`, whose first index is `base`. Returns whether the node is left with nothing.
    fn cut_node<P: Pool>(pool: &mut P, node: NodeId, height: u32, base: u128, len: u128) -> bool {
        let width = span(pool.fan(), height);
        let mut empty = true;
        for slot in 0..pool.fan() {
            let child = pool.get(node, slot);
            if child == 0 {
                continue;
            }
            let first = base.saturating_add(width.saturating_mul(slot as u128));
            if first.saturating_add(width) <= len {
                empty = false; // wholly kept
            } else if height == 1 {
                pool.set(node, slot, 0); // a value at or after `len`
            } else if List::cut_node(pool, child, height - 1, first, len) {
                pool.free(child);
                pool.set(node, slot, 0);
            } else {
                empty = false;
            }
        }
        empty
    }
}
