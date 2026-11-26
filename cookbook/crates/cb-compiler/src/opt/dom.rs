//! Dominators (Cooper, Harvey & Kennedy, "A Simple, Fast Dominance Algorithm"),
//! the dominator tree with O(1) dominance queries, and dominance frontiers.
//!
//! Only blocks reachable from the entry take part; `idom(entry)` is `None`.
//! The tree is numbered with a depth-first enter/exit clock so `dominates(a, b)`
//! is two integer comparisons (the trick LLVM's `DominatorTree` also uses).

use crate::mir::*;

pub struct DomTree {
    /// Immediate dominator of every reachable block (`None` for the entry and for unreachable blocks).
    pub idom: Vec<Option<BlockId>>,
    /// Reverse postorder of the reachable blocks.
    pub rpo: Vec<BlockId>,
    /// Dominator-tree children, in reverse-postorder of the children.
    pub children: Vec<Vec<BlockId>>,
    /// Predecessors of every reachable block (deduplicated).
    pub preds: Vec<Vec<BlockId>>,
    reachable: Vec<bool>,
    tin: Vec<u32>,
    tout: Vec<u32>,
}

impl DomTree {
    pub fn compute(f: &Func) -> DomTree {
        let n = f.blocks.len();
        let rpo = f.rpo();
        let mut num = vec![u32::MAX; n];
        for (i, &b) in rpo.iter().enumerate() {
            num[b as usize] = i as u32;
        }
        let preds = f.predecessors();
        let mut idom: Vec<Option<BlockId>> = vec![None; n];
        idom[0] = Some(0);
        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                let mut new: Option<BlockId> = None;
                for &p in &preds[b as usize] {
                    if idom[p as usize].is_none() {
                        continue;
                    }
                    new = Some(match new {
                        None => p,
                        Some(cur) => intersect(&idom, &num, cur, p),
                    });
                }
                if new != idom[b as usize] {
                    idom[b as usize] = new;
                    changed = true;
                }
            }
        }
        idom[0] = None;
        let mut children = vec![Vec::new(); n];
        for &b in &rpo {
            if let Some(p) = idom[b as usize] {
                children[p as usize].push(b);
            }
        }
        let mut reachable = vec![false; n];
        rpo.iter().for_each(|&b| reachable[b as usize] = true);
        // Euler numbering with an explicit stack (deep CFGs must not recurse).
        let (mut tin, mut tout) = (vec![0u32; n], vec![0u32; n]);
        let mut clock = 0u32;
        let mut stack: Vec<(BlockId, usize)> = vec![(0, 0)];
        tin[0] = clock;
        clock += 1;
        while let Some(&mut (b, ref mut i)) = stack.last_mut() {
            if let Some(&c) = children[b as usize].get(*i) {
                *i += 1;
                tin[c as usize] = clock;
                clock += 1;
                stack.push((c, 0));
            } else {
                tout[b as usize] = clock;
                clock += 1;
                stack.pop();
            }
        }
        DomTree {
            idom,
            rpo,
            children,
            preds,
            reachable,
            tin,
            tout,
        }
    }

    pub fn is_reachable(&self, b: BlockId) -> bool {
        self.reachable.get(b as usize).copied().unwrap_or(false)
    }

    /// Does `a` dominate `b` (every block dominates itself)? False for unreachable blocks.
    pub fn dominates(&self, a: BlockId, b: BlockId) -> bool {
        self.is_reachable(a)
            && self.is_reachable(b)
            && self.tin[a as usize] <= self.tin[b as usize]
            && self.tout[b as usize] <= self.tout[a as usize]
    }

    pub fn strictly_dominates(&self, a: BlockId, b: BlockId) -> bool {
        a != b && self.dominates(a, b)
    }

    /// Dominator-tree preorder (parents before children).
    pub fn preorder(&self) -> Vec<BlockId> {
        let mut out = Vec::with_capacity(self.rpo.len());
        let mut stack = vec![0 as BlockId];
        while let Some(b) = stack.pop() {
            out.push(b);
            stack.extend(self.children[b as usize].iter().rev());
        }
        out
    }

    /// Dominance frontiers: `df[b]` is the set of blocks where `b`'s dominance ends
    /// (the places a definition in `b` needs a phi), sorted and without duplicates.
    pub fn frontiers(&self) -> Vec<Vec<BlockId>> {
        let n = self.idom.len();
        let mut df: Vec<Vec<BlockId>> = vec![Vec::new(); n];
        for &b in &self.rpo {
            let ps = &self.preds[b as usize];
            if ps.len() < 2 {
                continue;
            }
            let stop = self.idom[b as usize];
            for &p in ps {
                if !self.is_reachable(p) {
                    continue;
                }
                let mut runner = p;
                while Some(runner) != stop {
                    if df[runner as usize].last() != Some(&b) && !df[runner as usize].contains(&b) {
                        df[runner as usize].push(b);
                    }
                    match self.idom[runner as usize] {
                        Some(i) => runner = i,
                        None => break,
                    }
                }
            }
        }
        df.iter_mut().for_each(|v| v.sort_unstable());
        df
    }
}

fn intersect(idom: &[Option<BlockId>], num: &[u32], mut a: BlockId, mut b: BlockId) -> BlockId {
    while a != b {
        while num[a as usize] > num[b as usize] {
            a = idom[a as usize].expect("processed block has an idom");
        }
        while num[b as usize] > num[a as usize] {
            b = idom[b as usize].expect("processed block has an idom");
        }
    }
    a
}

/// A natural loop: the header dominates every block of the body, and every
/// back edge `latch -> header` has its source in the body.
#[derive(Clone, Debug)]
pub struct Loop {
    pub header: BlockId,
    pub latches: Vec<BlockId>,
    /// `body[b]` is true for the blocks of the loop (the header included).
    pub body: Vec<bool>,
}

impl Loop {
    pub fn contains(&self, b: BlockId) -> bool {
        self.body.get(b as usize).copied().unwrap_or(false)
    }
    pub fn blocks(&self) -> Vec<BlockId> {
        (0..self.body.len() as BlockId)
            .filter(|&b| self.body[b as usize])
            .collect()
    }
    pub fn size(&self) -> usize {
        self.body.iter().filter(|b| **b).count()
    }
}

/// Natural loops found from back edges (an edge whose target dominates its source).
/// Loops that share a header are merged. Sorted innermost-first (smaller bodies first,
/// which is also a valid order for processing nested loops inside out).
pub fn natural_loops(f: &Func, dt: &DomTree) -> Vec<Loop> {
    let n = f.blocks.len();
    let mut by_header: Vec<(BlockId, Vec<BlockId>)> = Vec::new();
    for &b in &dt.rpo {
        for s in f.blocks[b as usize].term.successors() {
            if dt.dominates(s, b) {
                match by_header.iter_mut().find(|(h, _)| *h == s) {
                    Some((_, l)) => {
                        if !l.contains(&b) {
                            l.push(b)
                        }
                    }
                    None => by_header.push((s, vec![b])),
                }
            }
        }
    }
    let mut loops: Vec<Loop> = by_header
        .into_iter()
        .map(|(header, latches)| {
            let mut body = vec![false; n];
            body[header as usize] = true;
            let mut stack = latches.clone();
            while let Some(b) = stack.pop() {
                if body[b as usize] {
                    continue;
                }
                body[b as usize] = true;
                stack.extend(dt.preds[b as usize].iter().copied());
            }
            Loop {
                header,
                latches,
                body,
            }
        })
        .collect();
    loops.sort_by_key(|l| (l.size(), l.header));
    loops
}
