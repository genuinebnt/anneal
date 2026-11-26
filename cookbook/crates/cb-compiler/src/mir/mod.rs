//! MIR: a monomorphic, typed mid-level IR with an explicit control-flow graph.
//!
//! * A [`Func`] is a vector of [`Block`]s (block 0 is the entry). A block is a
//!   list of [`Stmt`]s followed by one [`Terminator`].
//! * Locals are typed slots. Parameters are the first `params` locals. A local
//!   may be assigned many times (this is *not* SSA), which keeps lowering of
//!   `let mut`, loops and `match` bindings simple; the optimiser uses dataflow
//!   analyses instead of phi nodes.
//! * **Memory model.** There are no explicit drops. At run time heap values
//!   (strings, aggregates, arrays) are reference counted (`Rc`) by the VM;
//!   tuples, structs and enums have *value semantics* through copy-on-write
//!   (`SetField` clones a shared aggregate first), arrays have *reference
//!   semantics* (an `Rc<RefCell<Vec<_>>>` handle that is copied by `Use`).
//!   Reference cycles (only possible through arrays of recursive enums) leak
//!   until the process exits; they never cause unsafety.
//! * **Traps.** `Binary(Add|Sub|Mul|Div|Rem)`, `Unary(Neg)`, checked `Index`,
//!   `SetIndex`, and the built-ins `assert`, `panic`, `pop` can trap; passes
//!   must never delete, duplicate or reorder them across side effects.
//! * **Scopes.** Every statement carries a scope id into [`Func::scopes`]; the
//!   scope tree records *logical* call frames, so inlining keeps run-time stack
//!   traces identical to the unoptimised program.

pub mod lower;
pub mod print;
pub mod verify;

use crate::sema::hir::{EnumInfo, StructInfo};
use crate::sema::{Builtin, DefId, Ty};
use crate::source::Span;
use crate::syntax::ast::{BinOp, UnOp};
use std::collections::HashMap;
use std::rc::Rc;

