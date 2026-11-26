//! CFG clean-up: unreachable-block removal, constant-branch folding, jump
//! threading and block merging.

use crate::mir::*;

/// Drop blocks unreachable from the entry and renumber the rest (order kept).
pub fn remove_unreachable(f: &mut Func) {
    let n = f.blocks.len();
    let mut keep = vec![false; n];
    for b in f.reachable() {
        keep[b as usize] = true;
    }
    if keep.iter().all(|k| *k) {
        return;
    }
    let mut new_id = vec![0 as BlockId; n];
    let mut next = 0;
    for (i, k) in keep.iter().enumerate() {
        if *k {
            new_id[i] = next;
            next += 1;
        }
    }
    let old = std::mem::take(&mut f.blocks);
    f.blocks = old
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, mut b)| {
            b.term.for_each_succ_mut(&mut |t| *t = new_id[*t as usize]);
            b
        })
        .collect();
}

fn fold_terminators(f: &mut Func) -> bool {
    let mut changed = false;
    for b in &mut f.blocks {
        let new = match &b.term {
            Terminator::Branch {
                cond: Operand::Const(Const::Bool(c)),
                then_bb,
                else_bb,
            } => Some(Terminator::Goto(if *c { *then_bb } else { *else_bb })),
            Terminator::Branch {
                then_bb, else_bb, ..
            } if then_bb == else_bb => Some(Terminator::Goto(*then_bb)),
            Terminator::Switch {
                value: Operand::Const(Const::Int(v)),
                cases,
                default,
            } => Some(Terminator::Goto(
                cases.iter().find(|c| c.0 == *v).map_or(*default, |c| c.1),
            )),
            Terminator::Switch { cases, default, .. } if cases.iter().all(|c| c.1 == *default) => {
                Some(Terminator::Goto(*default))
            }
            _ => None,
        };
        if let Some(t) = new {
            b.term = t;
            changed = true;
        }
    }
    changed
}

/// Redirect jumps to empty `goto` blocks straight to their destination.
fn thread_jumps(f: &mut Func) -> bool {
    let n = f.blocks.len();
    let forward: Vec<Option<BlockId>> = f
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| match b.term {
            Terminator::Goto(t) if b.stmts.is_empty() && t as usize != i => Some(t),
            _ => None,
        })
        .collect();
    let resolve = |mut b: BlockId| {
        for _ in 0..n {
            match forward[b as usize] {
                Some(t) => b = t,
                None => break,
            }
        }
        b
    };
    let mut changed = false;
    for i in 0..n {
        f.blocks[i].term.for_each_succ_mut(&mut |t| {
            let r = resolve(*t);
            if r != *t {
                *t = r;
                changed = true;
            }
        });
    }
    changed
}

/// Merge `a: goto b` with `b` when `a` is `b`'s only predecessor.
fn merge_blocks(f: &mut Func) -> bool {
    let mut changed = false;
    loop {
        let mut edges = vec![0usize; f.blocks.len()];
        for b in f.reachable() {
            for s in f.blocks[b as usize].term.successors() {
                edges[s as usize] += 1;
            }
        }
        let cand = f
            .reachable()
            .into_iter()
            .find_map(|a| match f.blocks[a as usize].term {
                Terminator::Goto(b) if b != a && b != 0 && edges[b as usize] == 1 => Some((a, b)),
                _ => None,
            });
        let Some((a, b)) = cand else { return changed };
        let blk = std::mem::replace(
            &mut f.blocks[b as usize],
            Block::new(vec![], Terminator::Unreachable),
        );
        let dst = &mut f.blocks[a as usize];
        dst.stmts.extend(blk.stmts);
        dst.term = blk.term;
        changed = true;
    }
}

/// Run the clean-ups to a fixed point. Returns whether anything changed.
pub fn simplify_cfg(f: &mut Func) -> bool {
    let before = f.blocks.len();
    let mut any = false;
    loop {
        let mut c = fold_terminators(f);
        c |= thread_jumps(f);
        c |= merge_blocks(f);
        remove_unreachable(f);
        any |= c;
        if !c {
            break;
        }
    }
    any || f.blocks.len() != before
}
