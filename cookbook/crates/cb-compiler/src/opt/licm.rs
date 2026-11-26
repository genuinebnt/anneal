//! Loop-invariant code motion on SSA form.
//!
//! Loops are the natural loops of the dominator tree (`dom::natural_loops`,
//! innermost first). For one loop the pass hoists, in program order, every
//! assignment whose operands are all defined outside the loop, into the
//! *preheader*: the unique block that falls into the header from outside
//! (created, with phis merged, when the header has several outside entries).
//!
//! **What may move.** In SSA an operand defined outside the loop dominates the
//! header, so the hoisted value is available everywhere the original was.
//! The danger is *speculation*: the original instruction may never run (a
//! zero-trip loop, a branch inside the body), and PEBBLE has traps that are
//! observable, so:
//!
//! * pure, non-trapping rvalues (comparisons, `!`, field/tag reads, casts,
//!   `len` of a string, division and remainder by a constant other than 0 and
//!   -1) may be hoisted from any block of the loop;
//! * `len(array)` only if the loop contains no store, `push`, `pop` or call
//!   (arrays alias, so any of them can change the length);
//! * rvalues that can trap (`+ - *` overflow, `/ %` by a variable, negation,
//!   a *checked* index) are **never speculated**: they move only from the
//!   header and only while everything before them in the header is free of traps
//!   and effects. The header runs at least once whenever the loop is entered, so
//!   hoisting such an instruction before the loop executes the same trap, in
//!   the same order relative to other effects, as the first iteration would;
//! * never moved: aggregates and `chars` (allocation identity), built-ins with
//!   effects, string `+` and `to_str` (speculating an allocation can cost more
//!   than it saves), unchecked indexing (its safety proof depends on the loop's guard).

use super::dom::{self, DomTree, Loop};
use crate::mir::*;
use crate::sema::Builtin;
use crate::syntax::ast::{BinOp, UnOp};
use std::collections::HashSet;

#[derive(PartialEq)]
enum Class {
    /// Safe to execute even if the original would not have run.
    Speculatable,
    /// May trap: only hoistable from the loop header's trap-free prefix.
    Trapping,
    Never,
}

fn classify(f: &Func, rv: &Rvalue, mem_clean: bool) -> Class {
    match rv {
        Rvalue::Binary(op, a, b) => match op {
            BinOp::Add if f.operand_is_str(a) => Class::Never,
            BinOp::Add | BinOp::Sub | BinOp::Mul => Class::Trapping,
            BinOp::Div | BinOp::Rem => match b {
                Operand::Const(Const::Int(c)) if *c != 0 && *c != -1 => Class::Speculatable,
                _ => Class::Trapping,
            },
            BinOp::And | BinOp::Or => Class::Never,
            _ => Class::Speculatable,
        },
        Rvalue::Unary(UnOp::Neg, _) => Class::Trapping,
        Rvalue::Unary(UnOp::Not, _) | Rvalue::Field(..) | Rvalue::Tag(_) | Rvalue::Cast(_) => {
            Class::Speculatable
        }
        Rvalue::Len(a) if f.operand_is_str(a) || mem_clean => Class::Speculatable,
        Rvalue::Index(_, _, true) if mem_clean => Class::Trapping,
        Rvalue::Use(_) => Class::Speculatable,
        Rvalue::Builtin(Builtin::Len, _) => Class::Never,
        _ => Class::Never,
    }
}

fn defined_in(f: &Func, l: &Loop) -> HashSet<LocalId> {
    let mut d = HashSet::new();
    for b in l.blocks() {
        let blk = &f.blocks[b as usize];
        blk.phis.iter().for_each(|p| {
            d.insert(p.dest);
        });
        blk.stmts.iter().filter_map(|s| s.def()).for_each(|x| {
            d.insert(x);
        });
        if let Some(x) = blk.term.def() {
            d.insert(x);
        }
    }
    d
}