pub type LocalId = u32;
pub type BlockId = u32;
pub type FuncId = u32;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Const {
    Int(i64),
    Bool(bool),
    Unit,
    Str(Rc<str>),
    Fn(FuncId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Const(Const),
    Local(LocalId),
}

impl Operand {
    pub const UNIT: Operand = Operand::Const(Const::Unit);
    pub fn int(i: i64) -> Operand {
        Operand::Const(Const::Int(i))
    }
    pub fn as_local(&self) -> Option<LocalId> {
        match self {
            Operand::Local(l) => Some(*l),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggKind {
    Tuple,
    Array,
    Struct(DefId),
    /// Enum definition and variant index.
    Variant(DefId, u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Rvalue {
    Use(Operand),
    Unary(UnOp, Operand),
    /// Never `And`/`Or`: short-circuit operators are control flow.
    Binary(BinOp, Operand, Operand),
    Agg(AggKind, Vec<Operand>),
    /// Field of a tuple, struct or enum payload (declaration order).
    Field(Operand, u32),
    /// `array[index]`; the flag says whether the bounds check is still needed.
    Index(Operand, Operand, bool),
    /// Length of an array or the byte length of a string.
    Len(Operand),
    /// Variant index of an enum value, as an `int`.
    Tag(Operand),
    Builtin(Builtin, Vec<Operand>),
    /// `bool as int`.
    Cast(Operand),
    /// SSA form only: a copy of the aggregate with field `i` replaced
    /// (`SetField` has no SSA equivalent because it redefines its operand).
    WithField(Operand, u32, Operand),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Assign(LocalId, Rvalue),
    /// In-place update of an aggregate held in a local (copy-on-write).
    SetField(LocalId, u32, Operand),
    /// `array[index] = value`; the flag says whether the bounds check is needed.
    SetIndex(Operand, Operand, Operand, bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub scope: u32,
    /// Source span of the expression this statement was lowered from
    /// (`Span::default()` when synthesised). Passes keep the span of the
    /// statement they rewrite; the VM reports it for traps.
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Callee {
    Direct(FuncId),
    Indirect(Operand),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Terminator {
    Goto(BlockId),
    Branch {
        cond: Operand,
        then_bb: BlockId,
        else_bb: BlockId,
    },
    /// Jump on an integer value (used with `Tag`); `default` when no case matches.
    Switch {
        value: Operand,
        cases: Vec<(i64, BlockId)>,
        default: BlockId,
    },
    Call {
        callee: Callee,
        args: Vec<Operand>,
        dest: LocalId,
        target: BlockId,
        scope: u32,
        span: Span,
    },
    Return(Operand),
    Unreachable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// SSA form only (see [`Func::ssa`]); always empty otherwise. Phis execute in parallel on block entry.
    pub phis: Vec<Phi>,
    pub stmts: Vec<Stmt>,
    pub term: Terminator,
}

/// `dest = phi [pred: operand, ...]`: one argument per distinct predecessor block.
#[derive(Clone, Debug, PartialEq)]
pub struct Phi {
    pub dest: LocalId,
    pub args: Vec<(BlockId, Operand)>,
}

impl Block {
    pub fn new(stmts: Vec<Stmt>, term: Terminator) -> Block {
        Block {
            phis: Vec::new(),
            stmts,
            term,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalDecl {
    pub ty: Ty,
    /// Source name for user variables; `None` for compiler temporaries.
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scope {
    pub name: String,
    pub parent: Option<u32>,
    /// Where this logical frame was called from, when the call was inlined
    /// (`Span::default()` otherwise).
    pub call_site: Span,
}

#[derive(Clone, Debug)]
pub struct Func {
    /// Source name (used in stack traces).
    pub name: String,
    /// Name with type arguments, e.g. `map<int, str>`.
    pub inst_name: String,
    pub params: usize,
    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<Block>,
    pub ret: Ty,
    /// Scope 0 is the function itself.
    pub scopes: Vec<Scope>,
    /// A `const` initialiser: evaluated at most once, hidden from traces.
    pub is_const: bool,
    /// The function is in SSA form: every local has exactly one definition (a
    /// parameter, a statement, a call or a phi) that dominates its uses, `Block::phis`
    /// may be non-empty and `SetField` is replaced by `Rvalue::WithField`.
    /// Only the passes in `opt::ssa` and friends may run on SSA functions.
    pub ssa: bool,
}

/// Struct and enum layouts needed to type-check aggregates after lowering.
#[derive(Clone, Debug, Default)]
pub struct TypeEnv {
    pub structs: HashMap<DefId, StructInfo>,
    pub enums: HashMap<DefId, EnumInfo>,
}

impl TypeEnv {
    /// Field types of a struct (`variant == None`) or enum variant with `args` substituted.
    pub fn field_types(&self, def: DefId, variant: Option<u32>, args: &[Ty]) -> Option<Vec<Ty>> {
        let (generics, fields) = match variant {
            Some(v) => {
                let e = self.enums.get(&def)?;
                (&e.generics, &e.variants.get(v as usize)?.fields)
            }
            None => {
                let s = self.structs.get(&def)?;
                (&s.generics, &s.fields)
            }
        };
        let map: Vec<_> = generics.iter().cloned().zip(args.iter().cloned()).collect();
        Some(fields.iter().map(|f| f.ty.subst(&map)).collect())
    }
    pub fn variant_count(&self, def: DefId) -> usize {
        self.enums.get(&def).map_or(0, |e| e.variants.len())
    }
}

#[derive(Clone, Debug, Default)]
pub struct MonoStats {
    /// (function name, number of distinct instances), generic functions only.
    pub generic_instances: Vec<(String, usize)>,
    pub total_instances: usize,
}

#[derive(Clone, Debug)]
pub struct MirProgram {
    pub funcs: Vec<Func>,
    pub main: FuncId,
    pub types: TypeEnv,
    pub stats: MonoStats,
}

impl Rvalue {
    pub fn for_each_operand(&self, f: &mut dyn FnMut(&Operand)) {
        match self {
            Rvalue::Use(a)
            | Rvalue::Unary(_, a)
            | Rvalue::Field(a, _)
            | Rvalue::Len(a)
            | Rvalue::Tag(a)
            | Rvalue::Cast(a) => f(a),
            Rvalue::Binary(_, a, b) | Rvalue::Index(a, b, _) | Rvalue::WithField(a, _, b) => {
                f(a);
                f(b)
            }
            Rvalue::Agg(_, xs) | Rvalue::Builtin(_, xs) => xs.iter().for_each(f),
        }
    }
    pub fn for_each_operand_mut(&mut self, f: &mut dyn FnMut(&mut Operand)) {
        match self {
            Rvalue::Use(a)
            | Rvalue::Unary(_, a)
            | Rvalue::Field(a, _)
            | Rvalue::Len(a)
            | Rvalue::Tag(a)
            | Rvalue::Cast(a) => f(a),
            Rvalue::Binary(_, a, b) | Rvalue::Index(a, b, _) | Rvalue::WithField(a, _, b) => {
                f(a);
                f(b)
            }
            Rvalue::Agg(_, xs) | Rvalue::Builtin(_, xs) => xs.iter_mut().for_each(f),
        }
    }
    /// Can evaluation fail at run time or change observable state?
    pub fn may_trap_or_effect(&self) -> bool {
        match self {
            // Conservative: string `+` never traps, but that needs operand types
            // (see `Func::rvalue_is_pure`).
            Rvalue::Binary(op, ..) => matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
            ),
            Rvalue::Unary(UnOp::Neg, _) => true,
            Rvalue::Index(_, _, checked) => *checked,
            Rvalue::Builtin(b, _) => !matches!(b, Builtin::ToStr | Builtin::Chars),
            _ => false,
        }
    }
    /// Does the value depend on mutable heap state (array contents/length)?
    pub fn reads_memory(&self) -> bool {
        matches!(self, Rvalue::Index(..) | Rvalue::Len(_))
    }
}

impl Stmt {
    pub fn new(kind: StmtKind, scope: u32) -> Stmt {
        Stmt {
            kind,
            scope,
            span: Span::default(),
        }
    }
    pub fn at(kind: StmtKind, scope: u32, span: Span) -> Stmt {
        Stmt { kind, scope, span }
    }
    pub fn def(&self) -> Option<LocalId> {
        match &self.kind {
            StmtKind::Assign(d, _) | StmtKind::SetField(d, _, _) => Some(*d),
            StmtKind::SetIndex(..) => None,
        }
    }
    pub fn for_each_use(&self, f: &mut dyn FnMut(&Operand)) {
        match &self.kind {
            StmtKind::Assign(_, rv) => rv.for_each_operand(f),
            StmtKind::SetField(d, _, v) => {
                f(&Operand::Local(*d));
                f(v)
            }
            StmtKind::SetIndex(a, i, v, _) => {
                f(a);
                f(i);
                f(v)
            }
        }
    }
    /// Operands that may be *replaced* (the aggregate being updated by `SetField` is not one).
    pub fn for_each_use_mut(&mut self, f: &mut dyn FnMut(&mut Operand)) {
        match &mut self.kind {
            StmtKind::Assign(_, rv) => rv.for_each_operand_mut(f),
            StmtKind::SetField(_, _, v) => f(v),
            StmtKind::SetIndex(a, i, v, _) => {
                f(a);
                f(i);
                f(v)
            }
        }
    }
    /// Does this statement mutate array contents or lengths?
    pub fn writes_memory(&self) -> bool {
        match &self.kind {
            StmtKind::SetIndex(..) => true,
            StmtKind::Assign(_, Rvalue::Builtin(b, _)) => matches!(b, Builtin::Push | Builtin::Pop),
            _ => false,
        }
    }
}

impl Terminator {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Goto(t) => vec![*t],
            Terminator::Branch {
                then_bb, else_bb, ..
            } => vec![*then_bb, *else_bb],
            Terminator::Switch { cases, default, .. } => {
                cases.iter().map(|c| c.1).chain([*default]).collect()
            }
            Terminator::Call { target, .. } => vec![*target],
            Terminator::Return(_) | Terminator::Unreachable => vec![],
        }
    }
    pub fn for_each_succ_mut(&mut self, f: &mut dyn FnMut(&mut BlockId)) {
        match self {
            Terminator::Goto(t) => f(t),
            Terminator::Branch {
                then_bb, else_bb, ..
            } => {
                f(then_bb);
                f(else_bb)
            }
            Terminator::Switch { cases, default, .. } => {
                cases.iter_mut().for_each(|c| f(&mut c.1));
                f(default)
            }
            Terminator::Call { target, .. } => f(target),
            Terminator::Return(_) | Terminator::Unreachable => {}
        }
    }
    pub fn for_each_use(&self, f: &mut dyn FnMut(&Operand)) {
        match self {
            Terminator::Branch { cond, .. } => f(cond),
            Terminator::Switch { value, .. } => f(value),
            Terminator::Call { callee, args, .. } => {
                if let Callee::Indirect(o) = callee {
                    f(o)
                }
                args.iter().for_each(f)
            }
            Terminator::Return(o) => f(o),
            Terminator::Goto(_) | Terminator::Unreachable => {}
        }
    }
    pub fn for_each_use_mut(&mut self, f: &mut dyn FnMut(&mut Operand)) {
        match self {
            Terminator::Branch { cond, .. } => f(cond),
            Terminator::Switch { value, .. } => f(value),
            Terminator::Call { callee, args, .. } => {
                if let Callee::Indirect(o) = callee {
                    f(o)
                }
                args.iter_mut().for_each(f)
            }
            Terminator::Return(o) => f(o),
            Terminator::Goto(_) | Terminator::Unreachable => {}
        }
    }
    pub fn def(&self) -> Option<LocalId> {
        match self {
            Terminator::Call { dest, .. } => Some(*dest),
            _ => None,
        }
    }
}

impl Func {
    pub fn local_ty(&self, l: LocalId) -> &Ty {
        &self.locals[l as usize].ty
    }
    pub fn new_local(&mut self, ty: Ty) -> LocalId {
        self.locals.push(LocalDecl { ty, name: None });
        (self.locals.len() - 1) as LocalId
    }
    /// Blocks reachable from the entry, in depth-first preorder.
    pub fn reachable(&self) -> Vec<BlockId> {
        let mut seen = vec![false; self.blocks.len()];
        let mut order = Vec::new();
        let mut stack = vec![0 as BlockId];
        while let Some(b) = stack.pop() {
            if (b as usize) >= seen.len() || seen[b as usize] {
                continue;
            }
            seen[b as usize] = true;
            order.push(b);
            for s in self.blocks[b as usize].term.successors().into_iter().rev() {
                stack.push(s);
            }
        }
        order
    }
    /// Reverse postorder of the reachable blocks (good iteration order for forward dataflow).
    pub fn rpo(&self) -> Vec<BlockId> {
        let n = self.blocks.len();
        let mut seen = vec![false; n];
        let mut post = Vec::new();
        let mut stack: Vec<(BlockId, usize)> = vec![(0, 0)];
        seen[0] = true;
        while let Some(&mut (b, ref mut i)) = stack.last_mut() {
            let succs = self.blocks[b as usize].term.successors();
            if *i < succs.len() {
                let s = succs[*i];
                *i += 1;
                if (s as usize) < n && !seen[s as usize] {
                    seen[s as usize] = true;
                    stack.push((s, 0));
                }
            } else {
                post.push(b);
                stack.pop();
            }
        }
        post.reverse();
        post
    }
    pub fn predecessors(&self) -> Vec<Vec<BlockId>> {
        let mut preds = vec![Vec::new(); self.blocks.len()];
        for b in self.reachable() {
            for s in self.blocks[b as usize].term.successors() {
                if !preds[s as usize].contains(&b) {
                    preds[s as usize].push(b);
                }
            }
        }
        preds
    }
    /// Number of statements plus terminators in reachable blocks (inlining cost).
    pub fn size(&self) -> usize {
        self.reachable()
            .iter()
            .map(|&b| self.blocks[b as usize].stmts.len() + 1)
            .sum()
    }
    /// Is the rvalue free of traps and effects, given operand types?
    pub fn rvalue_is_pure(&self, rv: &Rvalue) -> bool {
        match rv {
            Rvalue::Binary(BinOp::Add, a, _) if self.operand_is_str(a) => true,
            _ => !rv.may_trap_or_effect(),
        }
    }
    pub fn operand_is_str(&self, o: &Operand) -> bool {
        match o {
            Operand::Const(Const::Str(_)) => true,
            Operand::Const(_) => false,
            Operand::Local(l) => *self.local_ty(*l) == Ty::Str,
        }
    }
}

/// Call `f` for every local *read* by a block (statements and terminator).
pub fn block_uses(b: &Block, f: &mut dyn FnMut(LocalId)) {
    let mut g = |o: &Operand| {
        if let Operand::Local(l) = o {
            f(*l)
        }
    };
    for s in &b.stmts {
        s.for_each_use(&mut g);
    }
    b.term.for_each_use(&mut g);
}
