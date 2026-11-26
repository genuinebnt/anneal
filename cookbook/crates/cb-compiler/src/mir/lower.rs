//! HIR -> MIR: monomorphisation plus control-flow lowering.
//!
//! Monomorphisation is a memoised worklist: starting from `main`, every use of
//! a generic function with concrete type arguments requests an *instance*
//! `(DefId, [Ty])`; each distinct instance is lowered exactly once with the
//! generic parameters substituted. Polymorphic recursion (`f<T>` calling
//! `f<[T]>`) would never terminate, so instance count and type depth are
//! bounded and exceeding the bound is a [`LowerError`].
//!
//! Lowering invariants: while lowering, `cur == None` means "the current point
//! is unreachable" (after `return`, `break`, `panic`, ...). Emitting then does
//! nothing, so dead code never reaches the MIR.

use super::*;
use crate::sema::hir::*;
use crate::sema::types::Name;
use crate::syntax::ast::Lit;
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct LowerError {
    pub message: String,
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lowering error: {}", self.message)
    }
}

const MAX_INSTANCES: usize = 4000;
const MAX_TYPE_DEPTH: usize = 24;

fn ty_depth(t: &Ty) -> usize {
    let mut d = 0;
    t.map_children(&mut |c| {
        d = d.max(ty_depth(c));
        c.clone()
    });
    d + 1
}

type Key = (DefId, Vec<Ty>);

struct Cx<'p> {
    prog: &'p Program,
    funcs: Vec<Option<Func>>,
    ids: HashMap<Key, FuncId>,
    queue: VecDeque<(FuncId, Key, bool)>,
    err: Option<String>,
}

impl<'p> Cx<'p> {
    fn instance(&mut self, def: DefId, args: Vec<Ty>, is_const: bool) -> FuncId {
        let key = (def, args);
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        if self.funcs.len() >= MAX_INSTANCES || key.1.iter().any(|t| ty_depth(t) > MAX_TYPE_DEPTH) {
            let name = &self.prog.def(def).name;
            self.err.get_or_insert(format!(
                "polymorphic recursion: instantiating `{name}` never reaches a fixed point (more than {MAX_INSTANCES} instances or a type nested deeper than {MAX_TYPE_DEPTH})"
            ));
            return 0;
        }
        let id = self.funcs.len() as FuncId;
        self.funcs.push(None);
        self.ids.insert(key.clone(), id);
        self.queue.push_back((id, key, is_const));
        id
    }
}

/// Lower a checked program to MIR, monomorphising from `main`.
pub fn lower(prog: &Program) -> Result<MirProgram, LowerError> {
    let main = prog.main.ok_or_else(|| LowerError {
        message: "no `main` function".into(),
    })?;
    let mut cx = Cx {
        prog,
        funcs: Vec::new(),
        ids: HashMap::new(),
        queue: VecDeque::new(),
        err: None,
    };
    let main_id = cx.instance(main, vec![], false);
    while let Some((id, key, is_const)) = cx.queue.pop_front() {
        let f = FnLower::lower(&mut cx, &key, is_const);
        cx.funcs[id as usize] = Some(f);
        if let Some(message) = cx.err.take() {
            return Err(LowerError { message });
        }
    }
    let mut per_def: HashMap<DefId, usize> = HashMap::new();
    for (d, _) in cx.ids.keys() {
        if prog.fn_by_def(*d).is_some_and(|f| !f.generics.is_empty()) {
            *per_def.entry(*d).or_default() += 1;
        }
    }
    let mut generic_instances: Vec<(String, usize)> = per_def
        .into_iter()
        .map(|(d, n)| (prog.def(d).name.clone(), n))
        .collect();
    generic_instances.sort();
    let funcs: Vec<Func> = cx
        .funcs
        .into_iter()
        .map(|f| f.expect("every queued instance is lowered"))
        .collect();
    let types = TypeEnv {
        structs: prog.structs.iter().map(|s| (s.def, s.clone())).collect(),
        enums: prog.enums.iter().map(|e| (e.def, e.clone())).collect(),
    };
    let stats = MonoStats {
        generic_instances,
        total_instances: funcs.len(),
    };
    let mut mp = MirProgram {
        funcs,
        main: main_id,
        types,
        stats,
    };
    for f in &mut mp.funcs {
        crate::opt::cfg::remove_unreachable(f);
    }
    Ok(mp)
}

