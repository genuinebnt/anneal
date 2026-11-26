//! MIR -> bytecode: block layout, register allocation by live intervals, and
//! instruction selection.
//!
//! **Register allocation.** Each MIR local gets one *interval* (the hull of all
//! positions where it is defined, used or live across a block boundary). A
//! linear scan assigns frame slots and re-uses a slot only after the previous
//! owner's interval has *strictly* ended, so an instruction never reads and
//! writes the same slot unless the MIR said so. Constants that an instruction
//! needs in a register are loaded into scratch slots above the allocated ones.

use super::*;
use crate::mir::{
    self, AggKind, Callee, Const, Func, LocalId, MirProgram, Operand, Rvalue, StmtKind, Terminator,
};
use crate::opt::live::{self, liveness};
use crate::sema::{Builtin, Ty};
use crate::source::Span;
use crate::syntax::ast::{BinOp, UnOp};
use std::collections::HashMap;

pub fn generate(p: &MirProgram) -> Chunk {
    let mut consts = Pool::default();
    let funcs = p.funcs.iter().map(|f| gen_func(f, &mut consts)).collect();
    Chunk {
        funcs,
        consts: consts.items,
        main: p.main,
    }
}

#[derive(Default)]
struct Pool {
    items: Vec<BConst>,
    ints: HashMap<i64, u32>,
    strs: HashMap<std::rc::Rc<str>, u32>,
}

impl Pool {
    fn int(&mut self, v: i64) -> u32 {
        *self.ints.entry(v).or_insert_with(|| {
            self.items.push(BConst::Int(v));
            (self.items.len() - 1) as u32
        })
    }
    fn str(&mut self, s: &std::rc::Rc<str>) -> u32 {
        *self.strs.entry(s.clone()).or_insert_with(|| {
            self.items.push(BConst::Str(s.clone()));
            (self.items.len() - 1) as u32
        })
    }
}

/// Assign a frame slot to every local; returns `(slot per local, slots used)`.
fn allocate(f: &Func, order: &[mir::BlockId], lv: &live::Liveness) -> (Vec<u16>, u16) {
    let nl = f.locals.len();
    let (mut lo, mut hi) = (vec![u32::MAX; nl], vec![0u32; nl]);
    let mut touch = |l: u32, pos: u32| {
        lo[l as usize] = lo[l as usize].min(pos);
        hi[l as usize] = hi[l as usize].max(pos);
    };
    let mut pos = 0u32;
    for &b in order {
        let blk = &f.blocks[b as usize];
        let start = pos;
        for l in lv.live_in[b as usize].iter() {
            touch(l, start);
        }
        for s in &blk.stmts {
            if let Some(d) = s.def() {
                touch(d, pos);
            }
            s.for_each_use(&mut |o| {
                if let Operand::Local(l) = o {
                    touch(*l, pos)
                }
            });
            pos += 1;
        }
        if let Some(d) = blk.term.def() {
            touch(d, pos);
        }
        blk.term.for_each_use(&mut |o| {
            if let Operand::Local(l) = o {
                touch(*l, pos)
            }
        });
        for l in lv.live_out[b as usize].iter() {
            touch(l, pos);
        }
        pos += 1;
    }
    for l in 0..f.params {
        lo[l] = 0;
    }
    let mut slot = vec![0u16; nl];
    let mut next = f.params as u16;
    let mut free: Vec<u16> = Vec::new();
    let mut active: Vec<(u32, u16)> = Vec::new();
    let mut by_start: Vec<usize> = (0..nl).filter(|&l| lo[l] != u32::MAX).collect();
    by_start.sort_by_key(|&l| (lo[l], l));
    for l in by_start {
        active.retain(|&(end, r)| {
            if end < lo[l] {
                free.push(r);
                false
            } else {
                true
            }
        });
        let r = if l < f.params {
            l as u16
        } else {
            free.sort_unstable_by(|a, b| b.cmp(a));
            free.pop().unwrap_or_else(|| {
                next += 1;
                next - 1
            })
        };
        slot[l] = r;
        active.push((hi[l], r));
    }
    (slot, next)
}

struct Gen<'a> {
    f: &'a Func,
    pool: &'a mut Pool,
    slot: Vec<u16>,
    base: u16,
    scratch: u16,
    max_scratch: u16,
    out: BcFunc,
    cur_scope: u32,
    cur_span: Span,
}

