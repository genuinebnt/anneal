//! Constant folding / propagation and copy propagation, one forward dataflow.
//!
//! Lattice per local: `Top` (not yet seen) > `Const(c)` | `Copy(y)` > `Bot`.
//! `Copy(y)` means "holds the same value as local `y`"; it is killed whenever
//! `y` (or the local itself) is reassigned. Folding never removes a trap: an
//! overflowing or dividing-by-zero operation is left in place.

use crate::mir::*;
use crate::syntax::ast::{BinOp, UnOp};

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Top,
    Const(Const),
    Copy(LocalId),
    Bot,
}

type State = Vec<Val>;

#[derive(Clone, Copy)]
pub struct Mode {
    pub consts: bool,
    pub copies: bool,
}

fn meet(a: &Val, b: &Val) -> Val {
    match (a, b) {
        (Val::Top, x) | (x, Val::Top) => x.clone(),
        (x, y) if x == y => x.clone(),
        _ => Val::Bot,
    }
}

/// Replace a use by what is known about it.
fn subst(st: &State, mode: Mode, o: &Operand) -> Operand {
    if let Operand::Local(l) = o {
        match &st[*l as usize] {
            Val::Const(c) if mode.consts => return Operand::Const(c.clone()),
            Val::Copy(y) if mode.copies => return Operand::Local(*y),
            _ => {}
        }
    }
    o.clone()
}

fn kill(st: &mut State, d: LocalId) {
    for v in st.iter_mut() {
        if *v == Val::Copy(d) {
            *v = Val::Bot;
        }
    }
    st[d as usize] = Val::Bot;
}

fn as_const(o: &Operand) -> Option<&Const> {
    match o {
        Operand::Const(c) => Some(c),
        _ => None,
    }
}

/// Evaluate an rvalue whose operands are all constants, unless it would trap.
pub fn fold(rv: &Rvalue) -> Option<Const> {
    use Const::*;
    match rv {
        Rvalue::Use(Operand::Const(c)) => Some(c.clone()),
        Rvalue::Unary(UnOp::Neg, a) => match as_const(a)? {
            Int(i) => i.checked_neg().map(Int),
            _ => None,
        },
        Rvalue::Unary(UnOp::Not, a) => match as_const(a)? {
            Bool(b) => Some(Bool(!b)),
            _ => None,
        },
        Rvalue::Cast(a) => match as_const(a)? {
            Bool(b) => Some(Int(*b as i64)),
            _ => None,
        },
        Rvalue::Len(a) => match as_const(a)? {
            Str(s) => Some(Int(s.len() as i64)),
            _ => None,
        },
        Rvalue::Binary(op, a, b) => fold_binary(*op, as_const(a)?, as_const(b)?),
        _ => None,
    }
}

fn fold_binary(op: BinOp, a: &Const, b: &Const) -> Option<Const> {
    use Const::*;
    match (a, b) {
        (Int(x), Int(y)) => Some(match op {
            BinOp::Add => Int(x.checked_add(*y)?),
            BinOp::Sub => Int(x.checked_sub(*y)?),
            BinOp::Mul => Int(x.checked_mul(*y)?),
            BinOp::Div if *y != 0 => Int(x.checked_div(*y)?),
            BinOp::Rem if *y != 0 => Int(x.checked_rem(*y)?),
            BinOp::Eq => Bool(x == y),
            BinOp::Ne => Bool(x != y),
            BinOp::Lt => Bool(x < y),
            BinOp::Le => Bool(x <= y),
            BinOp::Gt => Bool(x > y),
            BinOp::Ge => Bool(x >= y),
            _ => return None,
        }),
        (Bool(x), Bool(y)) => match op {
            BinOp::Eq => Some(Bool(x == y)),
            BinOp::Ne => Some(Bool(x != y)),
            _ => None,
        },
        (Str(x), Str(y)) => match op {
            BinOp::Add if x.len() + y.len() < 4096 => Some(Str(format!("{x}{y}").into())),
            BinOp::Eq => Some(Bool(x == y)),
            BinOp::Ne => Some(Bool(x != y)),
            BinOp::Lt => Some(Bool(x < y)),
            BinOp::Le => Some(Bool(x <= y)),
            BinOp::Gt => Some(Bool(x > y)),
            BinOp::Ge => Some(Bool(x >= y)),
            _ => None,
        },
        (Unit, Unit) => match op {
            BinOp::Eq => Some(Bool(true)),
            BinOp::Ne => Some(Bool(false)),
            _ => None,
        },
        _ => None,
    }
}