struct LoopCtx {
    brk: BlockId,
    cont: BlockId,
    res: Option<LocalId>,
}

enum Place {
    Local(LocalId),
    Field(Box<Place>, u32, Ty),
    Index(Operand, Operand, Ty, Span),
}

struct FnLower<'a, 'p> {
    cx: &'a mut Cx<'p>,
    map: Vec<(Name, Ty)>,
    f: Func,
    cur: Option<BlockId>,
    locals: HashMap<DefId, LocalId>,
    loops: Vec<LoopCtx>,
    /// Temporaries assigned on several paths (join results); never forwarded.
    multi: HashSet<LocalId>,
    /// Span of the expression being lowered; stamped on every emitted statement.
    span: Span,
}

/// Direct sub-expressions, in evaluation order.
fn children(e: &HirExpr) -> Vec<&HirExpr> {
    use HirExprKind::*;
    match &e.kind {
        Lit(_) | Local(_) | Fn(..) | Const(_) | Continue | Error => vec![],
        Unary(_, a) | Field(a, _) | TupleField(a, _) | Cast(a) | Loop(a) => vec![a],
        Binary(_, a, b) | Index(a, b) | While(a, b) => vec![a, b],
        Assign { target, value, .. } => vec![value, target],
        Call(c, args) => std::iter::once(&**c).chain(args.iter()).collect(),
        Builtin(_, xs) | Tuple(xs) | Array(xs) => xs.iter().collect(),
        Struct { fields, .. } | Variant { fields, .. } => fields.iter().map(|(_, x)| x).collect(),
        If(c, t, el) => [&**c, &**t].into_iter().chain(el.as_deref()).collect(),
        Match(s, arms) => std::iter::once(&**s)
            .chain(
                arms.iter()
                    .flat_map(|a| a.guard.iter().chain(std::iter::once(&a.body))),
            )
            .collect(),
        Block(b) => b
            .stmts
            .iter()
            .map(|s| match s {
                HirStmt::Let { init, .. } => init,
                HirStmt::Expr(x) => x,
            })
            .chain(b.tail.as_deref())
            .collect(),
        Break(x) | Return(x) => x.as_deref().into_iter().collect(),
    }
}

fn has_assign(e: &HirExpr) -> bool {
    matches!(e.kind, HirExprKind::Assign { .. }) || children(e).into_iter().any(has_assign)
}