fn gen_func(f: &Func, pool: &mut Pool) -> BcFunc {
    let order = f.rpo();
    let lv = liveness(f);
    let (slot, nalloc) = allocate(f, &order, &lv);
    let mut g = Gen {
        f,
        pool,
        slot,
        base: nalloc,
        scratch: 0,
        max_scratch: 0,
        out: BcFunc::empty(&f.name),
        cur_scope: 0,
        cur_span: Span::default(),
    };
    g.out.scopes = f.scopes.clone();
    g.out.nparams = f.params as u16;
    g.out.is_const = f.is_const;
    let mut block_pc = vec![u32::MAX; f.blocks.len()];
    let mut fix: Vec<(usize, mir::BlockId)> = Vec::new();
    let mut switch_fix: Vec<(usize, Vec<mir::BlockId>)> = Vec::new();
    for (oi, &b) in order.iter().enumerate() {
        block_pc[b as usize] = g.out.code.len() as u32;
        let next = order.get(oi + 1).copied();
        g.block(b, next, &lv, &mut fix, &mut switch_fix);
    }
    // Scratch slots are reused by every instruction, so one watermark suffices.
    g.out.nregs = nalloc + g.max_scratch;
    for (pc, b) in fix {
        set_target(&mut g.out.code[pc], block_pc[b as usize]);
    }
    for (si, blocks) in switch_fix {
        let t = &mut g.out.switches[si];
        let n = t.cases.len();
        for (k, b) in blocks.iter().enumerate() {
            let pc = block_pc[*b as usize];
            if k < n {
                t.cases[k].1 = pc;
            } else {
                t.default = pc;
            }
        }
    }
    g.out
}

/// The comparison that holds exactly when `op` does not.
fn negate_cmp(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Ge,
        BinOp::Ge => BinOp::Lt,
        BinOp::Gt => BinOp::Le,
        _ => BinOp::Gt,
    }
}

/// `a op b` rewritten as `b op' a`.
fn swap_cmp(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Gt,
        BinOp::Gt => BinOp::Lt,
        BinOp::Le => BinOp::Ge,
        _ => BinOp::Le,
    }
}

fn set_target(i: &mut Instr, pc: u32) {
    match i {
        Instr::Jmp { t }
        | Instr::JmpIf { t, .. }
        | Instr::JmpIfNot { t, .. }
        | Instr::JmpLt { t, .. }
        | Instr::JmpGe { t, .. }
        | Instr::JmpLtI { t, .. }
        | Instr::JmpGeI { t, .. }
        | Instr::JmpGtI { t, .. }
        | Instr::JmpLeI { t, .. } => *t = Tgt(pc),
        _ => {}
    }
}

