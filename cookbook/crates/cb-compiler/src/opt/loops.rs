//! Loop-oriented passes: self tail calls become jumps, and bounds checks that
//! a loop condition already proves are removed.

use crate::mir::*;
use crate::syntax::ast::BinOp;
use std::collections::{HashMap, HashSet};

// ------------------------------------------------------------- tail calls

/// Does control starting at `block` only copy `value` around and then return it?
/// (Lowering routes `if`/`match` results through a join temporary.)
fn returns_result_unchanged(f: &Func, mut block: BlockId, mut value: LocalId) -> bool {
    for _ in 0..8 {
        let blk = &f.blocks[block as usize];
        for s in &blk.stmts {
            match &s.kind {
                StmtKind::Assign(d, Rvalue::Use(Operand::Local(src))) if *src == value => {
                    value = *d
                }
                _ => return false,
            }
        }
        match &blk.term {
            Terminator::Goto(next) => block = *next,
            Terminator::Return(Operand::Local(l)) => return *l == value,
            Terminator::Return(Operand::Const(Const::Unit)) => {
                return f.ret == crate::sema::Ty::Unit;
            }
            _ => return false,
        }
    }
    false
}

/// `return f(args)` inside `f` (a *self tail call*) becomes "assign the
/// parameters, jump to the top". This changes stack depth behaviour: a program
/// that would hit the recursion limit may now run to completion, and traps
/// inside the loop report one frame instead of one per iteration.
pub fn tail_calls(f: &mut Func, self_id: FuncId) -> bool {
    let is_tail = |f: &Func, b: usize| -> bool {
        let Terminator::Call {
            callee: Callee::Direct(c),
            dest,
            target,
            ..
        } = &f.blocks[b].term
        else {
            return false;
        };
        *c == self_id && !f.is_const && returns_result_unchanged(f, *target, *dest)
    };
    let cands: Vec<usize> = f
        .reachable()
        .into_iter()
        .map(|b| b as usize)
        .filter(|&b| is_tail(f, b))
        .collect();
    if cands.is_empty() {
        return false;
    }
    // Give the old entry block a new id so the loop header is a normal block.
    let header = f.blocks.len() as BlockId;
    let old_entry = std::mem::replace(
        &mut f.blocks[0],
        Block::new(vec![], Terminator::Goto(header)),
    );
    f.blocks.push(old_entry);
    for b in cands {
        // Blocks that were the entry are now at `header`.
        let b = if b == 0 { header as usize } else { b };
        let Terminator::Call {
            args, scope, span, ..
        } = std::mem::replace(&mut f.blocks[b].term, Terminator::Goto(header))
        else {
            continue;
        };
        let mut moves = Vec::new();
        let mut final_assign = Vec::new();
        for (i, a) in args.into_iter().enumerate() {
            let i = i as LocalId;
            if a == Operand::Local(i) {
                continue;
            }
            // An argument that reads another parameter must be copied first:
            // parameters are overwritten one by one.
            let a = match &a {
                Operand::Local(l) if (*l as usize) < f.params => {
                    let ty = f.local_ty(*l).clone();
                    let t = f.new_local(ty);
                    moves.push(Stmt::at(
                        StmtKind::Assign(t, Rvalue::Use(a.clone())),
                        scope,
                        span,
                    ));
                    Operand::Local(t)
                }
                _ => a,
            };
            final_assign.push(Stmt::at(StmtKind::Assign(i, Rvalue::Use(a)), scope, span));
        }
        f.blocks[b].stmts.extend(moves);
        f.blocks[b].stmts.extend(final_assign);
    }
    true
}

// --------------------------------------------------- bounds-check elimination

/// Facts valid at a program point (all are *must* facts).
#[derive(Clone, PartialEq, Debug)]
struct St {
    /// Known to be `>= 0`.
    nonneg: Vec<bool>,
    /// `n` holds `len(a)`.
    lens: HashMap<LocalId, LocalId>,
    /// `t` holds `i < n`.
    cmps: HashMap<LocalId, (LocalId, LocalId)>,
    /// `0 <= i < len(a)`.
    facts: HashSet<(LocalId, LocalId)>,
}

impl St {
    fn meet(&mut self, o: &St) -> bool {
        let before = self.clone();
        self.nonneg
            .iter_mut()
            .zip(&o.nonneg)
            .for_each(|(a, b)| *a &= *b);
        self.lens.retain(|k, v| o.lens.get(k) == Some(v));
        self.cmps.retain(|k, v| o.cmps.get(k) == Some(v));
        self.facts.retain(|k| o.facts.contains(k));
        *self != before
    }
    fn forget_arrays(&mut self) {
        self.lens.clear();
        self.facts.clear();
    }
    /// `d` is about to be overwritten: drop every fact that mentions it.
    fn kill(&mut self, d: LocalId) {
        self.lens.retain(|n, a| *n != d && *a != d);
        self.cmps.retain(|t, (i, n)| *t != d && *i != d && *n != d);
        self.facts.retain(|(i, a)| *i != d && *a != d);
        self.nonneg[d as usize] = false;
    }
    fn is_nonneg(&self, o: &Operand) -> bool {
        match o {
            Operand::Const(Const::Int(i)) => *i >= 0,
            Operand::Local(l) => self.nonneg[*l as usize],
            _ => false,
        }
    }
}

