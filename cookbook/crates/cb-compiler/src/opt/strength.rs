//! Algebraic simplification and strength reduction.
//!
//! Only rewrites that preserve the trap behaviour are applied:
//! `x + 0`, `x - 0`, `x * 1`, `x / 1` become `x` (none can trap, `MIN / 1` is
//! fine); `x * 0` and `x % 1` become `0`; `x * 2` becomes `x + x` (overflows
//! exactly when the multiplication does, and an add is cheaper than a mul);
//! `x - x` is `0`; `x == x` is `true` for any type without function values.
//! `x / -1` and `x % -1` are *not* touched: they trap on `MIN`.

use crate::mir::*;
use crate::syntax::ast::BinOp;

fn is_int(o: &Operand, n: i64) -> bool {
    matches!(o, Operand::Const(Const::Int(i)) if *i == n)
}

pub fn simplify(op: BinOp, a: &Operand, b: &Operand, is_str: bool) -> Option<Rvalue> {
    use BinOp::*;
    let same = a == b && matches!(a, Operand::Local(_));
    let int = |i| Rvalue::Use(Operand::int(i));
    let boolean = |v| Rvalue::Use(Operand::Const(Const::Bool(v)));
    match op {
        Add if !is_str && is_int(b, 0) => Some(Rvalue::Use(a.clone())),
        Add if !is_str && is_int(a, 0) => Some(Rvalue::Use(b.clone())),
        Sub if is_int(b, 0) => Some(Rvalue::Use(a.clone())),
        Sub if same => Some(int(0)),
        Mul if is_int(b, 1) => Some(Rvalue::Use(a.clone())),
        Mul if is_int(a, 1) => Some(Rvalue::Use(b.clone())),
        Mul if is_int(b, 0) || is_int(a, 0) => Some(int(0)),
        Mul if is_int(b, 2) => Some(Rvalue::Binary(Add, a.clone(), a.clone())),
        Mul if is_int(a, 2) => Some(Rvalue::Binary(Add, b.clone(), b.clone())),
        Div if is_int(b, 1) => Some(Rvalue::Use(a.clone())),
        Rem if is_int(b, 1) => Some(int(0)),
        Eq | Le | Ge if same => Some(boolean(true)),
        Ne | Lt | Gt if same => Some(boolean(false)),
        _ => None,
    }
}

pub fn strength(f: &mut Func) -> bool {
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        for si in 0..f.blocks[bi].stmts.len() {
            let new = match &f.blocks[bi].stmts[si].kind {
                StmtKind::Assign(_, Rvalue::Binary(op, a, b)) => {
                    // `x == x` on a function type is rejected by the checker; `Eq` on aggregates is still reflexive.
                    simplify(*op, a, b, f.operand_is_str(a))
                }
                _ => None,
            };
            if let Some(rv) = new
                && let StmtKind::Assign(_, slot) = &mut f.blocks[bi].stmts[si].kind
            {
                *slot = rv;
                changed = true;
            }
        }
    }
    changed
}