impl Gen<'_> {
    fn push(&mut self, i: Instr) {
        self.out.code.push(i);
        self.out.pc_scope.push(self.cur_scope);
        self.out.pc_span.push(self.cur_span);
    }
    fn local(&self, l: LocalId) -> Reg {
        Reg(self.slot[l as usize])
    }
    fn scratch_reg(&mut self) -> Reg {
        self.scratch += 1;
        self.max_scratch = self.max_scratch.max(self.scratch);
        Reg(self.base + self.scratch - 1)
    }
    /// A register holding `o` (constants are loaded into a scratch slot).
    fn reg(&mut self, o: &Operand) -> Reg {
        match o {
            Operand::Local(l) => self.local(*l),
            Operand::Const(c) => {
                let r = self.scratch_reg();
                self.load_const(r, c);
                r
            }
        }
    }
    fn load_const(&mut self, d: Reg, c: &Const) {
        let i = match c {
            Const::Int(v) => match i32::try_from(*v) {
                Ok(v) => Instr::LoadI { d, v: Imm(v) },
                Err(_) => Instr::LoadK {
                    d,
                    k: Kst(self.pool.int(*v)),
                },
            },
            Const::Bool(b) => Instr::LoadB { d, v: Flag(*b) },
            Const::Unit => Instr::LoadUnit { d },
            Const::Str(s) => Instr::LoadK {
                d,
                k: Kst(self.pool.str(s)),
            },
            Const::Fn(f) => Instr::LoadFn { d, f: Fid(*f) },
        };
        self.push(i);
    }
    fn list(&mut self, ops: &[Operand]) -> Lst {
        let regs: Vec<u16> = ops.iter().map(|o| self.reg(o).0).collect();
        self.out.lists.push(regs.into_boxed_slice());
        Lst((self.out.lists.len() - 1) as u32)
    }
    fn ty(&self, o: &Operand) -> Ty {
        match o {
            Operand::Local(l) => self.f.local_ty(*l).clone(),
            Operand::Const(Const::Int(_)) => Ty::Int,
            Operand::Const(Const::Str(_)) => Ty::Str,
            Operand::Const(Const::Bool(_)) => Ty::Bool,
            _ => Ty::Unit,
        }
    }

    fn block(
        &mut self,
        b: mir::BlockId,
        next: Option<mir::BlockId>,
        lv: &live::Liveness,
        fix: &mut Vec<(usize, mir::BlockId)>,
        sfix: &mut Vec<(usize, Vec<mir::BlockId>)>,
    ) {
        let blk = &self.f.blocks[b as usize];
        // Fuse `t = a < b; branch t` into one compare-and-branch when `t` has no other use.
        let mut stmts = &blk.stmts[..];
        let mut fused: Option<(BinOp, Operand, Operand)> = None;
        if let (
            Terminator::Branch {
                cond: Operand::Local(t),
                ..
            },
            Some(mir::Stmt {
                kind:
                    StmtKind::Assign(
                        d,
                        Rvalue::Binary(op @ (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge), x, y),
                    ),
                ..
            }),
        ) = (&blk.term, stmts.last())
            && d == t
            && !lv.live_out[b as usize].contains(*t)
            && self.ty(x) == Ty::Int
        {
            fused = Some((*op, x.clone(), y.clone()));
            stmts = &stmts[..stmts.len() - 1];
        }
        for s in stmts {
            self.cur_scope = s.scope;
            self.cur_span = s.span;
            self.scratch = 0;
            self.stmt(s);
        }
        self.scratch = 0;
        let term_scope = blk.stmts.last().map_or(self.cur_scope, |s| s.scope);
        let term_span = blk.stmts.last().map_or(self.cur_span, |s| s.span);
        self.cur_span = term_span;
        match &blk.term {
            Terminator::Goto(t) => {
                self.cur_scope = term_scope;
                if Some(*t) != next {
                    fix.push((self.out.code.len(), *t));
                    self.push(Instr::Jmp { t: Tgt(0) });
                }
            }
            Terminator::Branch {
                cond,
                then_bb,
                else_bb,
            } => {
                self.cur_scope = term_scope;
                self.branch(cond, *then_bb, *else_bb, next, fused, fix);
            }
            Terminator::Switch {
                value,
                cases,
                default,
            } => {
                self.cur_scope = term_scope;
                let r = self.reg(value);
                self.out.switches.push(SwitchTable {
                    cases: cases.iter().map(|c| (c.0, 0)).collect(),
                    default: 0,
                });
                let si = self.out.switches.len() - 1;
                let mut targets: Vec<mir::BlockId> = cases.iter().map(|c| c.1).collect();
                targets.push(*default);
                sfix.push((si, targets));
                self.push(Instr::Switch {
                    r,
                    s: Swt(si as u32),
                });
            }
            Terminator::Call {
                callee,
                args,
                dest,
                target,
                scope,
                span,
            } => {
                self.cur_scope = *scope;
                self.cur_span = *span;
                let list = self.list(args);
                let d = self.local(*dest);
                match callee {
                    Callee::Direct(f) => self.push(Instr::Call {
                        d,
                        f: Fid(*f),
                        args: list,
                    }),
                    Callee::Indirect(o) => {
                        let fr = self.reg(o);
                        self.push(Instr::CallInd { d, fr, args: list })
                    }
                }
                if Some(*target) != next {
                    fix.push((self.out.code.len(), *target));
                    self.push(Instr::Jmp { t: Tgt(0) });
                }
            }
            Terminator::Return(o) => {
                self.cur_scope = term_scope;
                let a = self.reg(o);
                self.push(Instr::Ret { a });
            }
            Terminator::Unreachable => {
                self.cur_scope = term_scope;
                self.push(Instr::Unreachable {});
            }
        }
    }

    fn branch(
        &mut self,
        cond: &Operand,
        t: mir::BlockId,
        e: mir::BlockId,
        next: Option<mir::BlockId>,
        fused: Option<(BinOp, Operand, Operand)>,
        fix: &mut Vec<(usize, mir::BlockId)>,
    ) {
        if let Some((op, x, y)) = fused {
            self.fused_branch(op, x, y, t, e, next, fix);
            return;
        }
        let c = self.reg(cond);
        let pc = self.out.code.len();
        if Some(t) == next {
            fix.push((pc, e));
            self.push(Instr::JmpIfNot { c, t: Tgt(0) });
        } else {
            fix.push((pc, t));
            self.push(Instr::JmpIf { c, t: Tgt(0) });
            if Some(e) != next {
                fix.push((pc + 1, e));
                self.push(Instr::Jmp { t: Tgt(0) });
            }
        }
    }

    /// Compare-and-branch. Falls through to `next` when possible, so at most
    /// one jump is emitted for the common loop shapes.
    #[allow(clippy::too_many_arguments)]
    fn fused_branch(
        &mut self,
        op: BinOp,
        x: Operand,
        y: Operand,
        t: mir::BlockId,
        e: mir::BlockId,
        next: Option<mir::BlockId>,
        fix: &mut Vec<(usize, mir::BlockId)>,
    ) {
        let is_const = |o: &Operand| matches!(o, Operand::Const(_));
        // Put a constant on the right so it can become an immediate.
        let (op, x, y) = if is_const(&x) && !is_const(&y) {
            (swap_cmp(op), y, x)
        } else {
            (op, x, y)
        };
        let imm = match &y {
            Operand::Const(Const::Int(v)) => i32::try_from(*v).ok(),
            _ => None,
        };
        let jump_on_true = Some(t) != next;
        let (op, target) = if jump_on_true {
            (op, t)
        } else {
            (negate_cmp(op), e)
        };
        let a = self.reg(&x);
        let ins = match imm {
            Some(v) => {
                let (a, v, t) = (a, Imm(v), Tgt(0));
                match op {
                    BinOp::Lt => Instr::JmpLtI { a, v, t },
                    BinOp::Ge => Instr::JmpGeI { a, v, t },
                    BinOp::Gt => Instr::JmpGtI { a, v, t },
                    _ => Instr::JmpLeI { a, v, t },
                }
            }
            None => {
                let b = self.reg(&y);
                match op {
                    BinOp::Lt => Instr::JmpLt { a, b, t: Tgt(0) },
                    BinOp::Ge => Instr::JmpGe { a, b, t: Tgt(0) },
                    BinOp::Gt => Instr::JmpLt {
                        a: b,
                        b: a,
                        t: Tgt(0),
                    },
                    _ => Instr::JmpGe {
                        a: b,
                        b: a,
                        t: Tgt(0),
                    },
                }
            }
        };
        fix.push((self.out.code.len(), target));
        self.push(ins);
        if jump_on_true && Some(e) != next {
            fix.push((self.out.code.len(), e));
            self.push(Instr::Jmp { t: Tgt(0) });
        }
    }

    fn stmt(&mut self, s: &mir::Stmt) {
        match &s.kind {
            StmtKind::Assign(d, rv) => {
                let d = self.local(*d);
                self.rvalue(d, rv);
            }
            StmtKind::SetField(d, i, v) => {
                let v = self.reg(v);
                let r = self.local(*d);
                self.push(Instr::SetField {
                    r,
                    i: Idx(*i as u16),
                    v,
                });
            }
            StmtKind::SetIndex(a, i, v, checked) => {
                let (a, i, v) = (self.reg(a), self.reg(i), self.reg(v));
                self.push(if *checked {
                    Instr::SetIndex { a, i, v }
                } else {
                    Instr::SetIndexU { a, i, v }
                });
            }
        }
    }

    fn rvalue(&mut self, d: Reg, rv: &Rvalue) {
        match rv {
            Rvalue::Use(Operand::Const(c)) => self.load_const(d, c),
            Rvalue::Use(Operand::Local(l)) => {
                let s = self.local(*l);
                if s != d {
                    self.push(Instr::Move { d, s });
                }
            }
            Rvalue::Unary(UnOp::Neg, a) => {
                let a = self.reg(a);
                self.push(Instr::Neg { d, a })
            }
            Rvalue::Unary(UnOp::Not, a) => {
                let a = self.reg(a);
                self.push(Instr::Not { d, a })
            }
            Rvalue::Binary(op, a, b) => self.binary(d, *op, a, b),
            Rvalue::Agg(kind, ops) => {
                let list = self.list(ops);
                self.push(match kind {
                    AggKind::Array => Instr::MkArray { d, list },
                    AggKind::Variant(_, v) => Instr::MkAgg {
                        d,
                        tag: Idx(*v as u16),
                        list,
                    },
                    AggKind::Tuple | AggKind::Struct(_) => Instr::MkAgg {
                        d,
                        tag: Idx(0),
                        list,
                    },
                });
            }
            Rvalue::Field(a, i) => {
                let a = self.reg(a);
                self.push(Instr::GetField {
                    d,
                    a,
                    i: Idx(*i as u16),
                })
            }
            Rvalue::Index(a, i, checked) => {
                let (a, i) = (self.reg(a), self.reg(i));
                self.push(if *checked {
                    Instr::Index { d, a, i }
                } else {
                    Instr::IndexU { d, a, i }
                })
            }
            Rvalue::Len(a) => {
                let a = self.reg(a);
                self.push(Instr::Len { d, a })
            }
            Rvalue::Tag(a) => {
                let a = self.reg(a);
                self.push(Instr::Tag { d, a })
            }
            Rvalue::Cast(a) => {
                let a = self.reg(a);
                self.push(Instr::Cast { d, a })
            }
            Rvalue::Builtin(b, ops) => self.builtin(d, *b, ops),
            Rvalue::WithField(a, i, v) => {
                // Only reachable for SSA input, which the pipeline converts back first; kept total.
                let (a, v) = (self.reg(a), self.reg(v));
                if a != d {
                    self.push(Instr::Move { d, s: a });
                }
                self.push(Instr::SetField {
                    r: d,
                    i: Idx(*i as u16),
                    v,
                });
            }
        }
    }

    fn binary(&mut self, d: Reg, op: BinOp, a: &Operand, b: &Operand) {
        let is_str = self.ty(a) == Ty::Str;
        let is_int = self.ty(a) == Ty::Int;
        // Immediate forms keep the exact trap message of the register forms.
        let (a, b) = match (op, a, b) {
            (BinOp::Add | BinOp::Mul, Operand::Const(_), Operand::Local(_)) if is_int => (b, a), // commutative: constant to the right
            _ => (a, b),
        };
        if let (BinOp::Add | BinOp::Sub | BinOp::Mul, Operand::Const(Const::Int(v))) = (op, b)
            && is_int
            && let Ok(v) = i32::try_from(*v)
        {
            let a = self.reg(a);
            self.push(match op {
                BinOp::Add => Instr::AddI { d, a, v: Imm(v) },
                BinOp::Sub => Instr::SubI { d, a, v: Imm(v) },
                _ => Instr::MulI { d, a, v: Imm(v) },
            });
            return;
        }
        let (a, b) = (self.reg(a), self.reg(b));
        let i = match op {
            BinOp::Add if is_str => Instr::Concat { d, a, b },
            BinOp::Add => Instr::Add { d, a, b },
            BinOp::Sub => Instr::Sub { d, a, b },
            BinOp::Mul => Instr::Mul { d, a, b },
            BinOp::Div => Instr::Div { d, a, b },
            BinOp::Rem => Instr::Rem { d, a, b },
            BinOp::Eq if is_int => Instr::EqI { d, a, b },
            BinOp::Ne if is_int => Instr::NeI { d, a, b },
            BinOp::Eq => Instr::Eq { d, a, b },
            BinOp::Ne => Instr::Ne { d, a, b },
            BinOp::Lt if is_str => Instr::LtS { d, a, b },
            BinOp::Le if is_str => Instr::LeS { d, a, b },
            BinOp::Gt if is_str => Instr::GtS { d, a, b },
            BinOp::Ge if is_str => Instr::GeS { d, a, b },
            BinOp::Lt => Instr::LtI { d, a, b },
            BinOp::Le => Instr::LeI { d, a, b },
            BinOp::Gt => Instr::GtI { d, a, b },
            BinOp::Ge => Instr::GeI { d, a, b },
            BinOp::And | BinOp::Or => Instr::Unreachable {},
        };
        self.push(i);
    }

    fn builtin(&mut self, d: Reg, b: Builtin, ops: &[Operand]) {
        let regs: Vec<Reg> = ops.iter().map(|o| self.reg(o)).collect();
        let i = match (b, regs.as_slice()) {
            (Builtin::Print, [a]) => Instr::Print { a: *a },
            (Builtin::Println, []) => Instr::PrintlnEmpty {},
            (Builtin::Println, [a]) => Instr::Println { a: *a },
            (Builtin::Assert, [c]) => Instr::Assert { c: *c },
            (Builtin::Assert, [c, m]) => Instr::AssertMsg { c: *c, m: *m },
            (Builtin::Panic, [m]) => Instr::Panic { m: *m },
            (Builtin::ToStr, [a]) => Instr::ToStr { d, a: *a },
            (Builtin::Push, [a, v]) => Instr::Push { a: *a, v: *v },
            (Builtin::Pop, [a]) => Instr::Pop { d, a: *a },
            (Builtin::Chars, [a]) => Instr::Chars { d, a: *a },
            (Builtin::Len, [a]) => Instr::Len { d, a: *a },
            _ => Instr::Unreachable {},
        };
        self.push(i);
    }
}