/// Value of the destination after `d = rv` (operands already substituted).
fn eval(rv: &Rvalue, mode: Mode) -> Val {
    if mode.consts
        && let Some(c) = fold(rv)
    {
        return Val::Const(c);
    }
    match rv {
        Rvalue::Use(Operand::Local(y)) if mode.copies => Val::Copy(*y),
        _ => Val::Bot,
    }
}

fn apply_stmt(st: &mut State, s: &Stmt, mode: Mode) {
    match &s.kind {
        StmtKind::Assign(d, rv) => {
            let mut rv = rv.clone();
            rv.for_each_operand_mut(&mut |o| *o = subst(st, mode, o));
            // `x = x` changes nothing (and must not become a self-copy).
            if matches!(rv, Rvalue::Use(Operand::Local(y)) if y == *d) {
                return;
            }
            let v = eval(&rv, mode);
            kill(st, *d);
            st[*d as usize] = v;
        }
        StmtKind::SetField(d, ..) => kill(st, *d),
        StmtKind::SetIndex(..) => {}
    }
}

fn entry_states(f: &Func, mode: Mode) -> Vec<Option<State>> {
    let mut entry: Vec<Option<State>> = vec![None; f.blocks.len()];
    let mut start = vec![Val::Top; f.locals.len()];
    start[..f.params].iter_mut().for_each(|v| *v = Val::Bot);
    entry[0] = Some(start);
    let order = f.rpo();
    let mut changed = true;
    while changed {
        changed = false;
        for &b in &order {
            let Some(mut st) = entry[b as usize].clone() else {
                continue;
            };
            let blk = &f.blocks[b as usize];
            for s in &blk.stmts {
                apply_stmt(&mut st, s, mode);
            }
            if let Some(d) = blk.term.def() {
                kill(&mut st, d);
            }
            for s in blk.term.successors() {
                match &mut entry[s as usize] {
                    slot @ None => {
                        *slot = Some(st.clone());
                        changed = true;
                    }
                    Some(old) => {
                        for (o, n) in old.iter_mut().zip(&st) {
                            let m = meet(o, n);
                            if m != *o {
                                *o = m;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    entry
}

/// Run the pass to a fixed point (folding a branch exposes new constants);
/// returns whether the function changed.
pub fn propagate(f: &mut Func, mode: Mode) -> bool {
    let mut any = false;
    for _ in 0..8 {
        if !propagate_once(f, mode) {
            break;
        }
        any = true;
    }
    any
}

fn propagate_once(f: &mut Func, mode: Mode) -> bool {
    super::cfg::remove_unreachable(f);
    let entry = entry_states(f, mode);
    let mut changed = false;
    for b in 0..f.blocks.len() {
        let Some(mut st) = entry[b].clone() else {
            continue;
        };
        let blk = &mut f.blocks[b];
        for s in &mut blk.stmts {
            let before = s.clone();
            s.for_each_use_mut(&mut |o| *o = subst(&st, mode, o));
            if mode.consts
                && let StmtKind::Assign(_, rv) = &mut s.kind
                && !matches!(rv, Rvalue::Use(_))
                && let Some(c) = fold(rv)
            {
                *rv = Rvalue::Use(Operand::Const(c));
            }
            changed |= *s != before;
            apply_stmt(&mut st, s, mode);
        }
        let before = blk.term.clone();
        blk.term.for_each_use_mut(&mut |o| *o = subst(&st, mode, o));
        changed |= blk.term != before;
    }
    if mode.consts {
        changed |= super::cfg::simplify_cfg(f);
    }
    changed
}