impl<'a, 'p> FnLower<'a, 'p> {
    fn lower(cx: &'a mut Cx<'p>, key: &Key, is_const: bool) -> Func {
        let prog = cx.prog;
        let (def, args) = (key.0, &key.1);
        let (name, generics, params, ret, body): (&str, &[Name], &[DefId], &Ty, &HirExpr) =
            if is_const {
                let c = prog.const_by_def(def).expect("const instance");
                (&c.name, &[], &[], &c.ty, &c.body)
            } else {
                let f = prog.fn_by_def(def).expect("function instance");
                (&f.name, &f.generics, &f.params, &f.ret, &f.body)
            };
        let map: Vec<(Name, Ty)> = generics.iter().cloned().zip(args.iter().cloned()).collect();
        let inst_name = if args.is_empty() {
            name.to_string()
        } else {
            format!(
                "{name}<{}>",
                args.iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let ret = ret.subst(&map);
        let f = Func {
            name: name.to_string(),
            inst_name,
            params: params.len(),
            locals: Vec::new(),
            blocks: vec![Block::new(vec![], Terminator::Unreachable)],
            ret,
            scopes: vec![Scope {
                name: name.to_string(),
                parent: None,
                call_site: Span::default(),
            }],
            is_const,
            ssa: false,
        };
        let mut l = FnLower {
            cx,
            map,
            f,
            cur: Some(0),
            locals: HashMap::new(),
            loops: Vec::new(),
            multi: HashSet::new(),
            span: Span::default(),
        };
        for p in params {
            l.local_of(*p);
        }
        let v = l.expr(body);
        l.set_term(Terminator::Return(v));
        l.f
    }

    // ------------------------------------------------------------ builders

    fn ty(&self, t: &Ty) -> Ty {
        t.subst(&self.map)
    }
    fn local_of(&mut self, d: DefId) -> LocalId {
        if let Some(&l) = self.locals.get(&d) {
            return l;
        }
        let info = self.cx.prog.def(d);
        let ty = info.ty.as_ref().map(|t| self.ty(t)).unwrap_or(Ty::Unit);
        self.f.locals.push(LocalDecl {
            ty,
            name: Some(info.name.clone()),
        });
        let l = (self.f.locals.len() - 1) as LocalId;
        self.locals.insert(d, l);
        l
    }
    fn new_block(&mut self) -> BlockId {
        self.f
            .blocks
            .push(Block::new(vec![], Terminator::Unreachable));
        (self.f.blocks.len() - 1) as BlockId
    }
    fn temp(&mut self, ty: Ty) -> LocalId {
        self.f.new_local(ty)
    }
    fn emit(&mut self, kind: StmtKind) {
        if let Some(b) = self.cur {
            self.f.blocks[b as usize]
                .stmts
                .push(Stmt::at(kind, 0, self.span));
        }
    }
    fn set_term(&mut self, t: Terminator) {
        if let Some(b) = self.cur.take() {
            self.f.blocks[b as usize].term = t;
        }
    }
    fn goto(&mut self, b: BlockId) {
        self.set_term(Terminator::Goto(b));
    }
    fn rv(&mut self, ty: Ty, rv: Rvalue) -> Operand {
        let t = self.temp(ty);
        self.emit(StmtKind::Assign(t, rv));
        Operand::Local(t)
    }
    /// A join temporary that several paths assign.
    fn join_temp(&mut self, ty: &Ty) -> Option<LocalId> {
        if matches!(ty, Ty::Unit | Ty::Never) {
            return None;
        }
        let t = self.temp(ty.clone());
        self.multi.insert(t);
        Some(t)
    }
    fn is_named(&self, o: &Operand) -> bool {
        matches!(o, Operand::Local(l) if self.f.locals[*l as usize].name.is_some())
    }

    /// `dest = op`, retargeting the defining statement of a fresh temporary
    /// instead of emitting a copy.
    fn assign_to(&mut self, dest: LocalId, op: Operand) {
        if let (Some(b), Operand::Local(t)) = (self.cur, &op)
            && self.f.locals[*t as usize].name.is_none()
            && !self.multi.contains(t)
            && let Some(Stmt {
                kind: StmtKind::Assign(d, _),
                ..
            }) = self.f.blocks[b as usize].stmts.last_mut()
            && d == t
        {
            *d = dest;
            return;
        }
        self.emit(StmtKind::Assign(dest, Rvalue::Use(op)));
    }

    /// Evaluate left to right; a named local that a *later* sibling might
    /// reassign is copied first so the earlier operand keeps its old value.
    fn ordered(&mut self, es: &[&HirExpr]) -> Vec<Operand> {
        let mut out = Vec::with_capacity(es.len());
        for (i, e) in es.iter().enumerate() {
            let mut o = self.expr(e);
            if self.is_named(&o) && es[i + 1..].iter().any(|x| has_assign(x)) {
                let ty = self.ty(&e.ty);
                o = self.rv(ty, Rvalue::Use(o));
            }
            out.push(o);
        }
        out
    }

    // --------------------------------------------------------- expressions

    fn expr(&mut self, e: &HirExpr) -> Operand {
        let saved = std::mem::replace(&mut self.span, e.span);
        let r = self.expr_inner(e);
        self.span = saved;
        r
    }

    fn expr_inner(&mut self, e: &HirExpr) -> Operand {
        if self.cur.is_none() {
            return Operand::UNIT;
        }
        let ty = self.ty(&e.ty);
        match &e.kind {
            HirExprKind::Lit(l) => Operand::Const(match l {
                Lit::Int(i) => Const::Int(*i),
                Lit::Bool(b) => Const::Bool(*b),
                Lit::Str(s) => Const::Str(Rc::from(s.as_str())),
                Lit::Unit => Const::Unit,
            }),
            HirExprKind::Local(d) => Operand::Local(self.local_of(*d)),
            HirExprKind::Fn(d, targs) => {
                let targs = targs.iter().map(|t| self.ty(t)).collect();
                Operand::Const(Const::Fn(self.cx.instance(*d, targs, false)))
            }
            HirExprKind::Const(d) => {
                let f = self.cx.instance(*d, vec![], true);
                self.call(Callee::Direct(f), vec![], ty)
            }
            HirExprKind::Unary(op, a) => {
                let a = self.expr(a);
                self.rv(ty, Rvalue::Unary(*op, a))
            }
            HirExprKind::Binary(op @ (BinOp::And | BinOp::Or), a, b) => {
                self.short_circuit(*op, a, b)
            }
            HirExprKind::Binary(op, a, b) => {
                let v = self.ordered(&[a, b]);
                let [x, y]: [Operand; 2] = v.try_into().expect("two operands");
                self.rv(ty, Rvalue::Binary(*op, x, y))
            }
            HirExprKind::Assign { op, target, value } => {
                self.assign(*op, target, value);
                Operand::UNIT
            }
            HirExprKind::Call(callee, args) => {
                let (callee_op, args) = match &callee.kind {
                    HirExprKind::Fn(d, targs) => {
                        let targs = targs.iter().map(|t| self.ty(t)).collect();
                        (
                            Callee::Direct(self.cx.instance(*d, targs, false)),
                            self.ordered(&args.iter().collect::<Vec<_>>()),
                        )
                    }
                    _ => {
                        let all: Vec<&HirExpr> =
                            std::iter::once(&**callee).chain(args.iter()).collect();
                        let mut v = self.ordered(&all);
                        let c = v.remove(0);
                        (Callee::Indirect(c), v)
                    }
                };
                self.call(callee_op, args, ty)
            }
            HirExprKind::Builtin(b, args) => self.builtin(*b, args, ty),
            HirExprKind::Field(b, i) | HirExprKind::TupleField(b, i) => {
                let b = self.expr(b);
                self.rv(ty, Rvalue::Field(b, *i as u32))
            }
            HirExprKind::Index(b, i) => {
                let v = self.ordered(&[b, i]);
                let [x, y]: [Operand; 2] = v.try_into().expect("two operands");
                self.rv(ty, Rvalue::Index(x, y, true))
            }
            HirExprKind::Struct { def, fields } => {
                let n = self.cx.prog.struct_info(*def).map_or(0, |s| s.fields.len());
                let ops = self.fields_in_order(n, fields);
                self.rv(ty, Rvalue::Agg(AggKind::Struct(*def), ops))
            }
            HirExprKind::Variant {
                enum_def,
                variant,
                fields,
            } => {
                let n = self
                    .cx
                    .prog
                    .enum_info(*enum_def)
                    .map_or(0, |s| s.variants[*variant].fields.len());
                let ops = self.fields_in_order(n, fields);
                self.rv(
                    ty,
                    Rvalue::Agg(AggKind::Variant(*enum_def, *variant as u32), ops),
                )
            }
            HirExprKind::Tuple(xs) => {
                let ops = self.ordered(&xs.iter().collect::<Vec<_>>());
                self.rv(ty, Rvalue::Agg(AggKind::Tuple, ops))
            }
            HirExprKind::Array(xs) => {
                let ops = self.ordered(&xs.iter().collect::<Vec<_>>());
                self.rv(ty, Rvalue::Agg(AggKind::Array, ops))
            }
            HirExprKind::If(c, t, el) => self.if_(&ty, c, t, el.as_deref()),
            HirExprKind::Match(s, arms) => self.match_(&ty, s, arms),
            HirExprKind::Block(b) => self.block(b),
            HirExprKind::While(c, body) => {
                let (head, bodyb, exit) = (self.new_block(), self.new_block(), self.new_block());
                self.goto(head);
                self.cur = Some(head);
                let cond = self.expr(c);
                self.set_term(Terminator::Branch {
                    cond,
                    then_bb: bodyb,
                    else_bb: exit,
                });
                self.cur = Some(bodyb);
                self.loops.push(LoopCtx {
                    brk: exit,
                    cont: head,
                    res: None,
                });
                self.expr(body);
                self.loops.pop();
                self.goto(head);
                self.cur = Some(exit);
                Operand::UNIT
            }
            HirExprKind::Loop(body) => {
                let (head, exit) = (self.new_block(), self.new_block());
                let res = self.join_temp(&ty);
                self.goto(head);
                self.cur = Some(head);
                self.loops.push(LoopCtx {
                    brk: exit,
                    cont: head,
                    res,
                });
                self.expr(body);
                self.loops.pop();
                self.goto(head);
                self.cur = Some(exit);
                res.map_or(Operand::UNIT, Operand::Local)
            }
            HirExprKind::Break(v) => {
                let val = v.as_ref().map(|x| self.expr(x));
                if let Some(ctx) = self.loops.last() {
                    let (brk, res) = (ctx.brk, ctx.res);
                    if let (Some(r), Some(val)) = (res, val) {
                        self.emit(StmtKind::Assign(r, Rvalue::Use(val)));
                    }
                    self.goto(brk);
                }
                Operand::UNIT
            }
            HirExprKind::Continue => {
                if let Some(ctx) = self.loops.last() {
                    let c = ctx.cont;
                    self.goto(c);
                }
                Operand::UNIT
            }
            HirExprKind::Return(v) => {
                let val = v.as_ref().map_or(Operand::UNIT, |x| self.expr(x));
                self.set_term(Terminator::Return(val));
                Operand::UNIT
            }
            HirExprKind::Cast(a) => {
                let a = self.expr(a);
                self.rv(ty, Rvalue::Cast(a))
            }
            HirExprKind::Error => {
                self.cx
                    .err
                    .get_or_insert("program contains error nodes".into());
                Operand::UNIT
            }
        }
    }

    fn call(&mut self, callee: Callee, args: Vec<Operand>, ty: Ty) -> Operand {
        let Some(b) = self.cur else {
            return Operand::UNIT;
        };
        let diverges = ty == Ty::Never;
        let dest = self.temp(ty);
        let target = self.new_block();
        self.f.blocks[b as usize].term = Terminator::Call {
            callee,
            args,
            dest,
            target,
            scope: 0,
            span: self.span,
        };
        self.cur = if diverges { None } else { Some(target) };
        Operand::Local(dest)
    }

    fn fields_in_order(&mut self, n: usize, fields: &[(usize, HirExpr)]) -> Vec<Operand> {
        let exprs: Vec<&HirExpr> = fields.iter().map(|(_, e)| e).collect();
        let vals = self.ordered(&exprs);
        let mut out = vec![Operand::UNIT; n];
        for ((i, _), v) in fields.iter().zip(vals) {
            if *i < n {
                out[*i] = v;
            }
        }
        out
    }

    fn short_circuit(&mut self, op: BinOp, a: &HirExpr, b: &HirExpr) -> Operand {
        let res = self.temp(Ty::Bool);
        self.multi.insert(res);
        let av = self.expr(a);
        self.emit(StmtKind::Assign(res, Rvalue::Use(av)));
        let (rhs, join) = (self.new_block(), self.new_block());
        let (t, e) = if op == BinOp::And {
            (rhs, join)
        } else {
            (join, rhs)
        };
        self.set_term(Terminator::Branch {
            cond: Operand::Local(res),
            then_bb: t,
            else_bb: e,
        });
        self.cur = Some(rhs);
        let bv = self.expr(b);
        self.emit(StmtKind::Assign(res, Rvalue::Use(bv)));
        self.goto(join);
        self.cur = Some(join);
        Operand::Local(res)
    }

    fn if_(&mut self, ty: &Ty, c: &HirExpr, t: &HirExpr, el: Option<&HirExpr>) -> Operand {
        let cond = self.expr(c);
        let (tb, eb, join) = (self.new_block(), self.new_block(), self.new_block());
        self.set_term(Terminator::Branch {
            cond,
            then_bb: tb,
            else_bb: eb,
        });
        let res = self.join_temp(ty);
        for (blk, body) in [(tb, Some(t)), (eb, el)] {
            self.cur = Some(blk);
            if let Some(body) = body {
                let v = self.expr(body);
                if let Some(r) = res {
                    self.emit(StmtKind::Assign(r, Rvalue::Use(v)));
                }
            }
            self.goto(join);
        }
        self.cur = Some(join);
        res.map_or(Operand::UNIT, Operand::Local)
    }

    fn block(&mut self, b: &HirBlock) -> Operand {
        for s in &b.stmts {
            match s {
                HirStmt::Let { pat, init } => {
                    let v = self.expr(init);
                    if self.cur.is_none() {
                        return Operand::UNIT;
                    }
                    match &pat.kind {
                        HirPatKind::Bind(d, _) => {
                            let l = self.local_of(*d);
                            self.assign_to(l, v);
                        }
                        _ => {
                            let v = self.own_temp(v, &pat.ty);
                            let fail = self.new_block();
                            self.pat(pat, v, fail);
                        }
                    }
                }
                HirStmt::Expr(x) => {
                    self.expr(x);
                }
            }
        }
        match &b.tail {
            Some(t) => self.expr(t),
            None => Operand::UNIT,
        }
    }

    /// Copy a named local into a temporary so later reassignment cannot change it.
    fn own_temp(&mut self, o: Operand, ty: &Ty) -> Operand {
        if self.is_named(&o) {
            let ty = self.ty(ty);
            self.rv(ty, Rvalue::Use(o))
        } else {
            o
        }
    }

    // ------------------------------------------------------------ built-ins

    fn builtin(&mut self, b: Builtin, args: &[HirExpr], ty: Ty) -> Operand {
        let ops = self.ordered(&args.iter().collect::<Vec<_>>());
        match b {
            Builtin::Len => self.rv(ty, Rvalue::Len(ops[0].clone())),
            Builtin::Panic => {
                self.rv(Ty::Never, Rvalue::Builtin(b, ops));
                self.set_term(Terminator::Unreachable);
                Operand::UNIT
            }
            _ => self.rv(ty, Rvalue::Builtin(b, ops)),
        }
    }

    // ---------------------------------------------------------- assignment

    fn assign(&mut self, op: Option<BinOp>, target: &HirExpr, value: &HirExpr) {
        let rhs = self.expr(value);
        if self.cur.is_none() {
            return;
        }
        if let HirExprKind::Local(d) = &target.kind {
            let l = self.local_of(*d);
            match op {
                None => self.assign_to(l, rhs),
                Some(o) => self.emit(StmtKind::Assign(
                    l,
                    Rvalue::Binary(o, Operand::Local(l), rhs),
                )),
            }
            return;
        }
        let place = self.place(target);
        let new = match op {
            None => rhs,
            Some(o) => {
                let cur = self.read_place(&place);
                let ty = self.ty(&target.ty);
                self.rv(ty, Rvalue::Binary(o, cur, rhs))
            }
        };
        self.write_place(&place, new);
    }

    fn place(&mut self, t: &HirExpr) -> Place {
        match &t.kind {
            HirExprKind::Local(d) => Place::Local(self.local_of(*d)),
            HirExprKind::Field(b, i) | HirExprKind::TupleField(b, i) => {
                let inner = self.place(b);
                Place::Field(Box::new(inner), *i as u32, self.ty(&t.ty))
            }
            HirExprKind::Index(b, i) => {
                let v = self.ordered(&[b, i]);
                let [a, ix]: [Operand; 2] = v.try_into().expect("two operands");
                Place::Index(a, ix, self.ty(&t.ty), t.span)
            }
            _ => {
                self.cx
                    .err
                    .get_or_insert("assignment to a non-place".into());
                Place::Local(0)
            }
        }
    }

    fn read_place(&mut self, p: &Place) -> Operand {
        match p {
            Place::Local(l) => Operand::Local(*l),
            Place::Field(inner, i, ty) => {
                let b = self.read_place(inner);
                self.rv(ty.clone(), Rvalue::Field(b, *i))
            }
            Place::Index(a, i, ty, sp) => {
                let saved = std::mem::replace(&mut self.span, *sp);
                let r = self.rv(ty.clone(), Rvalue::Index(a.clone(), i.clone(), true));
                self.span = saved;
                r
            }
        }
    }

    /// Store into a place. Aggregates have value semantics, so a nested field
    /// store reads the parent, updates the copy (copy-on-write) and writes it back.
    fn write_place(&mut self, p: &Place, v: Operand) {
        match p {
            Place::Local(l) => self.assign_to(*l, v),
            Place::Index(a, i, _, sp) => {
                let saved = std::mem::replace(&mut self.span, *sp);
                self.emit(StmtKind::SetIndex(a.clone(), i.clone(), v, true));
                self.span = saved;
            }
            Place::Field(inner, i, _) => {
                let base = self.read_place(inner);
                if let Operand::Local(l) = base {
                    self.emit(StmtKind::SetField(l, *i, v));
                    if !matches!(**inner, Place::Local(_)) {
                        self.write_place(inner, base);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------- matching

    /// Test pattern `p` against `v`: on success control continues in the
    /// current block with bindings assigned, on failure it jumps to `fail`.
    fn pat(&mut self, p: &HirPat, v: Operand, fail: BlockId) {
        if self.cur.is_none() {
            return;
        }
        match &p.kind {
            HirPatKind::Wild | HirPatKind::Error => {}
            HirPatKind::Bind(d, _) => {
                let l = self.local_of(*d);
                self.emit(StmtKind::Assign(l, Rvalue::Use(v)));
            }
            HirPatKind::Lit(l) => {
                let c = Operand::Const(match l {
                    Lit::Int(i) => Const::Int(*i),
                    Lit::Bool(b) => Const::Bool(*b),
                    Lit::Str(s) => Const::Str(Rc::from(s.as_str())),
                    Lit::Unit => Const::Unit,
                });
                if matches!(l, Lit::Unit) {
                    return;
                }
                let eq = self.rv(Ty::Bool, Rvalue::Binary(BinOp::Eq, v, c));
                self.branch_to(eq, fail);
            }
            HirPatKind::Tuple(ps) | HirPatKind::Struct(_, ps) => self.pat_fields(ps, &v, fail),
            HirPatKind::Variant {
                enum_def,
                variant,
                fields,
            } => {
                if self
                    .cx
                    .prog
                    .enum_info(*enum_def)
                    .is_some_and(|e| e.variants.len() > 1)
                {
                    let tag = self.rv(Ty::Int, Rvalue::Tag(v.clone()));
                    let eq = self.rv(
                        Ty::Bool,
                        Rvalue::Binary(BinOp::Eq, tag, Operand::int(*variant as i64)),
                    );
                    self.branch_to(eq, fail);
                }
                self.pat_fields(fields, &v, fail);
            }
            HirPatKind::Or(alts) => {
                let ok = self.new_block();
                for (i, a) in alts.iter().enumerate() {
                    if i + 1 == alts.len() {
                        self.pat(a, v.clone(), fail);
                    } else {
                        let next = self.new_block();
                        self.pat(a, v.clone(), next);
                        self.goto(ok);
                        self.cur = Some(next);
                    }
                }
                self.goto(ok);
                self.cur = Some(ok);
            }
        }
    }

    fn pat_fields(&mut self, ps: &[HirPat], v: &Operand, fail: BlockId) {
        for (i, q) in ps.iter().enumerate() {
            if matches!(q.kind, HirPatKind::Wild) {
                continue;
            }
            let ty = self.ty(&q.ty);
            let f = self.rv(ty, Rvalue::Field(v.clone(), i as u32));
            self.pat(q, f, fail);
        }
    }

    /// Continue when `cond` is true, jump to `fail` otherwise.
    fn branch_to(&mut self, cond: Operand, fail: BlockId) {
        let ok = self.new_block();
        self.set_term(Terminator::Branch {
            cond,
            then_bb: ok,
            else_bb: fail,
        });
        self.cur = Some(ok);
    }

    /// Arms are tried in order; for enum scrutinees a `Switch` on the tag
    /// jumps straight to the first arm that can match each variant.
    fn match_(&mut self, ty: &Ty, s: &HirExpr, arms: &[HirArm]) -> Operand {
        let sv = self.expr(s);
        let sv = self.own_temp(sv, &s.ty);
        if self.cur.is_none() {
            return Operand::UNIT;
        }
        let res = self.join_temp(ty);
        let join = self.new_block();
        let n = arms.len();
        let entries: Vec<BlockId> = (0..=n).map(|_| self.new_block()).collect(); // entries[n]: no arm matched
        let multi_variant = |p: &HirPat, cx: &Cx| match &p.kind {
            HirPatKind::Variant { enum_def, .. } => cx
                .prog
                .enum_info(*enum_def)
                .is_some_and(|e| e.variants.len() > 1),
            _ => false,
        };
        let tag_ok: Vec<Option<BlockId>> = arms
            .iter()
            .map(|a| {
                if multi_variant(&a.pat, self.cx) {
                    Some(self.new_block())
                } else {
                    None
                }
            })
            .collect();
        // Dispatch.
        if let Ty::Adt(def, _, _) = self.ty(&s.ty)
            && let Some(en) = self.cx.prog.enum_info(def)
            && n >= 2
            && en.variants.len() >= 2
        {
            let tag = self.rv(Ty::Int, Rvalue::Tag(sv.clone()));
            let cases = (0..en.variants.len())
                .map(|v| {
                    let target = arms
                        .iter()
                        .enumerate()
                        .find_map(|(i, a)| match &a.pat.kind {
                            HirPatKind::Variant { variant, .. } if *variant == v => {
                                Some(tag_ok[i].unwrap_or(entries[i]))
                            }
                            HirPatKind::Variant { .. } => None,
                            _ => Some(entries[i]),
                        })
                        .unwrap_or(entries[n]);
                    (v as i64, target)
                })
                .collect();
            self.set_term(Terminator::Switch {
                value: tag,
                cases,
                default: entries[n],
            });
        } else {
            self.goto(entries[0]);
        }
        for (i, arm) in arms.iter().enumerate() {
            self.cur = Some(entries[i]);
            let fail = entries[i + 1];
            match (&arm.pat.kind, tag_ok[i]) {
                (
                    HirPatKind::Variant {
                        variant, fields, ..
                    },
                    Some(ok),
                ) => {
                    let tag = self.rv(Ty::Int, Rvalue::Tag(sv.clone()));
                    let eq = self.rv(
                        Ty::Bool,
                        Rvalue::Binary(BinOp::Eq, tag, Operand::int(*variant as i64)),
                    );
                    self.set_term(Terminator::Branch {
                        cond: eq,
                        then_bb: ok,
                        else_bb: fail,
                    });
                    self.cur = Some(ok);
                    self.pat_fields(fields, &sv, fail);
                }
                _ => self.pat(&arm.pat, sv.clone(), fail),
            }
            if let Some(g) = &arm.guard {
                let c = self.expr(g);
                self.branch_to(c, fail);
            }
            let v = self.expr(&arm.body);
            if let Some(r) = res {
                self.emit(StmtKind::Assign(r, Rvalue::Use(v)));
            }
            self.goto(join);
        }
        self.cur = Some(entries[n]);
        self.set_term(Terminator::Unreachable);
        self.cur = Some(join);
        res.map_or(Operand::UNIT, Operand::Local)
    }
}
