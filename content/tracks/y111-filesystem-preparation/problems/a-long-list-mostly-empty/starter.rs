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

impl List {
    /// `levels` is at least 1: the height of the tree.
    pub fn new(levels: u32) -> List {
        todo!()
    }

    /// The node that holds the tree, or 0 when every value is 0.
    pub fn root(&self) -> NodeId {
        todo!()
    }

    pub fn get<P: Pool>(&self, pool: &P, index: u64) -> u32 {
        todo!()
    }

    pub fn set<P: Pool>(&mut self, pool: &mut P, index: u64, value: u32) -> Result<(), SetError> {
        todo!()
    }

    /// Makes every value at `len` and after it 0.
    pub fn cut<P: Pool>(&mut self, pool: &mut P, len: u64) {
        todo!()
    }
}
