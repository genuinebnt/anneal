//! Sparse conditional constant propagation (Wegman & Zadeck) on SSA form.
//!
//! Two worklists run together: CFG edges that became executable and SSA values
//! whose lattice value changed. A value is `Top` (not yet known), `Const(c)` or
//! `Bot` (varies). Blocks and edges start non-executable and a branch only
//! makes the successor its condition allows executable, so code guarded by a
//! condition that is constant *only because of other constants* is found
//! (plain constant propagation, `prop.rs`, cannot see through a loop whose
//! exit test depends on a value that is itself computed by the loop).
//!
//! Folding never evaluates an operation that would trap (`prop::fold` returns
//! `None` then), so traps survive. The rewrite replaces uses of constant values,
//! deletes their definitions, turns constant branches into jumps and drops
//! blocks that were never executable.

use super::prop::fold;
use super::ssa;
use crate::mir::*;
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
enum L {
    Top,
    Const(Const),
    Bot,
}

fn meet(a: &L, b: &L) -> L {
    match (a, b) {
        (L::Top, x) | (x, L::Top) => x.clone(),
        (L::Const(x), L::Const(y)) if x == y => a.clone(),
        _ => L::Bot,
    }
}

#[derive(Clone, Copy)]
enum Site {
    Phi(BlockId),
    Stmt(BlockId, usize),
    Term(BlockId),
}

