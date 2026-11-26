#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

use smallvec::SmallVec;

/// Nearly every block has at most 4 edges each way (a goto, a branch, a small switch). Four `u32`s are
/// 16 bytes: the same space as the heap pointer and length they share a union with, so the inline
/// capacity costs nothing over a `Vec`'s 24 bytes, and a switch with more targets spills to the heap.
type Edges = SmallVec<[BlockId; 4]>;

/// A basic block: a run of statements and its control-flow edges, in the order they were added.
pub struct Block {
    pub stmts: u32,
    succs: Edges,
    preds: Edges,
}

/// A function's control-flow graph.
pub struct Cfg {
    blocks: Vec<Block>,
}

impl Cfg {
    pub fn new() -> Cfg {
        Cfg { blocks: Vec::new() }
    }

    pub fn with_capacity(blocks: usize) -> Cfg {
        Cfg {
            blocks: Vec::with_capacity(blocks),
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn add_block(&mut self, stmts: u32) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(Block {
            stmts,
            succs: Edges::new(),
            preds: Edges::new(),
        });
        id
    }

    /// Adds the edge `from -> to`, unless it's already there.
    pub fn add_edge(&mut self, from: BlockId, to: BlockId) {
        if self.blocks[from.0 as usize].succs.contains(&to) {
            return;
        }
        self.blocks[from.0 as usize].succs.push(to);
        self.blocks[to.0 as usize].preds.push(from);
    }

    pub fn stmts(&self, b: BlockId) -> u32 {
        self.blocks[b.0 as usize].stmts
    }

    pub fn succs(&self, b: BlockId) -> &[BlockId] {
        &self.blocks[b.0 as usize].succs
    }

    pub fn preds(&self, b: BlockId) -> &[BlockId] {
        &self.blocks[b.0 as usize].preds
    }

    /// Depth-first from `entry`, successors in order; the reverse of the post-order. Unreachable blocks
    /// are left out.
    pub fn reverse_postorder(&self, entry: BlockId) -> Vec<BlockId> {
        let mut seen = vec![false; self.blocks.len()];
        let mut post = Vec::new();
        let mut stack = vec![(entry, 0)];
        seen[entry.0 as usize] = true;
        while let Some((b, next)) = stack.last_mut() {
            let b = *b;
            if let Some(&s) = self.blocks[b.0 as usize].succs.get(*next) {
                *next += 1;
                if !seen[s.0 as usize] {
                    seen[s.0 as usize] = true;
                    stack.push((s, 0));
                }
            } else {
                post.push(b);
                stack.pop();
            }
        }
        post.reverse();
        post
    }

    /// Splits every critical edge: an edge from a block with several successors to a block with several
    /// predecessors gets a new empty block (0 statements) on it. The new block takes the edge's place in
    /// both lists. Blocks are visited in id order and successors in order; returns how many were split.
    pub fn split_critical_edges(&mut self) -> usize {
        let original = self.blocks.len();
        let mut split = 0;
        for from in 0..original {
            if self.blocks[from].succs.len() < 2 {
                continue;
            }
            for i in 0..self.blocks[from].succs.len() {
                let to = self.blocks[from].succs[i];
                if self.blocks[to.0 as usize].preds.len() < 2 {
                    continue;
                }
                let mid = BlockId(self.blocks.len() as u32);
                let mut succs = Edges::new();
                succs.push(to);
                let mut preds = Edges::new();
                preds.push(BlockId(from as u32));
                self.blocks.push(Block {
                    stmts: 0,
                    succs,
                    preds,
                });
                self.blocks[from].succs[i] = mid;
                let to_preds = &mut self.blocks[to.0 as usize].preds;
                let at = to_preds
                    .iter()
                    .position(|&p| p.0 as usize == from)
                    .expect("edges are kept in both lists");
                to_preds[at] = mid;
                split += 1;
            }
        }
        split
    }
}
