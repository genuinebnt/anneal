//! Global value numbering over the dominator tree (SSA form).
//!
//! The walk visits blocks in dominator-tree preorder with a *scoped* hash
//! table: an expression computed in block `B` is visible to every block `B`
//! dominates and to no other, which is exactly when reusing the earlier result
//! is correct. Each pure expression is keyed by its operator and the
//! *canonical* operands (copies and constants are resolved on the fly), so
//! `a + b` in a dominating block makes the same expression in a loop body or an
//! `else` branch a copy of the first result: redundancy elimination across
//! blocks, which the block-local `cse` cannot do.
//!
//! * **Traps.** Replacing a trapping operation by an earlier identical one is
//!   sound: the dominating occurrence already ran (and would have trapped), so
//!   `a / b` is merged but never *hoisted* here.
//! * **Memory.** `Len` and `Index` read array contents, which `SetIndex`,
//!   `push`, `pop` and every call may change. Memory reads are keyed by a
//!   *memory epoch*: it advances at each such write, and a block that is not the
//!   sole successor of its immediate dominator (a join, a loop header) starts a
//!   fresh epoch. Aggregates built with `Agg` have identity (arrays) or are
//!   cheap to rebuild, so they are never merged.
//! * **Phis.** A phi whose arguments are all one value (ignoring itself) is
//!   that value; two phis of one block with identical arguments are equal.
//! * Constant folding (`prop::fold`) happens on the canonical operands, so a
//!   fold exposed by merging two expressions is applied in the same walk.

use super::dom::DomTree;
use super::prop::fold;
use super::ssa;
use crate::mir::*;
use crate::sema::Builtin;
use crate::syntax::ast::{BinOp, UnOp};
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Bin(BinOp, Operand2, Operand2),
    Un(UnOp, Operand2),
    Field(Operand2, u32),
    WithField(Operand2, u32, Operand2),
    Tag(Operand2),
    Cast(Operand2),
    ToStr(Operand2),
    Len(Operand2, u32),
    Index(Operand2, Operand2, u32),
    Phi(BlockId, Vec<(BlockId, Operand2)>),
}

/// Hashable operand (`Operand` itself is only `PartialEq`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Operand2 {
    C(Const),
    L(LocalId),
}

fn o2(o: &Operand) -> Operand2 {
    match o {
        Operand::Const(c) => Operand2::C(c.clone()),
        Operand::Local(l) => Operand2::L(*l),
    }
}

fn key_of(f: &Func, rv: &Rvalue, epoch: u32) -> Option<Key> {
    Some(match rv {
        Rvalue::Binary(op, a, b) => {
            let (mut x, mut y) = (o2(a), o2(b));
            let commutes = matches!(op, BinOp::Eq | BinOp::Ne)
                || (matches!(op, BinOp::Add | BinOp::Mul) && !f.operand_is_str(a));
            if commutes && format!("{:?}", hash_ord(&x)) > format!("{:?}", hash_ord(&y)) {
                std::mem::swap(&mut x, &mut y);
            }
            Key::Bin(*op, x, y)
        }
        Rvalue::Unary(op, a) => Key::Un(*op, o2(a)),
        Rvalue::Field(a, i) => Key::Field(o2(a), *i),
        Rvalue::WithField(a, i, v) => Key::WithField(o2(a), *i, o2(v)),
        Rvalue::Tag(a) => Key::Tag(o2(a)),
        Rvalue::Cast(a) => Key::Cast(o2(a)),
        Rvalue::Builtin(Builtin::ToStr, xs) => Key::ToStr(o2(&xs[0])),
        Rvalue::Len(a) => Key::Len(o2(a), epoch),
        Rvalue::Index(a, i, _) => Key::Index(o2(a), o2(i), epoch),
        _ => return None,
    })
}

/// A total order on operands for canonicalising commutative operators.
fn hash_ord(o: &Operand2) -> (u8, String) {
    match o {
        Operand2::L(l) => (0, format!("{l:010}")),
        Operand2::C(c) => (1, format!("{c:?}")),
    }
}

fn resolve(canon: &[Option<Operand>], o: &Operand) -> Operand {
    match o {
        Operand::Local(l) => match &canon[*l as usize] {
            Some(n) => n.clone(),
            None => o.clone(),
        },
        c => c.clone(),
    }
}