pub fn sccp(f: &mut Func) -> bool {
    if !f.ssa {
        return false;
    }
    let nl = f.locals.len();
    let nb = f.blocks.len();
    let mut val = vec![L::Top; nl];
    for v in val.iter_mut().take(f.params) {
        *v = L::Bot;
    }
    // Def-use chains: where each local is read.
    let mut users: Vec<Vec<Site>> = vec![Vec::new(); nl];
    for b in f.reachable() {
        let blk = &f.blocks[b as usize];
        for p in &blk.phis {
            for (_, o) in &p.args {
                if let Operand::Local(l) = o {
                    users[*l as usize].push(Site::Phi(b));
                }
            }
        }
        for (i, s) in blk.stmts.iter().enumerate() {
            s.for_each_use(&mut |o| {
                if let Operand::Local(l) = o {
                    users[*l as usize].push(Site::Stmt(b, i))
                }
            });
        }
        blk.term.for_each_use(&mut |o| {
            if let Operand::Local(l) = o {
                users[*l as usize].push(Site::Term(b))
            }
        });
    }
    let mut exec_block = vec![false; nb];
    let mut exec_edge: std::collections::HashSet<(BlockId, BlockId)> = Default::default();
    let mut flow: VecDeque<(BlockId, BlockId)> = VecDeque::new();
    let mut ssa_work: VecDeque<Site> = VecDeque::new();

    let get = |val: &Vec<L>, o: &Operand| match o {
        Operand::Const(c) => L::Const(c.clone()),
        Operand::Local(l) => val[*l as usize].clone(),
    };
    // Evaluate one statement's rvalue under the current lattice.
    let eval_rv = |val: &Vec<L>, rv: &Rvalue| -> L {
        let mut any_top = false;
        let mut sub = rv.clone();
        sub.for_each_operand_mut(&mut |o| match get(val, o) {
            L::Const(c) => *o = Operand::Const(c),
            L::Top => any_top = true,
            L::Bot => {}
        });
        if any_top {
            return L::Top;
        }
        match fold(&sub) {
            Some(c) => L::Const(c),
            None => L::Bot,
        }
    };

    // Seed: the entry block is executable.
    exec_block[0] = true;
    let visit_block_first = |b: BlockId, ssa_work: &mut VecDeque<Site>, f: &Func| {
        let blk = &f.blocks[b as usize];
        if !blk.phis.is_empty() {
            ssa_work.push_back(Site::Phi(b));
        }
        for i in 0..blk.stmts.len() {
            ssa_work.push_back(Site::Stmt(b, i));
        }
        ssa_work.push_back(Site::Term(b));
    };
    visit_block_first(0, &mut ssa_work, f);

    macro_rules! set {
        ($l:expr, $new:expr) => {{
            let (l, new): (LocalId, L) = ($l, $new);
            let merged = meet(&val[l as usize], &new);
            // Lattice values only move down; `meet` with Top keeps `new`.
            let next = if val[l as usize] == L::Top {
                new
            } else {
                merged
            };
            if next != val[l as usize] {
                val[l as usize] = next;
                for &u in &users[l as usize] {
                    ssa_work.push_back(u);
                }
            }
        }};
    }

    loop {
        if let Some((p, s)) = flow.pop_front() {
            if !exec_edge.insert((p, s)) {
                continue;
            }
            if !exec_block[s as usize] {
                exec_block[s as usize] = true;
                visit_block_first(s, &mut ssa_work, f);
            } else if !f.blocks[s as usize].phis.is_empty() {
                ssa_work.push_back(Site::Phi(s));
            }
            continue;
        }
        let Some(site) = ssa_work.pop_front() else {
            break;
        };
        match site {
            Site::Phi(b) => {
                if !exec_block[b as usize] {
                    continue;
                }
                for ph in &f.blocks[b as usize].phis {
                    let mut v = L::Top;
                    for (p, o) in &ph.args {
                        if exec_edge.contains(&(*p, b)) {
                            v = meet(&v, &get(&val, o));
                        }
                    }
                    set!(ph.dest, v);
                }
            }
            Site::Stmt(b, i) => {
                if !exec_block[b as usize] {
                    continue;
                }
                if let StmtKind::Assign(d, rv) = &f.blocks[b as usize].stmts[i].kind {
                    let v = eval_rv(&val, rv);
                    set!(*d, v);
                }
            }
            Site::Term(b) => {
                if !exec_block[b as usize] {
                    continue;
                }
                let blk = &f.blocks[b as usize];
                match &blk.term {
                    Terminator::Goto(t) => flow.push_back((b, *t)),
                    Terminator::Call { target, dest, .. } => {
                        set!(*dest, L::Bot);
                        flow.push_back((b, *target));
                    }
                    Terminator::Branch {
                        cond,
                        then_bb,
                        else_bb,
                    } => match get(&val, cond) {
                        L::Const(Const::Bool(true)) => flow.push_back((b, *then_bb)),
                        L::Const(Const::Bool(false)) => flow.push_back((b, *else_bb)),
                        L::Top => {}
                        _ => {
                            flow.push_back((b, *then_bb));
                            flow.push_back((b, *else_bb));
                        }
                    },
                    Terminator::Switch {
                        value,
                        cases,
                        default,
                    } => match get(&val, value) {
                        L::Const(Const::Int(v)) => flow.push_back((
                            b,
                            cases.iter().find(|c| c.0 == v).map_or(*default, |c| c.1),
                        )),
                        L::Top => {}
                        _ => {
                            for c in cases {
                                flow.push_back((b, c.1));
                            }
                            flow.push_back((b, *default));
                        }
                    },
                    Terminator::Return(_) | Terminator::Unreachable => {}
                }
            }
        }
    }

    // ---- rewrite
    let mut changed = false;
    let map: Vec<Option<Operand>> = val
        .iter()
        .map(|v| {
            if let L::Const(c) = v {
                Some(Operand::Const(c.clone()))
            } else {
                None
            }
        })
        .collect();
    let mut is_const_def = vec![false; nl];
    for (l, m) in map.iter().enumerate() {
        is_const_def[l] = m.is_some();
    }
    // Delete the definitions of constant values (phis and pure assignments); calls keep their dest.
    for b in 0..nb {
        let blk = &mut f.blocks[b];
        let before = (blk.phis.len(), blk.stmts.len());
        blk.phis.retain(|p| !is_const_def[p.dest as usize]);
        blk.stmts.retain(|s| !matches!(&s.kind, StmtKind::Assign(d, rv) if is_const_def[*d as usize] && !rv.may_trap_or_effect()));
        changed |= before != (blk.phis.len(), blk.stmts.len());
    }
    // A constant value whose definition must stay (a trapping rvalue cannot be a constant, but be safe):
    // keep it defined by turning the stmt into a copy of the constant.
    for blk in &mut f.blocks {
        for s in &mut blk.stmts {
            if let StmtKind::Assign(d, rv) = &mut s.kind
                && is_const_def[*d as usize]
                && let Some(Operand::Const(c)) = &map[*d as usize]
            {
                *rv = Rvalue::Use(Operand::Const(c.clone()));
            }
        }
    }
    ssa::replace_uses(f, &map);
    // Fold branches whose condition is now constant (this also covers branches whose edges SCCP proved dead).
    for b in 0..nb {
        let new = match &f.blocks[b].term {
            Terminator::Branch {
                cond: Operand::Const(Const::Bool(c)),
                then_bb,
                else_bb,
            } => Some(Terminator::Goto(if *c { *then_bb } else { *else_bb })),
            Terminator::Switch {
                value: Operand::Const(Const::Int(v)),
                cases,
                default,
            } => Some(Terminator::Goto(
                cases.iter().find(|c| c.0 == *v).map_or(*default, |c| c.1),
            )),
            _ => None,
        };
        if let Some(t) = new {
            f.blocks[b].term = t;
            changed = true;
        }
    }
    // Dead blocks: not executable.
    for b in 0..nb {
        if !exec_block[b] && !matches!(f.blocks[b].term, Terminator::Unreachable) {
            f.blocks[b] = Block::new(vec![], Terminator::Unreachable);
            changed = true;
        }
    }
    // A branch with a non-constant condition keeps both edges even if one was never taken;
    // that is sound (the edge just carries no information).
    ssa::remove_unreachable_blocks(f);
    ssa::simplify_phis(f);
    changed |= f.blocks.len() != nb;
    changed
}
