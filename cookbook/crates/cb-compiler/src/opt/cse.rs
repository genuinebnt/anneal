//! Local value numbering (common-subexpression elimination within a block).
//!
//! Every local gets a *value number*; a pure rvalue is keyed by the value
//! numbers of its operands. If the same key was computed earlier in the block
//! and the holder still carries that value, the rvalue becomes a copy.
//! Replacing a *trapping* operation by a copy is safe: the first occurrence
//! already executed (and would have trapped). Array reads (`Index`, `Len`) are
//! forgotten at every store, `push`/`pop` and call because arrays alias.
//! Aggregates and `chars` are never merged: array construction has identity.

use crate::mir::*;
use crate::syntax::ast::{BinOp, UnOp};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash)]
enum VOp {
    C(Const),
    N(u32),
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Bin(BinOp, VOp, VOp),
    Un(UnOp, VOp),
    Field(VOp, u32),
    Index(VOp, VOp),
    Len(VOp),
    Tag(VOp),
    Cast(VOp),
    ToStr(VOp),
}

impl Key {
    fn reads_memory(&self) -> bool {
        matches!(self, Key::Index(..) | Key::Len(_))
    }
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

pub fn cse(f: &mut Func) -> bool {
    let mut changed = false;
    for b in f.reachable() {
        changed |= cse_block(f, b as usize);
    }
    changed
}

fn cse_block(f: &mut Func, b: usize) -> bool {
    let nl = f.locals.len() as u32;
    let mut vn: Vec<u32> = (0..nl).collect();
    let mut next = nl;
    let mut fresh = |vn: &mut Vec<u32>, l: LocalId| {
        vn[l as usize] = next;
        next += 1;
    };
    let mut table: HashMap<Key, (LocalId, u32)> = HashMap::new();
    let mut changed = false;
    let mut stmts = std::mem::take(&mut f.blocks[b].stmts);
    for s in &mut stmts {
        let invalidates = s.writes_memory();
        match &mut s.kind {
            StmtKind::Assign(d, rv) => {
                let d = *d;
                let vop = |o: &Operand, vn: &Vec<u32>| match o {
                    Operand::Const(c) => VOp::C(c.clone()),
                    Operand::Local(l) => VOp::N(vn[*l as usize]),
                };
                let key = make_key(f, rv, &vn, &vop);
                if let Some(k) = &key
                    && let Some(&(h, hv)) = table.get(k)
                    && h != d
                    && vn[h as usize] == hv
                {
                    let new_vn = vn[h as usize];
                    *rv = Rvalue::Use(Operand::Local(h));
                    vn[d as usize] = new_vn;
                    changed = true;
                    continue;
                }
                if let Rvalue::Use(o) = rv {
                    vn[d as usize] = match o {
                        Operand::Local(l) => vn[*l as usize],
                        Operand::Const(_) => {
                            fresh(&mut vn, d);
                            vn[d as usize]
                        }
                    };
                } else {
                    fresh(&mut vn, d);
                }
                if invalidates {
                    table.retain(|k, _| !k.reads_memory());
                }
                // A key is only worth remembering while its holder survives.
                if let Some(k) = key
                    && !operand_mentions(rv, d)
                {
                    table.insert(k, (d, vn[d as usize]));
                }
            }
            StmtKind::SetField(d, ..) => fresh(&mut vn, *d),
            StmtKind::SetIndex(..) => table.retain(|k, _| !k.reads_memory()),
        }
    }
    f.blocks[b].stmts = stmts;
    changed
}

fn operand_mentions(rv: &Rvalue, d: LocalId) -> bool {
    let mut m = false;
    rv.for_each_operand(&mut |o| m |= *o == Operand::Local(d));
    m
}

fn make_key(
    f: &Func,
    rv: &Rvalue,
    vn: &Vec<u32>,
    vop: &dyn Fn(&Operand, &Vec<u32>) -> VOp,
) -> Option<Key> {
    let v = |o: &Operand| vop(o, vn);
    Some(match rv {
        Rvalue::Binary(op, a, b) => {
            let (mut x, mut y) = (v(a), v(b));
            let commutes = matches!(op, BinOp::Eq | BinOp::Ne)
                || (matches!(op, BinOp::Add | BinOp::Mul) && !f.operand_is_str(a));
            if commutes && hash_of(&x) > hash_of(&y) {
                std::mem::swap(&mut x, &mut y);
            }
            Key::Bin(*op, x, y)
        }
        Rvalue::Unary(op, a) => Key::Un(*op, v(a)),
        Rvalue::Field(a, i) => Key::Field(v(a), *i),
        Rvalue::Index(a, i, _) => Key::Index(v(a), v(i)),
        Rvalue::Len(a) => Key::Len(v(a)),
        Rvalue::Tag(a) => Key::Tag(v(a)),
        Rvalue::Cast(a) => Key::Cast(v(a)),
        Rvalue::Builtin(crate::sema::Builtin::ToStr, xs) => Key::ToStr(v(&xs[0])),
        _ => return None,
    })
}