pub fn gvn(f: &mut Func) -> bool {
    if !f.ssa {
        return false;
    }
    ssa::remove_unreachable_blocks(f);
    let dt = DomTree::compute(f);
    let nl = f.locals.len();
    // canon[l] = the operand `l` is equal to (a leader local or a constant), if known.
    let mut canon: Vec<Option<Operand>> = vec![None; nl];
    let mut table: HashMap<Key, LocalId> = HashMap::new();
    let mut removed: Vec<(BlockId, usize)> = Vec::new();
    let mut removed_phis: Vec<(BlockId, usize)> = Vec::new();
    let nb = f.blocks.len();
    let mut next_epoch = 1u32;
    let mut end_epoch = vec![0u32; nb];
    let mut start_epoch = vec![0u32; nb];

    enum Ev {
        Enter(BlockId),
        Exit(Vec<Key>),
    }
    let mut work = vec![Ev::Enter(0)];
    while let Some(ev) = work.pop() {
        let b = match ev {
            Ev::Exit(keys) => {
                for k in keys {
                    table.remove(&k);
                }
                continue;
            }
            Ev::Enter(b) => b,
        };
        let mut inserted: Vec<Key> = Vec::new();
        // Memory epoch at block entry.
        let mut epoch = match dt.idom[b as usize] {
            Some(p) if dt.preds[b as usize].len() == 1 && dt.preds[b as usize][0] == p => {
                end_epoch[p as usize]
            }
            _ => {
                next_epoch += 1;
                next_epoch
            }
        };
        start_epoch[b as usize] = epoch;
        // Phis.
        for k in 0..f.blocks[b as usize].phis.len() {
            let ph = &f.blocks[b as usize].phis[k];
            let args: Vec<(BlockId, Operand)> = ph
                .args
                .iter()
                .map(|(p, o)| (*p, resolve(&canon, o)))
                .collect();
            let mut vals: Vec<&Operand> = Vec::new();
            for (_, o) in &args {
                if *o != Operand::Local(ph.dest) && !vals.contains(&o) {
                    vals.push(o);
                }
            }
            if vals.len() == 1 {
                canon[ph.dest as usize] = Some(vals[0].clone());
                removed_phis.push((b, k));
                continue;
            }
            let mut sorted: Vec<(BlockId, Operand2)> =
                args.iter().map(|(p, o)| (*p, o2(o))).collect();
            sorted.sort_by_key(|(p, _)| *p);
            let key = Key::Phi(b, sorted);
            match table.get(&key) {
                Some(&leader) => {
                    canon[ph.dest as usize] = Some(Operand::Local(leader));
                    removed_phis.push((b, k));
                }
                None => {
                    table.insert(key.clone(), ph.dest);
                    inserted.push(key);
                }
            }
        }
        // Statements.
        for i in 0..f.blocks[b as usize].stmts.len() {
            let mut st = f.blocks[b as usize].stmts[i].clone();
            if st.writes_memory() {
                next_epoch += 1;
                epoch = next_epoch;
            }
            let StmtKind::Assign(d, rv) = &mut st.kind else {
                if matches!(st.kind, StmtKind::SetIndex(..)) {
                    next_epoch += 1;
                    epoch = next_epoch;
                }
                continue;
            };
            let d = *d;
            rv.for_each_operand_mut(&mut |o| *o = resolve(&canon, o));
            if let Rvalue::Use(o) = rv {
                canon[d as usize] = Some(o.clone());
                removed.push((b, i));
                continue;
            }
            if let Some(c) = fold(rv) {
                canon[d as usize] = Some(Operand::Const(c));
                removed.push((b, i));
                continue;
            }
            if let Some(key) = key_of(f, rv, epoch) {
                match table.get(&key) {
                    Some(&leader) => {
                        canon[d as usize] = Some(Operand::Local(leader));
                        removed.push((b, i));
                    }
                    None => {
                        table.insert(key.clone(), d);
                        inserted.push(key);
                    }
                }
            }
        }
        if matches!(f.blocks[b as usize].term, Terminator::Call { .. }) {
            next_epoch += 1;
            epoch = next_epoch;
        }
        end_epoch[b as usize] = epoch;
        work.push(Ev::Exit(inserted));
        for &c in dt.children[b as usize].iter().rev() {
            work.push(Ev::Enter(c));
        }
    }
    let changed = !removed.is_empty() || !removed_phis.is_empty();
    // Apply: rewrite operands everywhere (phi arguments of back edges are only
    // known after the walk), then delete the redundant definitions.
    ssa::replace_uses(f, &canon);
    removed.sort_unstable();
    for &(b, i) in removed.iter().rev() {
        f.blocks[b as usize].stmts.remove(i);
    }
    removed_phis.sort_unstable();
    for &(b, k) in removed_phis.iter().rev() {
        f.blocks[b as usize].phis.remove(k);
    }
    // Folded `Use`s changed the statements' operands only through `replace_uses`; fold what became constant.
    ssa::simplify_phis(f);
    changed
}
