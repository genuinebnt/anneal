//! Dead-code and dead-store elimination plus local compaction.
//!
//! A statement is removed only when its destination is dead *and* evaluating it
//! can neither trap nor have an effect: `x = a + b` stays even if `x` is never
//! read, because the overflow trap is observable.

use super::live::*;
use crate::mir::*;

pub fn dce(f: &mut Func) -> bool {
    super::cfg::remove_unreachable(f);
    let mut any = false;
    loop {
        let lv = liveness(f);
        let mut changed = false;
        for b in f.reachable() {
            let mut live = lv.live_out[b as usize].clone();
            let blk = &mut f.blocks[b as usize];
            step_back_term(&blk.term, &mut live);
            let mut keep = vec![true; blk.stmts.len()];
            for (i, s) in blk.stmts.iter().enumerate().rev() {
                let dead = match &s.kind {
                    StmtKind::Assign(d, rv) => {
                        !live.contains(*d) && !rv.may_trap_or_effect_with(&f.locals)
                    }
                    StmtKind::SetField(d, ..) => !live.contains(*d),
                    StmtKind::SetIndex(..) => false,
                };
                if dead {
                    keep[i] = false;
                    changed = true;
                } else {
                    step_back_stmt(s, &mut live);
                }
            }
            let mut it = keep.iter();
            blk.stmts.retain(|_| *it.next().unwrap());
        }
        any |= changed;
        if !changed {
            break;
        }
    }
    any | compact_locals(f)
}

impl Rvalue {
    /// Like [`Rvalue::may_trap_or_effect`] but knows that string `+` is total.
    pub fn may_trap_or_effect_with(&self, locals: &[LocalDecl]) -> bool {
        if let Rvalue::Binary(BinOp::Add, Operand::Local(l), _) = self
            && locals[*l as usize].ty == crate::sema::Ty::Str
        {
            return false;
        }
        if let Rvalue::Binary(BinOp::Add, Operand::Const(Const::Str(_)), _) = self {
            return false;
        }
        self.may_trap_or_effect()
    }
}

use crate::syntax::ast::BinOp;

/// Renumber locals so that unused ones disappear (parameters always stay).
pub fn compact_locals(f: &mut Func) -> bool {
    let mut used = vec![false; f.locals.len()];
    used[..f.params].iter_mut().for_each(|u| *u = true);
    for b in f.reachable() {
        let blk = &f.blocks[b as usize];
        for s in &blk.stmts {
            if let Some(d) = s.def() {
                used[d as usize] = true;
            }
        }
        block_uses(blk, &mut |l| used[l as usize] = true);
        if let Some(d) = blk.term.def() {
            used[d as usize] = true;
        }
    }
    if used.iter().all(|u| *u) {
        return false;
    }
    let mut map = vec![0u32; used.len()];
    let mut n = 0;
    for (i, u) in used.iter().enumerate() {
        if *u {
            map[i] = n;
            n += 1;
        }
    }
    let mut i = 0;
    f.locals.retain(|_| {
        i += 1;
        used[i - 1]
    });
    for blk in &mut f.blocks {
        for s in &mut blk.stmts {
            match &mut s.kind {
                StmtKind::Assign(d, _) | StmtKind::SetField(d, ..) => *d = map[*d as usize],
                StmtKind::SetIndex(..) => {}
            }
            s.for_each_use_mut(&mut |o| remap(o, &map));
        }
        blk.term.for_each_use_mut(&mut |o| remap(o, &map));
        if let Terminator::Call { dest, .. } = &mut blk.term {
            *dest = map[*dest as usize];
        }
    }
    true
}

fn remap(o: &mut Operand, map: &[u32]) {
    if let Operand::Local(l) = o {
        *l = map[*l as usize];
    }
}
