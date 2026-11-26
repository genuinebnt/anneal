//! Backward liveness analysis, shared by dead-code elimination and the
//! bytecode register allocator.

use crate::mir::*;

#[derive(Clone, Debug, PartialEq)]
pub struct BitSet(Vec<u64>);

impl BitSet {
    pub fn new(n: usize) -> BitSet {
        BitSet(vec![0; n.div_ceil(64)])
    }
    pub fn insert(&mut self, i: u32) {
        self.0[(i / 64) as usize] |= 1 << (i % 64);
    }
    pub fn remove(&mut self, i: u32) {
        self.0[(i / 64) as usize] &= !(1 << (i % 64));
    }
    pub fn contains(&self, i: u32) -> bool {
        self.0[(i / 64) as usize] >> (i % 64) & 1 == 1
    }
    /// `self |= other`; returns whether `self` grew.
    pub fn union_with(&mut self, other: &BitSet) -> bool {
        let mut grew = false;
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            let n = *a | *b;
            grew |= n != *a;
            *a = n;
        }
        grew
    }
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.iter().enumerate().flat_map(|(w, &bits)| {
            (0..64)
                .filter(move |b| bits >> b & 1 == 1)
                .map(move |b| (w * 64 + b) as u32)
        })
    }
}

/// Apply a statement backwards: kill its definition, then add its uses.
pub fn step_back_stmt(s: &Stmt, live: &mut BitSet) {
    if let StmtKind::Assign(d, _) = &s.kind {
        live.remove(*d);
    }
    s.for_each_use(&mut |o| {
        if let Operand::Local(l) = o {
            live.insert(*l)
        }
    });
}

pub fn step_back_term(t: &Terminator, live: &mut BitSet) {
    if let Some(d) = t.def() {
        live.remove(d);
    }
    t.for_each_use(&mut |o| {
        if let Operand::Local(l) = o {
            live.insert(*l)
        }
    });
}

pub struct Liveness {
    pub live_in: Vec<BitSet>,
    pub live_out: Vec<BitSet>,
}

pub fn liveness(f: &Func) -> Liveness {
    let (nb, nl) = (f.blocks.len(), f.locals.len());
    let mut live_in = vec![BitSet::new(nl); nb];
    let mut live_out = vec![BitSet::new(nl); nb];
    let mut order = f.rpo();
    order.reverse();
    let mut changed = true;
    while changed {
        changed = false;
        for &b in &order {
            let blk = &f.blocks[b as usize];
            let mut out = BitSet::new(nl);
            for s in blk.term.successors() {
                out.union_with(&live_in[s as usize]);
            }
            let mut cur = out.clone();
            step_back_term(&blk.term, &mut cur);
            for s in blk.stmts.iter().rev() {
                step_back_stmt(s, &mut cur);
            }
            live_out[b as usize] = out;
            if cur != live_in[b as usize] {
                live_in[b as usize] = cur;
                changed = true;
            }
        }
    }
    Liveness { live_in, live_out }
}
