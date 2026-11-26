//! The MIR verifier: structural well-formedness, operand/result types and
//! definite initialisation (no local is read on a path where it may be unset).
//! It runs after lowering and, in debug builds and tests, after every pass.
//!
//! Functions in SSA form (`Func::ssa`) are checked differently: instead of
//! definite initialisation, every local must have exactly one definition (a
//! parameter, statement, call or phi), every use must be dominated by it, and
//! every phi must have exactly one argument per predecessor.

use super::*;

pub type VerifyResult = Result<(), Vec<String>>;

/// Verify every function; errors are formatted `function: block: message`.
pub fn verify(p: &MirProgram) -> VerifyResult {
    let mut errs = Vec::new();
    if p.main as usize >= p.funcs.len() {
        errs.push("main function id out of range".to_string());
    }
    for (fi, f) in p.funcs.iter().enumerate() {
        let mut v = V {
            p,
            f,
            fi,
            errs: Vec::new(),
        };
        v.run();
        errs.extend(v.errs);
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

struct V<'a> {
    p: &'a MirProgram,
    f: &'a Func,
    fi: usize,
    errs: Vec<String>,
}

fn compat(a: &Ty, b: &Ty) -> bool {
    a == b || *a == Ty::Never || *b == Ty::Never
}

impl V<'_> {
    fn err(&mut self, bb: usize, msg: impl std::fmt::Display) {
        self.errs.push(format!(
            "{} (fn#{}): bb{bb}: {msg}",
            self.f.inst_name, self.fi
        ));
    }

    fn run(&mut self) {
        let f = self.f;
        if f.blocks.is_empty() {
            self.err(0, "function has no blocks");
            return;
        }
        if f.params > f.locals.len() {
            self.err(0, "more parameters than locals");
            return;
        }
        for (i, l) in f.locals.iter().enumerate() {
            if l.ty
                .contains(&|t| matches!(t, Ty::Param(_) | Ty::Var(_) | Ty::Error))
            {
                self.err(
                    0,
                    format!("local _{i} has a non-monomorphic type `{}`", l.ty),
                );
            }
        }
        for (i, s) in f.scopes.iter().enumerate() {
            if s.parent.is_some_and(|p| p as usize >= i) {
                self.err(0, format!("scope {i} has an invalid parent"));
            }
        }
        let reach = f.reachable();
        let n = f.blocks.len();
        for &b in &reach {
            let blk = &f.blocks[b as usize];
            for t in blk.term.successors() {
                if t as usize >= n {
                    self.err(b as usize, format!("jump to missing block bb{t}"));
                }
            }
            for s in &blk.stmts {
                if s.scope as usize >= f.scopes.len() {
                    self.err(b as usize, "statement scope out of range");
                }
            }
        }
        if !self.errs.is_empty() {
            return;
        }
        for &b in &reach {
            self.check_block(b as usize);
        }
        if self.errs.is_empty() {
            if f.ssa {
                self.check_ssa();
            } else {
                self.check_init(&reach);
            }
        }
    }

    /// SSA-only rules: single definition, dominance of uses, phi arity.
    fn check_ssa(&mut self) {
        use crate::opt::dom::DomTree;
        let f = self.f;
        let dt = DomTree::compute(f);
        if !dt.preds[0].is_empty() {
            self.err(
                0,
                "the entry block of an SSA function must have no predecessors",
            );
        }
        // Position of a definition inside its block: phis and parameters come
        // before statement 0, a call defines its destination after the last statement.
        let mut def: Vec<Option<(BlockId, i64)>> = vec![None; f.locals.len()];
        let define = |s: &mut Self,
                      l: LocalId,
                      at: (BlockId, i64),
                      bb: usize,
                      def: &mut Vec<Option<(BlockId, i64)>>| {
            if def[l as usize].is_some() {
                s.err(
                    bb,
                    format!("local _{l} is defined more than once (not SSA)"),
                );
            }
            def[l as usize] = Some(at);
        };
        for p in 0..f.params {
            define(self, p as LocalId, (0, -2), 0, &mut def);
        }
        for &b in &dt.rpo {
            let blk = &f.blocks[b as usize];
            for ph in &blk.phis {
                define(self, ph.dest, (b, -1), b as usize, &mut def);
            }
            for (i, s) in blk.stmts.iter().enumerate() {
                if let Some(d) = s.def() {
                    define(self, d, (b, i as i64), b as usize, &mut def);
                }
            }
            if let Some(d) = blk.term.def() {
                define(
                    self,
                    d,
                    (b, blk.stmts.len() as i64 + 1),
                    b as usize,
                    &mut def,
                );
            }
        }
        // Does the definition of `l` reach position `(b, pos)`?
        let reaches =
            |l: LocalId, b: BlockId, pos: i64, def: &Vec<Option<(BlockId, i64)>>| match def
                [l as usize]
            {
                None => false,
                Some((db, dpos)) => {
                    if db == b {
                        dpos < pos
                    } else {
                        dt.strictly_dominates(db, b)
                    }
                }
            };
        for &b in &dt.rpo {
            let blk = &f.blocks[b as usize];
            let preds = &dt.preds[b as usize];
            for ph in &blk.phis {
                let mut seen: Vec<BlockId> = Vec::new();
                for (p, o) in &ph.args {
                    if !preds.contains(p) {
                        self.err(
                            b as usize,
                            format!(
                                "phi _{} has an argument for bb{p}, which is not a predecessor",
                                ph.dest
                            ),
                        );
                    }
                    if seen.contains(p) {
                        self.err(
                            b as usize,
                            format!("phi _{} has two arguments for bb{p}", ph.dest),
                        );
                    }
                    seen.push(*p);
                    if let Operand::Local(l) = o {
                        // The value must be available at the end of the predecessor.
                        if !reaches(*l, *p, i64::MAX, &def) {
                            self.err(
                                b as usize,
                                format!(
                                    "phi _{}: argument _{l} from bb{p} is not defined on that path",
                                    ph.dest
                                ),
                            );
                        }
                    }
                }
                if let Some(p) = preds.iter().find(|p| !seen.contains(p)) {
                    self.err(
                        b as usize,
                        format!("phi _{} has no argument for predecessor bb{p}", ph.dest),
                    );
                }
            }
            for (i, s) in blk.stmts.iter().enumerate() {
                s.for_each_use(&mut |o| {
                    if let Operand::Local(l) = o
                        && !reaches(*l, b, i as i64, &def)
                    {
                        self.errs.push(format!(
                            "{} (fn#{}): bb{b}: use of _{l} is not dominated by its definition",
                            f.inst_name, self.fi
                        ));
                    }
                });
            }
            let at = blk.stmts.len() as i64;
            blk.term.for_each_use(&mut |o| {
                if let Operand::Local(l) = o
                    && !reaches(*l, b, at, &def)
                {
                    self.errs.push(format!("{} (fn#{}): bb{b}: use of _{l} in the terminator is not dominated by its definition", f.inst_name, self.fi));
                }
            });
        }
    }

    fn local_ok(&mut self, bb: usize, l: LocalId) -> bool {
        let ok = (l as usize) < self.f.locals.len();
        if !ok {
            self.err(bb, format!("local _{l} out of range"));
        }
        ok
    }

    fn op_ty(&mut self, bb: usize, o: &Operand) -> Option<Ty> {
        match o {
            Operand::Local(l) => self.local_ok(bb, *l).then(|| self.f.local_ty(*l).clone()),
            Operand::Const(Const::Int(_)) => Some(Ty::Int),
            Operand::Const(Const::Bool(_)) => Some(Ty::Bool),
            Operand::Const(Const::Unit) => Some(Ty::Unit),
            Operand::Const(Const::Str(_)) => Some(Ty::Str),
            Operand::Const(Const::Fn(id)) => match self.p.funcs.get(*id as usize) {
                Some(g) => Some(Ty::Fn(
                    (0..g.params).map(|i| g.locals[i].ty.clone()).collect(),
                    Box::new(g.ret.clone()),
                )),
                None => {
                    self.err(bb, format!("function constant fn#{id} out of range"));
                    None
                }
            },
        }
    }

    fn expect(&mut self, bb: usize, what: &str, got: Option<Ty>, want: &Ty) {
        if let Some(g) = got
            && !compat(&g, want)
        {
            self.err(bb, format!("{what}: expected `{want}`, found `{g}`"));
        }
    }

    fn check_block(&mut self, bb: usize) {
        let f = self.f;
        let blk = &f.blocks[bb];
        if !f.ssa && !blk.phis.is_empty() {
            self.err(bb, "phi in a function that is not in SSA form");
        }
        for ph in &blk.phis {
            if self.local_ok(bb, ph.dest) {
                let dty = f.local_ty(ph.dest).clone();
                for (_, a) in &ph.args {
                    let t = self.op_ty(bb, a);
                    self.expect(bb, "phi argument", t, &dty);
                }
            }
        }
        for s in &blk.stmts {
            match &s.kind {
                StmtKind::Assign(d, rv) => {
                    if self.local_ok(bb, *d) {
                        let dty = f.local_ty(*d).clone();
                        self.check_rvalue(bb, &dty, rv);
                    }
                }
                StmtKind::SetField(d, i, v) => {
                    if f.ssa {
                        self.err(bb, "SetField in SSA form (use with_field)");
                    }
                    if !self.local_ok(bb, *d) {
                        continue;
                    }
                    let ty = f.local_ty(*d).clone();
                    let fields = self.fields_of(&ty, None);
                    match fields.and_then(|fs| fs.get(*i as usize).cloned()) {
                        Some(ft) => {
                            let vt = self.op_ty(bb, v);
                            self.expect(bb, "field store", vt, &ft)
                        }
                        None => self.err(bb, format!("SetField _{d}.{i} on `{ty}`")),
                    }
                }
                StmtKind::SetIndex(a, i, v, _) => {
                    let at = self.op_ty(bb, a);
                    let it = self.op_ty(bb, i);
                    self.expect(bb, "index", it, &Ty::Int);
                    match at {
                        Some(Ty::Array(el)) => {
                            let vt = self.op_ty(bb, v);
                            self.expect(bb, "element store", vt, &el)
                        }
                        Some(t) => self.err(bb, format!("indexed store into non-array `{t}`")),
                        None => {}
                    }
                }
            }
        }
        self.check_term(bb);
    }

    /// Field types of an aggregate type; for enums `variant` selects the payload.
    fn fields_of(&self, ty: &Ty, variant: Option<u32>) -> Option<Vec<Ty>> {
        match ty {
            Ty::Tuple(ts) => Some(ts.clone()),
            Ty::Adt(def, _, args) => self.p.types.field_types(*def, variant, args),
            _ => None,
        }
    }

    fn check_rvalue(&mut self, bb: usize, dty: &Ty, rv: &Rvalue) {
        let want = |s: &mut Self, what: &str, got: Ty| {
            if !compat(&got, dty) {
                s.err(
                    bb,
                    format!("{what}: result `{got}` stored into local of type `{dty}`"),
                );
            }
        };
        match rv {
            Rvalue::Use(a) => {
                let t = self.op_ty(bb, a);
                self.expect(bb, "use", t, dty);
            }
            Rvalue::Unary(op, a) => {
                let (t, r) = if *op == UnOp::Neg {
                    (Ty::Int, Ty::Int)
                } else {
                    (Ty::Bool, Ty::Bool)
                };
                let at = self.op_ty(bb, a);
                self.expect(bb, "unary operand", at, &t);
                want(self, "unary", r);
            }
            Rvalue::Binary(op, a, b) => {
                let (at, bt) = (self.op_ty(bb, a), self.op_ty(bb, b));
                let (Some(at), Some(bt)) = (at, bt) else {
                    return;
                };
                if !compat(&at, &bt) {
                    self.err(
                        bb,
                        format!(
                            "binary `{}` operands differ: `{at}` and `{bt}`",
                            op.symbol()
                        ),
                    );
                    return;
                }
                let res = match op {
                    BinOp::Add if at == Ty::Str => Ty::Str,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
                        if at == Ty::Int =>
                    {
                        Ty::Int
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                        if matches!(at, Ty::Int | Ty::Str) =>
                    {
                        Ty::Bool
                    }
                    BinOp::Eq | BinOp::Ne if !at.has_fn() => Ty::Bool,
                    _ => {
                        self.err(
                            bb,
                            format!("operator `{}` not defined on `{at}`", op.symbol()),
                        );
                        return;
                    }
                };
                want(self, "binary", res);
            }
            Rvalue::Agg(kind, ops) => self.check_agg(bb, dty, *kind, ops),
            Rvalue::Field(a, i) => {
                if let Some(t) = self.op_ty(bb, a) {
                    // Enum payload fields: the variant is unknown here, so only check the arity bound.
                    let ok = match &t {
                        Ty::Tuple(ts) => ts.get(*i as usize).is_some_and(|ft| compat(ft, dty)),
                        Ty::Adt(def, _, args) if self.p.types.structs.contains_key(def) => self
                            .p
                            .types
                            .field_types(*def, None, args)
                            .and_then(|fs| fs.get(*i as usize).cloned())
                            .is_some_and(|ft| compat(&ft, dty)),
                        Ty::Adt(def, _, args) => {
                            (0..self.p.types.variant_count(*def) as u32).any(|v| {
                                self.p
                                    .types
                                    .field_types(*def, Some(v), args)
                                    .and_then(|fs| fs.get(*i as usize).cloned())
                                    .is_some_and(|ft| compat(&ft, dty))
                            })
                        }
                        _ => false,
                    };
                    if !ok {
                        self.err(
                            bb,
                            format!("field .{i} of `{t}` does not have type `{dty}`"),
                        );
                    }
                }
            }
            Rvalue::Index(a, i, _) => {
                let it = self.op_ty(bb, i);
                self.expect(bb, "index", it, &Ty::Int);
                match self.op_ty(bb, a) {
                    Some(Ty::Array(el)) => want(self, "index", *el),
                    Some(t) => self.err(bb, format!("indexing non-array `{t}`")),
                    None => {}
                }
            }
            Rvalue::Len(a) => {
                match self.op_ty(bb, a) {
                    Some(Ty::Array(_) | Ty::Str) | None => {}
                    Some(t) => self.err(bb, format!("len of `{t}`")),
                }
                want(self, "len", Ty::Int);
            }
            Rvalue::Tag(a) => {
                match self.op_ty(bb, a) {
                    Some(Ty::Adt(d, ..)) if self.p.types.enums.contains_key(&d) => {}
                    None => {}
                    Some(t) => self.err(bb, format!("tag of non-enum `{t}`")),
                }
                want(self, "tag", Ty::Int);
            }
            Rvalue::Cast(a) => {
                let t = self.op_ty(bb, a);
                self.expect(bb, "cast", t, &Ty::Bool);
                want(self, "cast", Ty::Int);
            }
            Rvalue::Builtin(b, ops) => self.check_builtin(bb, dty, *b, ops),
            Rvalue::WithField(a, i, v) => {
                if !self.f.ssa {
                    self.err(bb, "with_field outside SSA form");
                }
                let at = self.op_ty(bb, a);
                if let Some(t) = at {
                    match self
                        .fields_of(&t, None)
                        .and_then(|fs| fs.get(*i as usize).cloned())
                    {
                        Some(ft) => {
                            let vt = self.op_ty(bb, v);
                            self.expect(bb, "with_field value", vt, &ft);
                        }
                        None => self.err(bb, format!("with_field .{i} on `{t}`")),
                    }
                    want(self, "with_field", t);
                }
            }
        }
    }

    fn check_agg(&mut self, bb: usize, dty: &Ty, kind: AggKind, ops: &[Operand]) {
        let tys: Vec<Option<Ty>> = ops.iter().map(|o| self.op_ty(bb, o)).collect();
        let (expected, ok_shape): (Vec<Ty>, bool) = match (kind, dty) {
            (AggKind::Tuple, Ty::Tuple(ts)) => (ts.clone(), true),
            (AggKind::Array, Ty::Array(el)) => (vec![(**el).clone(); ops.len()], true),
            (AggKind::Struct(d), Ty::Adt(dd, _, args)) if d == *dd => self
                .p
                .types
                .field_types(d, None, args)
                .map_or((vec![], false), |f| (f, true)),
            (AggKind::Variant(d, v), Ty::Adt(dd, _, args)) if d == *dd => self
                .p
                .types
                .field_types(d, Some(v), args)
                .map_or((vec![], false), |f| (f, true)),
            _ => (vec![], false),
        };
        if !ok_shape || expected.len() != ops.len() {
            self.err(
                bb,
                format!(
                    "aggregate {kind:?} with {} operands does not build `{dty}`",
                    ops.len()
                ),
            );
            return;
        }
        for (t, e) in tys.into_iter().zip(&expected) {
            self.expect(bb, "aggregate element", t, e);
        }
    }

    fn check_builtin(&mut self, bb: usize, dty: &Ty, b: Builtin, ops: &[Operand]) {
        let tys: Vec<Option<Ty>> = ops.iter().map(|o| self.op_ty(bb, o)).collect();
        let printable = |t: &Option<Ty>| {
            t.as_ref()
                .is_none_or(|t| matches!(t, Ty::Int | Ty::Bool | Ty::Str | Ty::Unit))
        };
        let ok = match (b, tys.as_slice()) {
            (Builtin::Print, [a]) => printable(a),
            (Builtin::Println, []) => true,
            (Builtin::Println, [a]) => printable(a),
            (Builtin::Assert, [c]) => c.as_ref().is_none_or(|t| *t == Ty::Bool),
            (Builtin::Assert, [c, m]) => c.as_ref().is_none_or(|t| *t == Ty::Bool) && printable(m),
            (Builtin::Panic, [m]) => printable(m),
            (Builtin::ToStr, [a]) => printable(a) && compat(dty, &Ty::Str),
            (Builtin::Push, [Some(Ty::Array(el)), v]) => v.as_ref().is_none_or(|v| compat(v, el)),
            (Builtin::Pop, [Some(Ty::Array(el))]) => compat(el, dty),
            (Builtin::Chars, [a]) => {
                a.as_ref().is_none_or(|t| *t == Ty::Str)
                    && compat(dty, &Ty::Array(Box::new(Ty::Str)))
            }
            _ => false,
        };
        if !ok {
            self.err(bb, format!("ill-typed call of built-in `{}`", b.name()));
        }
    }

    fn check_term(&mut self, bb: usize) {
        let f = self.f;
        match &f.blocks[bb].term {
            Terminator::Goto(_) | Terminator::Unreachable => {}
            Terminator::Branch { cond, .. } => {
                let t = self.op_ty(bb, cond);
                self.expect(bb, "branch condition", t, &Ty::Bool);
            }
            Terminator::Switch { value, .. } => {
                let t = self.op_ty(bb, value);
                self.expect(bb, "switch value", t, &Ty::Int);
            }
            Terminator::Return(o) => {
                let t = self.op_ty(bb, o);
                self.expect(bb, "return value", t, &f.ret);
            }
            Terminator::Call {
                callee, args, dest, ..
            } => {
                if !self.local_ok(bb, *dest) {
                    return;
                }
                let dty = f.local_ty(*dest).clone();
                let (params, ret): (Vec<Ty>, Ty) = match callee {
                    Callee::Direct(id) => match self.p.funcs.get(*id as usize) {
                        Some(g) => (
                            (0..g.params).map(|i| g.locals[i].ty.clone()).collect(),
                            g.ret.clone(),
                        ),
                        None => return self.err(bb, format!("call of missing function fn#{id}")),
                    },
                    Callee::Indirect(o) => match self.op_ty(bb, o) {
                        Some(Ty::Fn(ps, r)) => (ps, *r),
                        None => return,
                        Some(t) => {
                            return self.err(bb, format!("indirect call of non-function `{t}`"));
                        }
                    },
                };
                if params.len() != args.len() {
                    return self.err(
                        bb,
                        format!(
                            "call passes {} arguments, callee takes {}",
                            args.len(),
                            params.len()
                        ),
                    );
                }
                for (a, p) in args.iter().zip(&params) {
                    let t = self.op_ty(bb, a);
                    self.expect(bb, "call argument", t, p);
                }
                if !compat(&ret, &dty) {
                    self.err(
                        bb,
                        format!("call returns `{ret}` into local of type `{dty}`"),
                    );
                }
            }
        }
    }

    /// Forward must-initialised analysis over reachable blocks.
    fn check_init(&mut self, reach: &[BlockId]) {
        let f = self.f;
        let nl = f.locals.len();
        let mut entry: Vec<Option<Vec<bool>>> = vec![None; f.blocks.len()];
        let mut start = vec![false; nl];
        start[..f.params].iter_mut().for_each(|x| *x = true);
        entry[0] = Some(start);
        let order = f.rpo();
        let mut changed = true;
        while changed {
            changed = false;
            for &b in &order {
                let Some(mut st) = entry[b as usize].clone() else {
                    continue;
                };
                self.transfer(b as usize, &mut st, false);
                for s in f.blocks[b as usize].term.successors() {
                    let slot = &mut entry[s as usize];
                    match slot {
                        None => {
                            *slot = Some(st.clone());
                            changed = true;
                        }
                        Some(old) => {
                            for (o, n) in old.iter_mut().zip(&st) {
                                if *o && !*n {
                                    *o = false;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        for &b in reach {
            if let Some(mut st) = entry[b as usize].clone() {
                self.transfer(b as usize, &mut st, true);
            }
        }
    }

    fn transfer(&mut self, bb: usize, st: &mut [bool], report: bool) {
        let f = self.f;
        let blk = &f.blocks[bb];
        let mut uses = Vec::new();
        for s in &blk.stmts {
            uses.clear();
            s.for_each_use(&mut |o| {
                if let Operand::Local(l) = o {
                    uses.push(*l)
                }
            });
            for &l in &uses {
                if report && !st[l as usize] {
                    self.err(bb, format!("use of possibly uninitialised local _{l}"));
                }
            }
            if let StmtKind::Assign(d, _) = &s.kind {
                st[*d as usize] = true;
            }
        }
        uses.clear();
        blk.term.for_each_use(&mut |o| {
            if let Operand::Local(l) = o {
                uses.push(*l)
            }
        });
        for &l in &uses {
            if report && !st[l as usize] {
                self.err(bb, format!("use of possibly uninitialised local _{l}"));
            }
        }
        if let Some(d) = blk.term.def() {
            st[d as usize] = true;
        }
    }
}