/// Hoist from one loop; returns whether the function changed.
fn hoist_loop(f: &mut Func, dt: &DomTree, l: &Loop) -> bool {
    let blocks: Vec<BlockId> = dt.rpo.iter().copied().filter(|&b| l.contains(b)).collect();
    let mem_clean = !blocks.iter().any(|&b| {
        let blk = &f.blocks[b as usize];
        blk.stmts.iter().any(|s| s.writes_memory()) || matches!(blk.term, Terminator::Call { .. })
    });
    let mut inside = defined_in(f, l);
    let mut hoisted: Vec<Stmt> = Vec::new();
    let mut changed = true;
    let mut taken: HashSet<(BlockId, usize)> = HashSet::new();
    while changed {
        changed = false;
        for &b in &blocks {
            let mut clean_prefix = true;
            for i in 0..f.blocks[b as usize].stmts.len() {
                if taken.contains(&(b, i)) {
                    continue;
                }
                let s = &f.blocks[b as usize].stmts[i];
                let StmtKind::Assign(d, rv) = &s.kind else {
                    clean_prefix = false;
                    continue;
                };
                let mut invariant = true;
                rv.for_each_operand(&mut |o| {
                    if let Operand::Local(x) = o
                        && inside.contains(x)
                    {
                        invariant = false;
                    }
                });
                let class = classify(f, rv, mem_clean);
                let in_header = b == l.header;
                let ok = invariant
                    && (class == Class::Speculatable
                        || (class == Class::Trapping && in_header && clean_prefix));
                if ok {
                    // Hoisting a statement that is already in the preheader's reach keeps program order:
                    // dependencies were hoisted earlier in this very scan.
                    inside.remove(d);
                    hoisted.push(s.clone());
                    taken.insert((b, i));
                    changed = true;
                } else if rv.may_trap_or_effect() {
                    clean_prefix = false;
                }
            }
        }
    }
    if hoisted.is_empty() {
        return false;
    }
    // Remove the hoisted statements from the loop.
    for &b in &blocks {
        let mut i = 0;
        f.blocks[b as usize].stmts.retain(|_| {
            i += 1;
            !taken.contains(&(b, i - 1))
        });
    }
    let pre = preheader(f, dt, l);
    f.blocks[pre as usize].stmts.extend(hoisted);
    true
}

/// The block that runs exactly once before the loop is entered; created when needed.
fn preheader(f: &mut Func, dt: &DomTree, l: &Loop) -> BlockId {
    let h = l.header;
    let outside: Vec<BlockId> = dt.preds[h as usize]
        .iter()
        .copied()
        .filter(|p| !l.contains(*p))
        .collect();
    if let [q] = outside[..] {
        let mut succs = f.blocks[q as usize].term.successors();
        succs.sort_unstable();
        succs.dedup();
        if succs == [h] && !matches!(f.blocks[q as usize].term, Terminator::Call { .. }) {
            return q;
        }
    }
    let p = f.blocks.len() as BlockId;
    f.blocks.push(Block::new(vec![], Terminator::Goto(h)));
    for &q in &outside {
        f.blocks[q as usize].term.for_each_succ_mut(&mut |t| {
            if *t == h {
                *t = p
            }
        });
    }
    // Header phis: the outside arguments are merged in the preheader.
    for k in 0..f.blocks[h as usize].phis.len() {
        let ph = f.blocks[h as usize].phis[k].clone();
        let (out_args, mut in_args): (Vec<_>, Vec<_>) =
            ph.args.into_iter().partition(|(pr, _)| !l.contains(*pr));
        let merged = if out_args.iter().all(|(_, o)| *o == out_args[0].1) {
            out_args[0].1.clone()
        } else {
            let ty = f.locals[ph.dest as usize].ty.clone();
            let m = f.new_local(ty);
            f.blocks[p as usize].phis.push(Phi {
                dest: m,
                args: out_args,
            });
            Operand::Local(m)
        };
        in_args.push((p, merged));
        f.blocks[h as usize].phis[k].args = in_args;
    }
    p
}

pub fn licm(f: &mut Func) -> bool {
    if !f.ssa {
        return false;
    }
    let mut any = false;
    // Hoisting can create a preheader, which changes the CFG: recompute after each loop.
    let mut done: HashSet<BlockId> = HashSet::new();
    for _ in 0..64 {
        let dt = DomTree::compute(f);
        let loops = dom::natural_loops(f, &dt);
        let Some(l) = loops.iter().find(|l| !done.contains(&l.header)) else {
            break;
        };
        done.insert(l.header);
        any |= hoist_loop(f, &dt, l);
    }
    any
}