fn step(st: &mut St, s: &Stmt) {
    match &s.kind {
        StmtKind::Assign(d, rv) => {
            let nn = match rv {
                Rvalue::Use(o) => st.is_nonneg(o),
                Rvalue::Binary(BinOp::Add | BinOp::Mul | BinOp::Div, a, b) => {
                    st.is_nonneg(a) && st.is_nonneg(b)
                }
                Rvalue::Binary(BinOp::Rem, a, _) => st.is_nonneg(a),
                Rvalue::Len(_) | Rvalue::Tag(_) | Rvalue::Cast(_) => true,
                _ => false,
            };
            if matches!(
                rv,
                Rvalue::Builtin(crate::sema::Builtin::Push | crate::sema::Builtin::Pop, _)
            ) {
                st.forget_arrays();
            }
            st.kill(*d);
            st.nonneg[*d as usize] = nn;
            match rv {
                Rvalue::Len(Operand::Local(a)) if a != d => {
                    st.lens.insert(*d, *a);
                }
                Rvalue::Binary(BinOp::Lt, Operand::Local(i), Operand::Local(n))
                    if i != d && n != d =>
                {
                    st.cmps.insert(*d, (*i, *n));
                }
                Rvalue::Binary(BinOp::Gt, Operand::Local(n), Operand::Local(i))
                    if i != d && n != d =>
                {
                    st.cmps.insert(*d, (*i, *n));
                }
                _ => {}
            }
        }
        StmtKind::SetField(d, ..) => st.kill(*d),
        StmtKind::SetIndex(..) => {}
    }
}

/// State on the edge `b -> succ_index`, including what a branch condition proves.
fn edge_state(f: &Func, b: usize, mut st: St, succ_index: usize) -> St {
    match &f.blocks[b].term {
        Terminator::Call { dest, .. } => {
            st.forget_arrays();
            st.kill(*dest);
        }
        Terminator::Branch {
            cond: Operand::Local(t),
            ..
        } if succ_index == 0 => {
            if let Some(&(i, n)) = st.cmps.get(t)
                && st.nonneg[i as usize]
                && let Some(&a) = st.lens.get(&n)
            {
                st.facts.insert((i, a));
            }
        }
        _ => {}
    }
    st
}

pub fn bounds_checks(f: &mut Func) -> bool {
    super::cfg::remove_unreachable(f);
    let nl = f.locals.len();
    let mut start = St {
        nonneg: vec![false; nl],
        lens: HashMap::new(),
        cmps: HashMap::new(),
        facts: HashSet::new(),
    };
    start.nonneg.iter_mut().for_each(|x| *x = false);
    let mut entry: Vec<Option<St>> = vec![None; f.blocks.len()];
    entry[0] = Some(start);
    let order = f.rpo();
    let mut changed = true;
    while changed {
        changed = false;
        for &b in &order {
            let Some(mut st) = entry[b as usize].clone() else {
                continue;
            };
            f.blocks[b as usize]
                .stmts
                .iter()
                .for_each(|s| step(&mut st, s));
            for (k, s) in f.blocks[b as usize]
                .term
                .successors()
                .into_iter()
                .enumerate()
            {
                let es = edge_state(f, b as usize, st.clone(), k);
                match &mut entry[s as usize] {
                    slot @ None => {
                        *slot = Some(es);
                        changed = true;
                    }
                    Some(old) => changed |= old.meet(&es),
                }
            }
        }
    }
    let mut any = false;
    for &b in &order {
        let Some(mut st) = entry[b as usize].clone() else {
            continue;
        };
        for s in &mut f.blocks[b as usize].stmts {
            match &mut s.kind {
                StmtKind::Assign(
                    _,
                    Rvalue::Index(Operand::Local(a), Operand::Local(i), checked @ true),
                ) if st.facts.contains(&(*i, *a)) => {
                    *checked = false;
                    any = true;
                }
                StmtKind::SetIndex(Operand::Local(a), Operand::Local(i), _, checked @ true)
                    if st.facts.contains(&(*i, *a)) =>
                {
                    *checked = false;
                    any = true;
                }
                _ => {}
            }
            step(&mut st, s);
        }
    }
    any
}
