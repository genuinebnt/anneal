//! The type checker: walks each function body once, inferring types by
//! unification (Hindley-Milner style, without let-polymorphism: only items are
//! generic), resolving local names, and producing typed HIR.
//!
//! Design notes (the "why"):
//! * `Ty::Error` absorbs everything, so one mistake never produces a second error.
//! * `Ty::Never` is *coerced* rather than unified: any expression of type `!`
//!   is accepted where a value is expected, and `join` skips `!` branches.
//! * Types are zonked and checked for leftover variables once per body, after all
//!   constraints are known; pattern analysis runs on the zonked tree.

use super::exhaustive;
use super::hir::*;
use super::index::{IndexNode, IndexNodeKind, SemanticIndex};
use super::types::*;
use super::unify::{Infer, UnifyError};
use super::{CheckOptions, CheckResult};
use crate::diag::{Applicability, Diagnostic, closest_match};
use crate::source::Span;
use crate::syntax::ast::{self, BinOp, Expr, ExprKind, ItemKind, Lit, StmtKind, UnOp};
use std::collections::{HashMap, HashSet};

pub(super) struct FnSig {
    pub generics: Vec<Name>,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub ret_span: Option<Span>,
}

pub(super) struct LocalInfo {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
    pub used: bool,
    pub mutated: bool,
    pub is_param: bool,
    pub name_span: Span,
    pub decl_span: Span,
}

pub(super) struct Scope {
    pub names: Vec<(String, DefId)>,
    pub end: u32,
}

pub(super) struct LoopCtx {
    pub is_loop: bool,
    pub break_ty: Option<Ty>,
    pub has_break: bool,
}

/// Extra context for a type mismatch message.
#[derive(Default, Clone)]
pub(super) struct Why {
    pub msg: Option<&'static str>,
    pub origin: Option<(Span, String)>,
    pub help: Option<String>,
}

impl Why {
    pub fn msg(msg: &'static str) -> Why {
        Why {
            msg: Some(msg),
            ..Why::default()
        }
    }
    pub fn origin(span: Span, text: &str) -> Why {
        Why {
            origin: Some((span, text.to_string())),
            ..Why::default()
        }
    }
}

pub(super) struct Checker<'m> {
    pub m: &'m ast::Module,
    pub opts: CheckOptions,
    pub diags: Vec<Diagnostic>,
    pub prog: Program,
    pub index: SemanticIndex,
    pub types: HashMap<String, DefId>,
    pub values: HashMap<String, DefId>,
    pub adt_generics: HashMap<DefId, Vec<Name>>,
    pub sigs: HashMap<DefId, FnSig>,
    pub item_defs: Vec<Option<DefId>>,
    pub has_item_errors: bool,
    pub cur_item: Option<DefId>,
    pub used_types: HashSet<DefId>,
    pub uses: HashMap<DefId, Vec<DefId>>,
    // per-body state
    pub infer: Infer,
    pub scopes: Vec<Scope>,
    pub locals: HashMap<DefId, LocalInfo>,
    pub loops: Vec<LoopCtx>,
    pub ret_ty: Option<Ty>,
    pub ret_origin: Option<(Span, String)>,
    pub generics: Vec<Name>,
}

pub(super) fn run(m: &ast::Module, opts: &CheckOptions) -> CheckResult {
    let mut c = Checker {
        m,
        opts: *opts,
        diags: Vec::new(),
        prog: Program::default(),
        index: SemanticIndex::default(),
        types: HashMap::new(),
        values: HashMap::new(),
        adt_generics: HashMap::new(),
        sigs: HashMap::new(),
        item_defs: Vec::new(),
        has_item_errors: false,
        cur_item: None,
        used_types: HashSet::new(),
        uses: HashMap::new(),
        infer: Infer::new(),
        scopes: Vec::new(),
        locals: HashMap::new(),
        loops: Vec::new(),
        ret_ty: None,
        ret_origin: None,
        generics: Vec::new(),
    };
    c.collect_items();
    c.lower_signatures();
    c.check_bodies();
    c.finish()
}

/// What a struct literal builds: display name, fields, generics, type and variant index.
type LitTarget = (String, Vec<FieldInfo>, Vec<Name>, DefId, Option<usize>);

fn peel(e: &Expr) -> &Expr {
    match &e.kind {
        ExprKind::Paren(inner) => peel(inner),
        _ => e,
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

impl Checker<'_> {
    // ------------------------------------------------------------ driver

    fn check_bodies(&mut self) {
        let m = self.m;
        for (ix, item) in m.items.iter().enumerate() {
            let Some(def) = self.item_defs[ix] else {
                continue;
            };
            self.cur_item = Some(def);
            match &item.kind {
                ItemKind::Fn(f) => self.check_fn(def, f, item.span),
                ItemKind::Const(c) => self.check_const(def, c, item.span),
                _ => {}
            }
        }
        self.cur_item = None;
    }

    fn reset_body(&mut self) {
        self.infer = Infer::new();
        self.scopes.clear();
        self.locals.clear();
        self.loops.clear();
    }

    fn check_fn(&mut self, def: DefId, f: &ast::FnDecl, span: Span) {
        self.reset_body();
        let Some(sig) = self.sigs.get(&def) else {
            return;
        };
        let (generics, ptys, ret, ret_span) = (
            sig.generics.clone(),
            sig.params.clone(),
            sig.ret.clone(),
            sig.ret_span,
        );
        self.generics = generics.clone();
        self.ret_ty = Some(ret.clone());
        self.ret_origin = Some((
            ret_span.unwrap_or(f.name.span),
            format!("expected `{ret}` because of this return type"),
        ));
        let first_local = self.prog.defs.len() as u32;
        self.push_scope(f.body.span.end);
        let mut params = Vec::new();
        for (p, ty) in f.params.iter().zip(ptys) {
            let d = self.new_local(&p.name.name, p.name.span, p.span, p.mutable, ty, true);
            if !p.name.name.is_empty() {
                self.scope_insert(d, f.body.span.start);
            }
            params.push(d);
        }
        let mut body = self.check_block(&f.body);
        self.demand_body(&body, &f.body, &ret);
        self.pop_scope();
        let last_local = self.prog.defs.len() as u32;
        self.finalize_body(&mut body);
        let ty = self.prog.defs[def.0 as usize]
            .ty
            .clone()
            .unwrap_or(Ty::Error);
        let _ = ty;
        self.prog.fns.push(HirFn {
            def,
            name: f.name.name.clone(),
            generics,
            params,
            ret: self.finalize_ty(&ret),
            body,
            span,
            locals_start: first_local,
            locals_len: last_local - first_local,
        });
        self.prog.rebuild_indexes();
        self.generics.clear();
        self.ret_ty = None;
    }

    fn check_const(&mut self, def: DefId, c: &ast::ConstDecl, span: Span) {
        self.reset_body();
        self.generics.clear();
        let ty = self.prog.defs[def.0 as usize]
            .ty
            .clone()
            .unwrap_or(Ty::Error);
        self.ret_ty = None;
        let first_local = self.prog.defs.len() as u32;
        self.push_scope(c.value.span.end);
        let mut body = self.check_expr(&c.value);
        self.demand(
            c.value.span,
            &ty,
            &body.ty.clone(),
            Why::origin(c.ty.span, &format!("expected `{ty}` because of this type")),
        );
        self.pop_scope();
        let last_local = self.prog.defs.len() as u32;
        self.finalize_body(&mut body);
        self.prog.consts.push(HirConst {
            def,
            name: c.name.name.clone(),
            ty,
            body,
            span,
            locals_start: first_local,
            locals_len: last_local - first_local,
        });
        self.prog.rebuild_indexes();
    }

    /// The function body block's type must agree with the declared return type.
    fn demand_body(&mut self, body: &HirExpr, block: &ast::Block, ret: &Ty) {
        let span = block.tail.as_ref().map(|t| t.span).unwrap_or_else(|| {
            Span::new(
                block.span.end.saturating_sub(1) as usize,
                block.span.end as usize,
            )
        });
        let why = Why {
            origin: self.ret_origin.clone(),
            ..Why::default()
        };
        self.demand(span, ret, &body.ty.clone(), why);
    }

    /// Zonk all types, report variables that stayed unsolved, run pattern analysis,
    /// and feed the semantic index.
    fn finalize_body(&mut self, body: &mut HirExpr) {
        let mut reported = false;
        {
            let infer = &self.infer;
            let diags = &mut self.diags;
            let index = std::cell::RefCell::new(&mut self.index);
            let erase = |t: &Ty| erase_vars(&infer.zonk(t));
            walk_expr(
                body,
                &mut |e| {
                    let z = infer.zonk(&e.ty);
                    if z.has_var() && !reported && !z.has_error() {
                        reported = true;
                        diags.push(
                            Diagnostic::error("E0310", "type annotations needed")
                                .primary(e.span, format!("cannot infer the type (found `{z}`)"))
                                .help("add a type annotation, e.g. `let x: [int] = [];`"),
                        );
                    }
                    e.ty = erase_vars(&z);
                    if let HirExprKind::Fn(_, targs) = &mut e.kind {
                        for t in targs.iter_mut() {
                            *t = erase(t);
                        }
                    }
                    let mut ix = index.borrow_mut();
                    ix.nodes.push(IndexNode {
                        id: e.id,
                        span: e.span,
                        kind: IndexNodeKind::Expr,
                    });
                    ix.types.insert(e.id, e.ty.clone());
                },
                &mut |p| {
                    p.ty = erase(&p.ty);
                    let mut ix = index.borrow_mut();
                    ix.nodes.push(IndexNode {
                        id: p.id,
                        span: p.span,
                        kind: IndexNodeKind::Pattern,
                    });
                    ix.types.insert(p.id, p.ty.clone());
                },
            );
        }
        let ids: Vec<DefId> = self.locals.keys().copied().collect();
        for id in ids {
            let t = self.finalize_ty(&self.locals[&id].ty.clone());
            let mutable = self.locals[&id].mutable;
            let d = &mut self.prog.defs[id.0 as usize];
            d.ty = Some(t);
            d.mutable = mutable;
        }
        exhaustive::check_body(&self.prog, &mut self.diags, body);
    }

    fn finalize_ty(&self, t: &Ty) -> Ty {
        erase_vars(&self.infer.zonk(t))
    }

    fn finish(mut self) -> CheckResult {
        self.check_main();
        self.check_const_cycles();
        self.warn_unused_items();
        self.prog.rebuild_indexes();
        self.index.defs = self.prog.defs.clone();
        crate::diag::sort_and_dedup(&mut self.diags);
        CheckResult {
            hir: self.prog,
            diagnostics: self.diags,
            index: self.index,
        }
    }

    fn check_main(&mut self) {
        match self.values.get("main").copied() {
            Some(def) if self.prog.defs[def.0 as usize].kind == DefKind::Fn => {
                self.prog.main = Some(def);
                if let Some(sig) = self.sigs.get(&def)
                    && (!sig.params.is_empty() || sig.ret != Ty::Unit)
                {
                    let span = self.prog.defs[def.0 as usize].span;
                    self.diags.push(
                        Diagnostic::error("E0209", "`main` has an invalid signature")
                            .primary(
                                span,
                                "`main` must be `fn main()` with no parameters and no return value",
                            )
                            .note(format!(
                                "found `{}`",
                                self.prog.defs[def.0 as usize]
                                    .ty
                                    .clone()
                                    .unwrap_or(Ty::Error)
                            )),
                    );
                }
            }
            Some(def) => {
                let span = self.prog.defs[def.0 as usize].span;
                self.diags.push(
                    Diagnostic::error("E0209", "`main` must be a function")
                        .primary(span, "not a function"),
                );
            }
            None if !self.has_item_errors => {
                let end = self.m.span.end as usize;
                self.diags.push(
                    Diagnostic::error("E0208", "`main` function not found in this program")
                        .primary(
                            Span::point(end),
                            "consider adding `fn main() { ... }` to this file",
                        )
                        .help("every PEBBLE program starts running at `fn main()`"),
                );
            }
            None => {}
        }
    }

    fn check_const_cycles(&mut self) {
        let consts: Vec<DefId> = self.prog.consts.iter().map(|c| c.def).collect();
        let mut reported: HashSet<DefId> = HashSet::new();
        for &start in &consts {
            // DFS looking for a path from `start` back to `start` through constants only.
            let mut stack = vec![start];
            let mut seen: HashSet<DefId> = HashSet::new();
            let mut cyclic = false;
            while let Some(c) = stack.pop() {
                for &n in self.uses.get(&c).into_iter().flatten() {
                    if self.prog.defs[n.0 as usize].kind != DefKind::Const {
                        continue;
                    }
                    if n == start {
                        cyclic = true;
                    } else if seen.insert(n) {
                        stack.push(n);
                    }
                }
            }
            if cyclic && reported.insert(start) {
                let d = &self.prog.defs[start.0 as usize];
                let (name, span) = (d.name.clone(), d.span);
                self.diags.push(
                    Diagnostic::error("E0216", format!("constant `{name}` depends on itself"))
                        .primary(span, "cycle detected when evaluating this constant")
                        .note(
                            "constants are evaluated before `main` runs, so they cannot be cyclic",
                        ),
                );
            }
        }
    }

    /// Dead-code warnings: items unreachable from `main`, and types never mentioned.
    fn warn_unused_items(&mut self) {
        if self.has_item_errors {
            return;
        }
        if let Some(main) = self.prog.main {
            let mut reached: HashSet<DefId> = HashSet::new();
            let mut stack = vec![main];
            while let Some(d) = stack.pop() {
                if reached.insert(d) {
                    stack.extend(self.uses.get(&d).into_iter().flatten().copied());
                }
            }
            for ix in 0..self.m.items.len() {
                let Some(def) = self.item_defs[ix] else {
                    continue;
                };
                let d = &self.prog.defs[def.0 as usize];
                if reached.contains(&def) || d.name.starts_with('_') || d.name.is_empty() {
                    continue;
                }
                let (name, span, kind) = (d.name.clone(), d.span, d.kind);
                match kind {
                    DefKind::Fn => self.diags.push(
                        Diagnostic::warning("W0502", format!("function `{name}` is never used"))
                            .primary(span, "not reachable from `main`")
                            .help("if this is intentional, prefix it with an underscore"),
                    ),
                    DefKind::Const => self.diags.push(
                        Diagnostic::warning("W0507", format!("constant `{name}` is never used"))
                            .primary(span, "never used"),
                    ),
                    _ => {}
                }
            }
        }
        for ix in 0..self.m.items.len() {
            let Some(def) = self.item_defs[ix] else {
                continue;
            };
            let d = &self.prog.defs[def.0 as usize];
            if matches!(d.kind, DefKind::Struct | DefKind::Enum)
                && !self.used_types.contains(&def)
                && !d.name.starts_with('_')
                && !d.name.is_empty()
            {
                let what = if d.kind == DefKind::Struct {
                    "struct"
                } else {
                    "enum"
                };
                let (name, span) = (d.name.clone(), d.span);
                self.diags.push(
                    Diagnostic::warning("W0508", format!("{what} `{name}` is never used"))
                        .primary(span, "never mentioned"),
                );
            }
        }
    }

    // ------------------------------------------------------------ scopes and locals

    pub(super) fn push_scope(&mut self, end: u32) {
        self.scopes.push(Scope {
            names: Vec::new(),
            end,
        });
    }

    /// Leave a scope, emitting unused-variable and unused-`mut` warnings.
    pub(super) fn pop_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        for (name, def) in &scope.names {
            let Some(l) = self.locals.get(def) else {
                continue;
            };
            if name.starts_with('_') || self.infer.shallow(&l.ty).is_error() {
                continue;
            }
            if !l.used {
                let what = if l.is_param { "parameter" } else { "variable" };
                self.diags.push(
                    Diagnostic::warning("W0501", format!("unused {what}: `{name}`"))
                        .primary(l.name_span, format!("this {what} is never read"))
                        .suggest(
                            format!(
                                "if this is intentional, prefix it with an underscore: `_{name}`"
                            ),
                            l.name_span,
                            format!("_{name}"),
                            Applicability::MaybeIncorrect,
                        ),
                );
            } else if l.mutable && !l.mutated {
                let mut d = Diagnostic::warning(
                    "W0506",
                    format!("variable `{name}` does not need to be mutable"),
                )
                .primary(l.decl_span, "declared `mut` but never mutated");
                let mut_span = Span::new(l.decl_span.start as usize, l.name_span.start as usize);
                if !mut_span.is_empty() {
                    d = d.suggest(
                        "remove this `mut`",
                        mut_span,
                        "",
                        Applicability::MachineApplicable,
                    );
                }
                self.diags.push(d);
            }
        }
    }

    pub(super) fn new_local(
        &mut self,
        name: &str,
        name_span: Span,
        decl_span: Span,
        mutable: bool,
        ty: Ty,
        is_param: bool,
    ) -> DefId {
        let kind = if is_param {
            DefKind::Param
        } else {
            DefKind::Local
        };
        let def = self.new_def(name, kind, name_span, decl_span, None, Some(ty.clone()));
        self.locals.insert(
            def,
            LocalInfo {
                name: name.to_string(),
                ty,
                mutable,
                used: false,
                mutated: false,
                is_param,
                name_span,
                decl_span,
            },
        );
        def
    }

    /// Make a local visible in the innermost scope from offset `vis_from` on.
    pub(super) fn scope_insert(&mut self, def: DefId, vis_from: u32) {
        let (name, name_span) = {
            let l = &self.locals[&def];
            (l.name.clone(), l.name_span)
        };
        if self.opts.warn_shadow
            && !name.starts_with('_')
            && let Some(prev) = self.lookup_local(&name)
        {
            let prev_span = self.locals[&prev].name_span;
            self.diags.push(
                Diagnostic::warning("W0505", format!("`{name}` shadows an earlier variable"))
                    .primary(name_span, "this binding shadows")
                    .secondary(prev_span, "the shadowed binding"),
            );
        }
        let end = match self.scopes.last_mut() {
            Some(s) => {
                s.names.push((name, def));
                s.end
            }
            None => return,
        };
        self.index.visibility.push((def, vis_from, end));
    }

    pub(super) fn lookup_local(&self, name: &str) -> Option<DefId> {
        self.scopes.iter().rev().find_map(|s| {
            s.names
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, d)| *d)
        })
    }

    fn visible_local_names(&self) -> Vec<&str> {
        self.scopes
            .iter()
            .flat_map(|s| s.names.iter().map(|(n, _)| n.as_str()))
            .collect()
    }

    // ------------------------------------------------------------ unification helpers

    pub(super) fn fresh(&mut self) -> Ty {
        self.infer.fresh()
    }

    pub(super) fn shallow(&self, t: &Ty) -> Ty {
        self.infer.shallow(t)
    }

    /// Require `found` to be usable where `expected` is needed. `!` is accepted anywhere.
    pub(super) fn demand(&mut self, span: Span, expected: &Ty, found: &Ty, why: Why) -> bool {
        if self.shallow(found) == Ty::Never {
            return true;
        }
        match self.infer.unify(expected, found) {
            Ok(()) => true,
            Err(UnifyError::Mismatch(..)) => {
                let (e, f) = (self.infer.zonk(expected), self.infer.zonk(found));
                if e.has_error() || f.has_error() {
                    return false;
                }
                let mut d = Diagnostic::error("E0301", why.msg.unwrap_or("mismatched types"))
                    .primary(span, format!("expected `{e}`, found `{f}`"));
                if let Some((s, t)) = why.origin {
                    d = d.secondary(s, t);
                }
                match (&e, &f) {
                    (Ty::Str, Ty::Int | Ty::Bool) => d = d.help("convert with `to_str(..)`"),
                    (Ty::Int, Ty::Bool) => d = d.help("convert with `x as int`"),
                    (Ty::Bool, Ty::Int) => d = d.help("compare with `!= 0` to get a `bool`"),
                    _ => {}
                }
                if let Some(h) = why.help {
                    d = d.help(h);
                }
                self.diags.push(d);
                false
            }
            Err(UnifyError::Occurs(_, t)) => {
                self.diags.push(
                    Diagnostic::error("E0311", "cannot construct an infinite type").primary(
                        span,
                        format!("this would make a type that contains itself (`{t}`)"),
                    ),
                );
                false
            }
        }
    }

    /// The common type of two branches (`!` branches do not constrain).
    fn join(&mut self, a: &HirExpr, b: &HirExpr, msg: &'static str) -> Ty {
        let (ta, tb) = (self.shallow(&a.ty), self.shallow(&b.ty));
        if ta == Ty::Never {
            return b.ty.clone();
        }
        if tb == Ty::Never {
            return a.ty.clone();
        }
        let why = Why {
            msg: Some(msg),
            origin: Some((
                a.span,
                format!("this is of type `{}`", self.infer.zonk(&a.ty)),
            )),
            help: None,
        };
        self.demand(b.span, &a.ty, &b.ty, why);
        a.ty.clone()
    }

    fn mk(&self, e: &Expr, ty: Ty, kind: HirExprKind) -> HirExpr {
        HirExpr {
            id: e.id,
            span: e.span,
            ty,
            kind,
        }
    }

    fn err_expr(&self, e: &Expr) -> HirExpr {
        self.mk(e, Ty::Error, HirExprKind::Error)
    }

    pub(super) fn fresh_args(&mut self, generics: &[Name]) -> Vec<Ty> {
        generics.iter().map(|_| self.fresh()).collect()
    }

    fn note_use(&mut self, def: DefId) {
        if let Some(cur) = self.cur_item {
            self.uses.entry(cur).or_default().push(def);
        }
    }

    // ------------------------------------------------------------ blocks and statements

    pub(super) fn check_block(&mut self, b: &ast::Block) -> HirExpr {
        self.push_scope(b.span.end);
        let mut stmts = Vec::new();
        let mut diverged: Option<Span> = None;
        let mut warned = false;
        for s in &b.stmts {
            if let (Some(dspan), false) = (diverged, warned)
                && !matches!(s.kind, StmtKind::Error)
            {
                warned = true;
                self.diags.push(
                    Diagnostic::warning("W0503", "unreachable statement")
                        .primary(s.span, "unreachable statement")
                        .secondary(dspan, "any code following this expression is unreachable"),
                );
            }
            match &s.kind {
                StmtKind::Error => {}
                StmtKind::Let { pat, ty, init } => {
                    stmts.push(self.check_let(s, pat, ty.as_ref(), init.as_ref()))
                }
                StmtKind::Expr { expr, semi } => {
                    let h = self.check_expr(expr);
                    if !semi {
                        self.demand(
                            expr.span,
                            &Ty::Unit,
                            &h.ty.clone(),
                            Why::msg("expected `()`, but this block-like expression has a value"),
                        );
                    } else {
                        self.warn_unused_value(expr, &h);
                    }
                    if self.shallow(&h.ty) == Ty::Never && diverged.is_none() {
                        diverged = Some(expr.span);
                    }
                    stmts.push(HirStmt::Expr(h));
                }
            }
        }
        let mut tail = None;
        let mut ty = Ty::Unit;
        if let Some(t) = &b.tail {
            if let (Some(dspan), false) = (diverged, warned) {
                self.diags.push(
                    Diagnostic::warning("W0503", "unreachable expression")
                        .primary(t.span, "unreachable expression")
                        .secondary(dspan, "any code following this expression is unreachable"),
                );
            }
            let h = self.check_expr(t);
            ty = h.ty.clone();
            tail = Some(Box::new(h));
        }
        if diverged.is_some() {
            ty = Ty::Never;
        }
        self.pop_scope();
        HirExpr {
            id: b.id,
            span: b.span,
            ty,
            kind: HirExprKind::Block(HirBlock { stmts, tail }),
        }
    }

    fn warn_unused_value(&mut self, e: &Expr, h: &HirExpr) {
        let inner = peel(e);
        let pure = matches!(
            inner.kind,
            ExprKind::Lit(_)
                | ExprKind::Path(_)
                | ExprKind::Unary(..)
                | ExprKind::Binary(..)
                | ExprKind::Field(..)
                | ExprKind::TupleField(..)
                | ExprKind::Index(..)
                | ExprKind::Tuple(_)
                | ExprKind::Array(_)
                | ExprKind::Cast(..)
                | ExprKind::Struct { .. }
        );
        if pure && !h.ty.is_error() && h.ty != Ty::Never {
            self.diags.push(
                Diagnostic::warning("W0509", "unused value that is never used")
                    .primary(e.span, "this value is computed and then discarded"),
            );
        }
    }

    fn check_let(
        &mut self,
        s: &ast::Stmt,
        pat: &ast::Pattern,
        ann: Option<&ast::Type>,
        init: Option<&Expr>,
    ) -> HirStmt {
        let ann_ty = ann.map(|t| {
            let g = self.generics.clone();
            (self.lower_type(t, &g), t.span)
        });
        let init_h = init.map(|e| self.check_expr(e));
        let ty = match (&ann_ty, &init_h) {
            (Some((a, aspan)), Some(h)) => {
                let (a, aspan) = (a.clone(), *aspan);
                self.demand(
                    h.span,
                    &a,
                    &h.ty.clone(),
                    Why::origin(aspan, "expected due to this type"),
                );
                a
            }
            (Some((a, _)), None) => a.clone(),
            (None, Some(h)) => h.ty.clone(),
            (None, None) => Ty::Error,
        };
        let mut binds = Vec::new();
        let hp = self.check_pat(pat, &ty, &mut binds, None);
        for (_, def, _) in &binds {
            self.scope_insert(*def, s.span.end);
        }
        let init_h = init_h.unwrap_or(HirExpr {
            id: s.id,
            span: s.span,
            ty: Ty::Error,
            kind: HirExprKind::Error,
        });
        HirStmt::Let {
            pat: hp,
            init: init_h,
        }
    }

    // ------------------------------------------------------------ expressions

    pub(super) fn check_expr(&mut self, e: &Expr) -> HirExpr {
        match &e.kind {
            ExprKind::Error => self.err_expr(e),
            ExprKind::Lit(l) => {
                let ty = match l {
                    Lit::Int(_) => Ty::Int,
                    Lit::Bool(_) => Ty::Bool,
                    Lit::Str(_) => Ty::Str,
                    Lit::Unit => Ty::Unit,
                };
                self.mk(e, ty, HirExprKind::Lit(l.clone()))
            }
            ExprKind::Paren(inner) => self.check_expr(inner),
            ExprKind::Path(p) => {
                if p.segments.len() == 1 {
                    self.check_name(e, &p.segments[0])
                } else {
                    self.check_variant_value(e, p)
                }
            }
            ExprKind::Unary(op, a) => self.check_unary(e, *op, a),
            ExprKind::Binary(op, a, b) => {
                let (ha, hb) = (self.check_expr(a), self.check_expr(b));
                if matches!(op, BinOp::Div | BinOp::Rem)
                    && let ExprKind::Lit(Lit::Int(0)) = peel(b).kind
                {
                    self.diags.push(
                        Diagnostic::warning(
                            "W0516",
                            "this operation will always trap: division by zero",
                        )
                        .primary(b.span, "the divisor is the literal `0`"),
                    );
                }
                let ty = self.binary_ty(e, *op, &ha, &hb);
                self.mk(e, ty, HirExprKind::Binary(*op, Box::new(ha), Box::new(hb)))
            }
            ExprKind::Assign { op, target, value } => self.check_assign(e, *op, target, value),
            ExprKind::Call { callee, args } => self.check_call(e, callee, args),
            ExprKind::MethodCall { recv, method, args } => self.check_method(e, recv, method, args),
            ExprKind::Field(base, name) => self.check_field(e, base, name),
            ExprKind::TupleField(base, n, nspan) => {
                let b = self.check_expr(base);
                match self.shallow(&b.ty) {
                    Ty::Tuple(ts) => match ts.get(*n as usize) {
                        Some(t) => {
                            let t = t.clone();
                            self.mk(e, t, HirExprKind::TupleField(Box::new(b), *n as usize))
                        }
                        None => {
                            self.diags.push(
                                Diagnostic::error(
                                    "E0318",
                                    format!("tuple index {n} is out of range"),
                                )
                                .primary(
                                    *nspan,
                                    format!("the tuple has only {}", plural(ts.len(), "element")),
                                )
                                .note(format!("the tuple has type `{}`", Ty::Tuple(ts.clone()))),
                            );
                            self.err_expr(e)
                        }
                    },
                    Ty::Error => self.err_expr(e),
                    Ty::Var(_) => self.needs_annotation(e, "tuple access"),
                    t => {
                        let t = self.infer.zonk(&t);
                        self.diags.push(
                            Diagnostic::error("E0306", format!("type `{t}` is not a tuple"))
                                .primary(e.span, "tuple fields (`.0`) need a tuple"),
                        );
                        self.err_expr(e)
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                let (b, i) = (self.check_expr(base), self.check_expr(idx));
                self.demand(
                    i.span,
                    &Ty::Int,
                    &i.ty.clone(),
                    Why::msg("array indices must be `int`"),
                );
                match self.shallow(&b.ty) {
                    Ty::Array(t) => self.mk(e, *t, HirExprKind::Index(Box::new(b), Box::new(i))),
                    Ty::Error => self.err_expr(e),
                    Ty::Var(_) => self.needs_annotation(e, "indexing"),
                    t => {
                        let t = self.infer.zonk(&t);
                        let mut d = Diagnostic::error(
                            "E0307",
                            format!("cannot index into a value of type `{t}`"),
                        )
                        .primary(base.span, "not an array");
                        if t == Ty::Str {
                            d = d.help("use `chars(s)` to get an array of characters");
                        }
                        self.diags.push(d);
                        self.err_expr(e)
                    }
                }
            }
            ExprKind::Struct { path, fields } => self.check_struct_lit(e, path, fields),
            ExprKind::Tuple(xs) => {
                let hs: Vec<HirExpr> = xs.iter().map(|x| self.check_expr(x)).collect();
                let ty = Ty::Tuple(hs.iter().map(|h| h.ty.clone()).collect());
                self.mk(e, ty, HirExprKind::Tuple(hs))
            }
            ExprKind::Array(xs) => {
                let elem = self.fresh();
                let mut hs = Vec::new();
                for x in xs {
                    let h = self.check_expr(x);
                    self.demand(
                        h.span,
                        &elem,
                        &h.ty.clone(),
                        Why::msg("array elements must all have the same type"),
                    );
                    hs.push(h);
                }
                self.mk(e, Ty::Array(Box::new(elem)), HirExprKind::Array(hs))
            }
            ExprKind::If { cond, then, els } => self.check_if(e, cond, then, els.as_deref()),
            ExprKind::Match { scrut, arms } => self.check_match(e, scrut, arms),
            ExprKind::Block(b) => {
                let h = self.check_block(b);
                HirExpr { id: e.id, ..h }
            }
            ExprKind::While { cond, body } => {
                let c = self.check_expr(cond);
                self.demand(
                    c.span,
                    &Ty::Bool,
                    &c.ty.clone(),
                    Why::msg("`while` condition must be `bool`"),
                );
                if let ExprKind::Lit(Lit::Bool(true)) = peel(cond).kind {
                    let kw = Span::new(e.span.start as usize, cond.span.end as usize);
                    self.diags.push(
                        Diagnostic::warning("W0510", "denote infinite loops with `loop`")
                            .primary(kw, "`while true`")
                            .suggest(
                                "use `loop` instead",
                                kw,
                                "loop",
                                Applicability::MachineApplicable,
                            ),
                    );
                }
                self.loops.push(LoopCtx {
                    is_loop: false,
                    break_ty: None,
                    has_break: false,
                });
                let b = self.check_block(body);
                self.loops.pop();
                self.demand(
                    body.span,
                    &Ty::Unit,
                    &b.ty.clone(),
                    Why::msg("the body of a loop must have type `()`"),
                );
                let hb = HirExpr { id: body.id, ..b };
                self.mk(e, Ty::Unit, HirExprKind::While(Box::new(c), Box::new(hb)))
            }
            ExprKind::Loop(body) => {
                self.loops.push(LoopCtx {
                    is_loop: true,
                    break_ty: None,
                    has_break: false,
                });
                let b = self.check_block(body);
                let ctx = self.loops.pop();
                self.demand(
                    body.span,
                    &Ty::Unit,
                    &b.ty.clone(),
                    Why::msg("the body of a loop must have type `()`"),
                );
                let ty = match ctx {
                    Some(c) if c.has_break => c.break_ty.unwrap_or(Ty::Unit),
                    _ => Ty::Never,
                };
                let hb = HirExpr { id: body.id, ..b };
                self.mk(e, ty, HirExprKind::Loop(Box::new(hb)))
            }
            ExprKind::Break(v) => self.check_break(e, v.as_deref()),
            ExprKind::Continue => {
                if self.loops.is_empty() {
                    self.outside_loop(e, "continue");
                }
                self.mk(e, Ty::Never, HirExprKind::Continue)
            }
            ExprKind::Return(v) => {
                let hv = v.as_ref().map(|x| self.check_expr(x));
                match self.ret_ty.clone() {
                    None => {
                        self.diags.push(
                            Diagnostic::error("E0215", "`return` outside of a function")
                                .primary(e.span, "cannot return from a constant initialiser"),
                        );
                    }
                    Some(ret) => {
                        let why = Why {
                            origin: self.ret_origin.clone(),
                            ..Why::default()
                        };
                        match &hv {
                            Some(h) => {
                                self.demand(h.span, &ret, &h.ty.clone(), why);
                            }
                            None => {
                                self.demand(e.span, &ret, &Ty::Unit, why);
                            }
                        }
                    }
                }
                self.mk(e, Ty::Never, HirExprKind::Return(hv.map(Box::new)))
            }
            ExprKind::Cast(a, t) => {
                let h = self.check_expr(a);
                let g = self.generics.clone();
                let target = self.lower_type(t, &g);
                match (self.shallow(&h.ty), &target) {
                    (Ty::Bool, Ty::Int) => self.mk(e, Ty::Int, HirExprKind::Cast(Box::new(h))),
                    (Ty::Int, Ty::Int) => {
                        self.diags.push(
                            Diagnostic::warning("W0512", "casting `int` as `int` is redundant")
                                .primary(e.span, "this cast does nothing")
                                .suggest(
                                    "remove the cast",
                                    Span::new(a.span.end as usize, e.span.end as usize),
                                    "",
                                    Applicability::MachineApplicable,
                                ),
                        );
                        h
                    }
                    (Ty::Error, _) | (_, Ty::Error) => self.err_expr(e),
                    (from, to) => {
                        let from = self.infer.zonk(&from);
                        let mut d =
                            Diagnostic::error("E0309", format!("cannot cast `{from}` as `{to}`"))
                                .primary(e.span, "invalid cast")
                                .note("`as` only converts `bool` to `int`");
                        if from == Ty::Int && *to == Ty::Bool {
                            d = d.help("compare with `!= 0` instead");
                        }
                        self.diags.push(d);
                        self.err_expr(e)
                    }
                }
            }
        }
    }

    fn needs_annotation(&mut self, e: &Expr, what: &str) -> HirExpr {
        self.diags.push(
            Diagnostic::error("E0310", "type annotations needed")
                .primary(e.span, format!("the type must be known before {what}"))
                .help("add a type annotation to the variable"),
        );
        self.err_expr(e)
    }

    fn outside_loop(&mut self, e: &Expr, kw: &str) {
        self.diags.push(
            Diagnostic::error("E0214", format!("`{kw}` outside of a loop"))
                .primary(e.span, format!("cannot `{kw}` here"))
                .note("`break` and `continue` only work inside `while` and `loop`"),
        );
    }

    fn check_break(&mut self, e: &Expr, v: Option<&Expr>) -> HirExpr {
        let hv = v.map(|x| self.check_expr(x));
        if self.loops.is_empty() {
            self.outside_loop(e, "break");
        } else {
            let i = self.loops.len() - 1;
            if !self.loops[i].is_loop {
                if let Some(h) = &hv {
                    self.diags.push(
                        Diagnostic::error("E0219", "`break` with value from a `while` loop")
                            .primary(h.span, "can only break with a value inside `loop`")
                            .help("use `loop` if you need a value"),
                    );
                }
            } else {
                let vty = hv.as_ref().map(|h| h.ty.clone()).unwrap_or(Ty::Unit);
                self.loops[i].has_break = true;
                match self.loops[i].break_ty.clone() {
                    None => self.loops[i].break_ty = Some(vty),
                    Some(bt) => {
                        let span = hv.as_ref().map(|h| h.span).unwrap_or(e.span);
                        self.demand(
                            span,
                            &bt,
                            &vty,
                            Why::msg("`break` values must all have the same type"),
                        );
                    }
                }
            }
        }
        self.mk(e, Ty::Never, HirExprKind::Break(hv.map(Box::new)))
    }

    fn check_if(
        &mut self,
        e: &Expr,
        cond: &Expr,
        then: &ast::Block,
        els: Option<&Expr>,
    ) -> HirExpr {
        let c = self.check_expr(cond);
        self.demand(
            c.span,
            &Ty::Bool,
            &c.ty.clone(),
            Why::msg("`if` condition must be `bool`"),
        );
        let t = self.check_block(then);
        let t = HirExpr { id: then.id, ..t };
        match els {
            Some(el) => {
                let h = self.check_expr(el);
                let ty = self.join(&t, &h, "`if` and `else` have incompatible types");
                self.mk(
                    e,
                    ty,
                    HirExprKind::If(Box::new(c), Box::new(t), Some(Box::new(h))),
                )
            }
            None => {
                self.demand(
                    then.span,
                    &Ty::Unit,
                    &t.ty.clone(),
                    Why {
                        msg: Some("`if` may be missing an `else` clause"),
                        help: Some(
                            "an `if` without `else` must have type `()`; add an `else` branch"
                                .into(),
                        ),
                        origin: None,
                    },
                );
                self.mk(e, Ty::Unit, HirExprKind::If(Box::new(c), Box::new(t), None))
            }
        }
    }

    fn check_match(&mut self, e: &Expr, scrut: &Expr, arms: &[ast::MatchArm]) -> HirExpr {
        let s = self.check_expr(scrut);
        let mut harms: Vec<HirArm> = Vec::new();
        let mut result: Option<HirExpr> = None;
        for arm in arms {
            self.push_scope(arm.span.end);
            let mut binds = Vec::new();
            let pat = self.check_pat(&arm.pat, &s.ty.clone(), &mut binds, None);
            for (_, def, _) in &binds {
                self.scope_insert(*def, arm.pat.span.end);
            }
            let guard = arm.guard.as_ref().map(|g| {
                let h = self.check_expr(g);
                self.demand(
                    h.span,
                    &Ty::Bool,
                    &h.ty.clone(),
                    Why::msg("match guards must be `bool`"),
                );
                h
            });
            let body = self.check_expr(&arm.body);
            self.pop_scope();
            match &result {
                None => {
                    result = Some(HirExpr {
                        id: body.id,
                        span: body.span,
                        ty: body.ty.clone(),
                        kind: HirExprKind::Error,
                    })
                }
                Some(prev) => {
                    let prev = prev.clone();
                    let ty = self.join(&prev, &body, "`match` arms have incompatible types");
                    result = Some(HirExpr { ty, ..prev });
                }
            }
            harms.push(HirArm {
                id: arm.id,
                span: arm.span,
                pat,
                guard,
                body,
            });
        }
        let ty = result.map(|r| r.ty).unwrap_or(Ty::Never);
        self.mk(e, ty, HirExprKind::Match(Box::new(s), harms))
    }

    fn check_unary(&mut self, e: &Expr, op: UnOp, a: &Expr) -> HirExpr {
        let h = self.check_expr(a);
        let want = if op == UnOp::Neg { Ty::Int } else { Ty::Bool };
        let t = self.shallow(&h.ty);
        let ty = match t {
            Ty::Error => Ty::Error,
            Ty::Never => want.clone(),
            Ty::Var(_) => {
                self.infer.unify(&t, &want).ok();
                want.clone()
            }
            ref t if *t == want => want.clone(),
            t => {
                let t = self.infer.zonk(&t);
                let sym = if op == UnOp::Neg { "-" } else { "!" };
                self.diags.push(
                    Diagnostic::error(
                        "E0305",
                        format!("cannot apply unary operator `{sym}` to type `{t}`"),
                    )
                    .primary(e.span, format!("`{sym}` needs `{want}`")),
                );
                Ty::Error
            }
        };
        self.mk(e, ty, HirExprKind::Unary(op, Box::new(h)))
    }

    /// Type of `l op r`, reporting E0304/E0301 when the operands do not fit.
    fn binary_ty(&mut self, e: &Expr, op: BinOp, l: &HirExpr, r: &HirExpr) -> Ty {
        let fallback = if op.is_comparison() || matches!(op, BinOp::And | BinOp::Or) {
            Ty::Bool
        } else {
            Ty::Error
        };
        let (mut lt, mut rt) = (self.shallow(&l.ty), self.shallow(&r.ty));
        if lt.is_error() || rt.is_error() {
            return fallback;
        }
        match (&lt, &rt) {
            (Ty::Never, Ty::Never) => return fallback,
            (Ty::Never, _) => lt = rt.clone(),
            (_, Ty::Never) => rt = lt.clone(),
            _ => {}
        }
        match (&lt, &rt) {
            (Ty::Var(_), Ty::Var(_)) => {
                self.diags.push(
                    Diagnostic::error("E0310", "type annotations needed")
                        .primary(e.span, "cannot infer the operand types"),
                );
                return fallback;
            }
            (Ty::Var(_), t) => {
                self.infer.unify(&lt, t).ok();
                lt = t.clone();
            }
            (t, Ty::Var(_)) => {
                self.infer.unify(&rt, t).ok();
                rt = t.clone();
            }
            _ => {}
        }
        let (lt, rt) = (self.infer.zonk(&lt), self.infer.zonk(&rt));
        let bad = |this: &mut Self, note: Option<&str>| {
            let mut d = Diagnostic::error(
                "E0304",
                format!("cannot apply `{}` to `{lt}` and `{rt}`", op.symbol()),
            )
            .primary(
                e.span,
                format!("no `{}` for `{lt}` and `{rt}`", op.symbol()),
            );
            if let Some(n) = note {
                d = d.help(n);
            }
            if lt == Ty::Str && matches!(rt, Ty::Int | Ty::Bool) && op == BinOp::Add {
                d = d.help("convert the right side with `to_str(..)` before concatenating");
            }
            this.diags.push(d);
            fallback.clone()
        };
        match op {
            BinOp::Add if lt == Ty::Int && rt == Ty::Int => Ty::Int,
            BinOp::Add if lt == Ty::Str && rt == Ty::Str => Ty::Str,
            BinOp::Add => bad(self, Some("`+` works on two `int`s or two `str`s")),
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                if lt == Ty::Int && rt == Ty::Int {
                    Ty::Int
                } else {
                    bad(self, Some("arithmetic works on `int`s"))
                }
            }
            BinOp::And | BinOp::Or => {
                if lt == Ty::Bool && rt == Ty::Bool {
                    Ty::Bool
                } else {
                    bad(self, Some("logical operators need `bool`s"))
                }
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if lt == rt && matches!(lt, Ty::Int | Ty::Str) {
                    Ty::Bool
                } else {
                    bad(self, Some("ordering works on two `int`s or two `str`s"))
                }
            }
            BinOp::Eq | BinOp::Ne => {
                if lt.has_fn() || rt.has_fn() {
                    return bad(self, Some("function values cannot be compared"));
                }
                self.demand(
                    r.span,
                    &l.ty.clone(),
                    &r.ty.clone(),
                    Why::origin(l.span, &format!("this is of type `{lt}`")),
                );
                Ty::Bool
            }
        }
    }

    // ------------------------------------------------------------ names

    fn unresolved(&mut self, id: &ast::Ident) {
        let n = id.name.as_str();
        let mut cands: Vec<&str> = self.visible_local_names();
        cands.extend(self.values.keys().map(|k| k.as_str()));
        cands.extend(Builtin::ALL.iter().map(|b| b.name()));
        let best = closest_match(n, cands);
        let mut d = Diagnostic::error("E0201", format!("cannot find value `{n}` in this scope"))
            .primary(id.span, "not found in this scope");
        // Variants must be qualified; point learners at `Enum::Variant`.
        let variant = self
            .prog
            .defs
            .iter()
            .find(|d| d.kind == DefKind::Variant && d.name == n && d.parent.is_some())
            .and_then(|d| d.parent)
            .map(|p| self.prog.defs[p.0 as usize].name.clone());
        if let Some(en) = variant {
            d = d.suggest(
                format!("enum variants are qualified: `{en}::{n}`"),
                id.span,
                format!("{en}::{n}"),
                Applicability::MaybeIncorrect,
            );
        } else if let Some(b) = best {
            d = d.suggest(
                format!("a local variable or function with a similar name exists: `{b}`"),
                id.span,
                b,
                Applicability::MaybeIncorrect,
            );
        }
        self.diags.push(d);
    }

    fn check_name(&mut self, e: &Expr, id: &ast::Ident) -> HirExpr {
        let n = id.name.as_str();
        if n.is_empty() {
            return self.err_expr(e);
        }
        if let Some(def) = self.lookup_local(n) {
            let l = self.locals.get_mut(&def);
            let ty = match l {
                Some(l) => {
                    l.used = true;
                    l.ty.clone()
                }
                None => Ty::Error,
            };
            self.index.refs.push((id.span, def));
            return self.mk(e, ty, HirExprKind::Local(def));
        }
        if let Some(&def) = self.values.get(n) {
            self.index.refs.push((id.span, def));
            self.note_use(def);
            return match self.prog.defs[def.0 as usize].kind {
                DefKind::Fn => {
                    let (targs, ps, r) = self.instantiate(def);
                    self.mk(e, Ty::Fn(ps, Box::new(r)), HirExprKind::Fn(def, targs))
                }
                _ => {
                    let ty = self.prog.defs[def.0 as usize]
                        .ty
                        .clone()
                        .unwrap_or(Ty::Error);
                    self.mk(e, ty, HirExprKind::Const(def))
                }
            };
        }
        if let Some(&def) = self.types.get(n) {
            self.index.refs.push((id.span, def));
            self.note_type_use(def);
            let (what, help) = if self.prog.defs[def.0 as usize].kind == DefKind::Struct {
                (
                    "struct",
                    format!("construct it with `{n} {{ field: value, .. }}`"),
                )
            } else {
                ("enum", format!("use a variant: `{n}::Variant`"))
            };
            self.diags.push(
                Diagnostic::error("E0217", format!("expected value, found {what} `{n}`"))
                    .primary(id.span, format!("`{n}` is a type, not a value"))
                    .help(help),
            );
            return self.err_expr(e);
        }
        if Builtin::from_name(n).is_some() {
            self.diags.push(
                Diagnostic::error(
                    "E0218",
                    format!("built-in function `{n}` cannot be used as a value"),
                )
                .primary(id.span, "only calls are allowed")
                .help("wrap it in your own function to pass it around"),
            );
            return self.err_expr(e);
        }
        self.unresolved(id);
        self.err_expr(e)
    }

    /// Fresh type arguments plus the substituted signature of a generic function.
    fn instantiate(&mut self, def: DefId) -> (Vec<Ty>, Vec<Ty>, Ty) {
        let Some(sig) = self.sigs.get(&def) else {
            return (vec![], vec![], Ty::Error);
        };
        let (generics, params, ret) = (sig.generics.clone(), sig.params.clone(), sig.ret.clone());
        let targs = self.fresh_args(&generics);
        let map: Vec<(Name, Ty)> = generics.into_iter().zip(targs.iter().cloned()).collect();
        (
            targs,
            params.iter().map(|p| p.subst(&map)).collect(),
            ret.subst(&map),
        )
    }

    /// Resolve `Enum::Variant`, reporting problems. Returns (enum def, variant index).
    pub(super) fn resolve_variant(&mut self, p: &ast::Path) -> Option<(DefId, usize)> {
        let (en, vn) = (&p.segments[0], p.segments.get(1)?);
        if en.name.is_empty() || vn.name.is_empty() {
            return None;
        }
        let Some(&edef) = self.types.get(en.name.as_str()) else {
            let cands: Vec<&str> = self.types.keys().map(|k| k.as_str()).collect();
            let mut d = Diagnostic::error(
                "E0202",
                format!("cannot find type `{}` in this scope", en.name),
            )
            .primary(en.span, "not found");
            if let Some(b) = closest_match(&en.name, cands) {
                d = d.suggest(
                    format!("a type with a similar name exists: `{b}`"),
                    en.span,
                    b,
                    Applicability::MaybeIncorrect,
                );
            }
            self.diags.push(d);
            return None;
        };
        self.index.refs.push((en.span, edef));
        self.note_type_use(edef);
        let Some(info) = self.prog.enum_info(edef) else {
            self.diags.push(
                Diagnostic::error("E0204", format!("`{}` is a struct, not an enum", en.name))
                    .primary(en.span, "only enums have variants"),
            );
            return None;
        };
        let names: Vec<&str> = info.variants.iter().map(|v| v.name.as_str()).collect();
        if let Some(i) = names.iter().position(|n| *n == vn.name) {
            let vdef = info.variants[i].def;
            self.index.refs.push((vn.span, vdef));
            return Some((edef, i));
        }
        let mut d = Diagnostic::error(
            "E0204",
            format!(
                "no variant named `{}` found for enum `{}`",
                vn.name, en.name
            ),
        )
        .primary(vn.span, "unknown variant");
        if let Some(b) = closest_match(&vn.name, names.iter().copied()) {
            d = d.suggest(
                format!("a variant with a similar name exists: `{b}`"),
                vn.span,
                b,
                Applicability::MaybeIncorrect,
            );
        }
        d = d.note(format!(
            "`{}` has the variants: {}",
            en.name,
            names.join(", ")
        ));
        self.diags.push(d);
        None
    }

    fn check_variant_value(&mut self, e: &Expr, p: &ast::Path) -> HirExpr {
        let Some((edef, vi)) = self.resolve_variant(p) else {
            return self.err_expr(e);
        };
        let info = self.prog.enum_info(edef).cloned();
        let Some(info) = info else {
            return self.err_expr(e);
        };
        let v = &info.variants[vi];
        if v.shape != VariantShape::Unit {
            let (kind, hint) = match v.shape {
                VariantShape::Tuple => {
                    ("tuple", format!("call it: `{}::{}(..)`", info.name, v.name))
                }
                _ => (
                    "struct",
                    format!("construct it: `{}::{} {{ .. }}`", info.name, v.name),
                ),
            };
            self.diags.push(
                Diagnostic::error(
                    "E0314",
                    format!(
                        "expected value, found {kind} variant `{}::{}`",
                        info.name, v.name
                    ),
                )
                .primary(e.span, "this variant has fields".to_string())
                .help(hint),
            );
            return self.err_expr(e);
        }
        let args = self.fresh_args(&info.generics);
        self.mk(
            e,
            Ty::Adt(edef, Name::from(info.name.as_str()), args),
            HirExprKind::Variant {
                enum_def: edef,
                variant: vi,
                fields: vec![],
            },
        )
    }

    // ------------------------------------------------------------ calls

    fn check_args(
        &mut self,
        call: &Expr,
        params: &[Ty],
        args: &[Expr],
        def_span: Option<Span>,
        what: &str,
    ) -> Vec<HirExpr> {
        let hs: Vec<HirExpr> = args.iter().map(|a| self.check_expr(a)).collect();
        if hs.len() != params.len() {
            let mut d = Diagnostic::error(
                "E0302",
                format!(
                    "this {what} takes {} but {} {} supplied",
                    plural(params.len(), "argument"),
                    hs.len(),
                    if hs.len() == 1 { "was" } else { "were" }
                ),
            )
            .primary(
                call.span,
                format!(
                    "expected {}, found {}",
                    plural(params.len(), "argument"),
                    hs.len()
                ),
            );
            if let Some(s) = def_span {
                d = d.secondary(s, format!("{what} defined here"));
            }
            self.diags.push(d);
            return hs;
        }
        for (h, p) in hs.iter().zip(params) {
            self.demand(h.span, p, &h.ty.clone(), Why::default());
        }
        hs
    }

    fn check_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> HirExpr {
        if let ExprKind::Path(p) = &peel(callee).kind {
            if p.segments.len() == 2 {
                return self.check_variant_ctor(e, p, args);
            }
            let n = p.segments[0].name.as_str();
            if self.lookup_local(n).is_none()
                && !self.values.contains_key(n)
                && let Some(b) = Builtin::from_name(n)
            {
                let refs: Vec<&Expr> = args.iter().collect();
                return self.check_builtin(e, b, &refs);
            }
        }
        let c = self.check_expr(callee);
        let def_span = match &c.kind {
            HirExprKind::Fn(d, _) => Some(self.prog.defs[d.0 as usize].span),
            _ => None,
        };
        match self.shallow(&c.ty) {
            Ty::Fn(ps, r) => {
                let hs = self.check_args(e, &ps, args, def_span, "function");
                self.mk(e, *r, HirExprKind::Call(Box::new(c), hs))
            }
            Ty::Error => {
                args.iter().for_each(|a| {
                    self.check_expr(a);
                });
                self.err_expr(e)
            }
            t => {
                args.iter().for_each(|a| {
                    self.check_expr(a);
                });
                let t = self.infer.zonk(&t);
                if matches!(t, Ty::Var(_)) {
                    return self.needs_annotation(e, "calling it");
                }
                self.diags.push(
                    Diagnostic::error("E0303", format!("expected function, found `{t}`"))
                        .primary(callee.span, "this is not a function"),
                );
                self.err_expr(e)
            }
        }
    }

    fn check_variant_ctor(&mut self, e: &Expr, p: &ast::Path, args: &[Expr]) -> HirExpr {
        let Some((edef, vi)) = self.resolve_variant(p) else {
            args.iter().for_each(|a| {
                self.check_expr(a);
            });
            return self.err_expr(e);
        };
        let Some(info) = self.prog.enum_info(edef).cloned() else {
            return self.err_expr(e);
        };
        let v = &info.variants[vi];
        if v.shape != VariantShape::Tuple {
            args.iter().for_each(|a| {
                self.check_expr(a);
            });
            let kind = if v.shape == VariantShape::Unit {
                "unit"
            } else {
                "struct"
            };
            let hint = if v.shape == VariantShape::Unit {
                format!("use `{}::{}` without parentheses", info.name, v.name)
            } else {
                format!("construct it: `{}::{} {{ .. }}`", info.name, v.name)
            };
            self.diags.push(
                Diagnostic::error(
                    "E0314",
                    format!(
                        "`{}::{}` is a {kind} variant and cannot be called",
                        info.name, v.name
                    ),
                )
                .primary(e.span, format!("{kind} variant"))
                .help(hint),
            );
            return self.err_expr(e);
        }
        let targs = self.fresh_args(&info.generics);
        let map: Vec<(Name, Ty)> = info
            .generics
            .iter()
            .cloned()
            .zip(targs.iter().cloned())
            .collect();
        let ptys: Vec<Ty> = v.fields.iter().map(|f| f.ty.subst(&map)).collect();
        let vspan = self.prog.defs[v.def.0 as usize].span;
        let hs = self.check_args(e, &ptys, args, Some(vspan), "variant");
        let fields = hs.into_iter().enumerate().collect();
        self.mk(
            e,
            Ty::Adt(edef, Name::from(info.name.as_str()), targs),
            HirExprKind::Variant {
                enum_def: edef,
                variant: vi,
                fields,
            },
        )
    }

    fn check_method(
        &mut self,
        e: &Expr,
        recv: &Expr,
        method: &ast::Ident,
        args: &[Expr],
    ) -> HirExpr {
        let b = match method.name.as_str() {
            "len" => Builtin::Len,
            "push" => Builtin::Push,
            "pop" => Builtin::Pop,
            "to_str" => Builtin::ToStr,
            "chars" => Builtin::Chars,
            "" => return self.err_expr(e),
            n => {
                self.check_expr(recv);
                args.iter().for_each(|a| {
                    self.check_expr(a);
                });
                let mut d = Diagnostic::error("E0308", format!("no method named `{n}` found"))
                    .primary(method.span, "unknown method")
                    .note("methods are sugar for built-ins: len, push, pop, to_str, chars");
                if let Some(best) = closest_match(n, ["len", "push", "pop", "to_str", "chars"]) {
                    d = d.suggest(
                        format!("a method with a similar name exists: `{best}`"),
                        method.span,
                        best,
                        Applicability::MaybeIncorrect,
                    );
                }
                self.diags.push(d);
                return self.err_expr(e);
            }
        };
        let mut all: Vec<&Expr> = vec![recv];
        all.extend(args.iter());
        self.check_builtin(e, b, &all)
    }

    fn is_place_syntax(e: &Expr) -> bool {
        match &peel(e).kind {
            ExprKind::Path(p) => p.segments.len() == 1,
            ExprKind::Field(b, _) | ExprKind::TupleField(b, ..) | ExprKind::Index(b, _) => {
                Self::is_place_syntax(b)
            }
            _ => false,
        }
    }

    fn check_builtin(&mut self, e: &Expr, b: Builtin, args: &[&Expr]) -> HirExpr {
        let (min, max) = match b {
            Builtin::Println => (0, 1),
            Builtin::Assert => (1, 2),
            Builtin::Push => (2, 2),
            _ => (1, 1),
        };
        if args.len() < min || args.len() > max {
            args.iter().for_each(|a| {
                self.check_expr(a);
            });
            let want = if min == max {
                plural(min, "argument")
            } else {
                format!("{min} or {max} arguments")
            };
            self.diags.push(
                Diagnostic::error(
                    "E0302",
                    format!(
                        "the built-in `{}` takes {want} but {} {} supplied",
                        b.name(),
                        args.len(),
                        if args.len() == 1 { "was" } else { "were" }
                    ),
                )
                .primary(e.span, format!("expected {want}")),
            );
            return self.err_expr(e);
        }
        let mutating = matches!(b, Builtin::Push | Builtin::Pop);
        let mut hs = Vec::new();
        if let Some(a0) = args.first() {
            let first = if mutating && Self::is_place_syntax(a0) {
                self.check_place(a0, true).map(|(h, _)| h)
            } else {
                Some(self.check_expr(a0))
            };
            hs.push(first.unwrap_or_else(|| self.err_expr(a0)));
        }
        for a in args.iter().skip(1) {
            hs.push(self.check_expr(a));
        }
        let ty = match b {
            Builtin::Print | Builtin::Println => {
                if let Some(h) = hs.first() {
                    match self.shallow(&h.ty) {
                        Ty::Int | Ty::Bool | Ty::Str | Ty::Unit | Ty::Error | Ty::Never => {}
                        Ty::Var(_) => {
                            self.needs_annotation(args[0], "printing");
                        }
                        t => {
                            let t = self.infer.zonk(&t);
                            self.diags.push(
                                Diagnostic::error(
                                    "E0324",
                                    format!("cannot print a value of type `{t}`"),
                                )
                                .primary(
                                    h.span,
                                    "only `int`, `bool`, `str` and `()` can be printed",
                                )
                                .help("convert with `to_str(..)` or print the parts"),
                            );
                        }
                    }
                }
                Ty::Unit
            }
            Builtin::Assert => {
                let c = hs[0].clone();
                self.demand(
                    c.span,
                    &Ty::Bool,
                    &c.ty,
                    Why::msg("`assert` needs a `bool` condition"),
                );
                if let Some(m) = hs.get(1) {
                    let m = m.clone();
                    self.demand(
                        m.span,
                        &Ty::Str,
                        &m.ty,
                        Why::msg("the `assert` message must be `str`"),
                    );
                }
                Ty::Unit
            }
            Builtin::Panic => {
                let m = hs[0].clone();
                self.demand(
                    m.span,
                    &Ty::Str,
                    &m.ty,
                    Why::msg("`panic` takes a `str` message"),
                );
                Ty::Never
            }
            Builtin::ToStr => match self.shallow(&hs[0].ty) {
                Ty::Int | Ty::Bool | Ty::Str | Ty::Error | Ty::Never => Ty::Str,
                t => {
                    let t = self.infer.zonk(&t);
                    self.diags.push(
                        Diagnostic::error(
                            "E0324",
                            format!("cannot convert a value of type `{t}` to `str`"),
                        )
                        .primary(hs[0].span, "`to_str` accepts `int`, `bool` and `str`"),
                    );
                    Ty::Str
                }
            },
            Builtin::Len => match self.shallow(&hs[0].ty) {
                Ty::Array(_) | Ty::Str | Ty::Error | Ty::Never => Ty::Int,
                t => {
                    let t = self.infer.zonk(&t);
                    self.diags.push(
                        Diagnostic::error("E0324", format!("cannot take the length of `{t}`"))
                            .primary(hs[0].span, "`len` accepts arrays and `str`"),
                    );
                    Ty::Int
                }
            },
            Builtin::Chars => {
                let s = hs[0].clone();
                self.demand(s.span, &Ty::Str, &s.ty, Why::msg("`chars` needs a `str`"));
                Ty::Array(Box::new(Ty::Str))
            }
            Builtin::Push | Builtin::Pop => {
                let elem = self.fresh();
                let want = Ty::Array(Box::new(elem.clone()));
                let a = hs[0].clone();
                match self.shallow(&a.ty) {
                    Ty::Array(_) | Ty::Var(_) | Ty::Error | Ty::Never => {
                        self.demand(a.span, &want, &a.ty, Why::default());
                    }
                    t => {
                        let t = self.infer.zonk(&t);
                        self.diags.push(
                            Diagnostic::error(
                                "E0324",
                                format!("`{}` needs an array, found `{t}`", b.name()),
                            )
                            .primary(a.span, "not an array"),
                        );
                    }
                }
                if b == Builtin::Push {
                    let v = hs[1].clone();
                    self.demand(
                        v.span,
                        &elem,
                        &v.ty,
                        Why::msg("pushed value does not match the array's element type"),
                    );
                    Ty::Unit
                } else {
                    elem
                }
            }
        };
        self.mk(e, ty, HirExprKind::Builtin(b, hs))
    }

    // ------------------------------------------------------------ places and assignment

    /// Check an assignable place (`x`, `p.f`, `t.0`, `a[i]`). Returns the typed place and
    /// its root local; reports E0317 for non-places and E0316 for immutable roots.
    fn check_place(&mut self, e: &Expr, mark_used: bool) -> Option<(HirExpr, DefId)> {
        let (h, root) = self.place_inner(e, mark_used)?;
        if let Some(l) = self.locals.get_mut(&root) {
            l.mutated = true;
            if !l.mutable {
                let (name, nspan, dspan, is_param) =
                    (l.name.clone(), l.name_span, l.decl_span, l.is_param);
                let direct = matches!(peel(e).kind, ExprKind::Path(_));
                let msg = if direct {
                    format!("cannot assign to `{name}`, as it is not declared as mutable")
                } else {
                    format!("cannot mutate through `{name}`, as it is not declared as mutable")
                };
                let what = if is_param { "parameter" } else { "binding" };
                self.diags.push(
                    Diagnostic::error("E0316", msg)
                        .primary(e.span, "cannot assign")
                        .secondary(nspan, format!("this {what} is not mutable"))
                        .suggest(
                            format!("consider declaring `{name}` as mutable: `mut {name}`"),
                            Span::point(dspan.start as usize),
                            "mut ",
                            Applicability::MachineApplicable,
                        ),
                );
            }
        }
        Some((h, root))
    }

    fn place_inner(&mut self, e: &Expr, mark_used: bool) -> Option<(HirExpr, DefId)> {
        let e = peel(e);
        match &e.kind {
            ExprKind::Path(p) if p.segments.len() == 1 => {
                let id = &p.segments[0];
                if let Some(def) = self.lookup_local(&id.name) {
                    let l = self.locals.get_mut(&def)?;
                    l.used |= mark_used;
                    let ty = l.ty.clone();
                    self.index.refs.push((id.span, def));
                    return Some((self.mk(e, ty, HirExprKind::Local(def)), def));
                }
                let h = self.check_expr(e);
                if !h.ty.is_error() {
                    self.invalid_target(e, "only variables can be assigned to");
                }
                None
            }
            ExprKind::Field(base, name) => {
                let (b, root) = self.place_inner(base, mark_used)?;
                let h = self.field_of(e, b, name);
                (!h.ty.is_error()).then_some((h, root))
            }
            ExprKind::TupleField(base, n, nspan) => {
                let (b, root) = self.place_inner(base, mark_used)?;
                match self.shallow(&b.ty) {
                    Ty::Tuple(ts) if (*n as usize) < ts.len() => {
                        let t = ts[*n as usize].clone();
                        Some((
                            self.mk(e, t, HirExprKind::TupleField(Box::new(b), *n as usize)),
                            root,
                        ))
                    }
                    Ty::Error => None,
                    t => {
                        let t = self.infer.zonk(&t);
                        self.diags.push(
                            Diagnostic::error(
                                "E0318",
                                format!("tuple index {n} is out of range for `{t}`"),
                            )
                            .primary(*nspan, "no such element"),
                        );
                        None
                    }
                }
            }
            ExprKind::Index(base, idx) => {
                let (b, root) = self.place_inner(base, mark_used)?;
                let i = self.check_expr(idx);
                self.demand(
                    i.span,
                    &Ty::Int,
                    &i.ty.clone(),
                    Why::msg("array indices must be `int`"),
                );
                match self.shallow(&b.ty) {
                    Ty::Array(t) => Some((
                        self.mk(e, *t, HirExprKind::Index(Box::new(b), Box::new(i))),
                        root,
                    )),
                    Ty::Error => None,
                    t => {
                        let t = self.infer.zonk(&t);
                        self.diags.push(
                            Diagnostic::error(
                                "E0307",
                                format!("cannot index into a value of type `{t}`"),
                            )
                            .primary(base.span, "not an array"),
                        );
                        None
                    }
                }
            }
            _ => {
                self.check_expr(e);
                self.invalid_target(e, "this expression cannot be assigned to");
                None
            }
        }
    }

    fn invalid_target(&mut self, e: &Expr, why: &str) {
        self.diags.push(
            Diagnostic::error("E0317", "invalid left-hand side of assignment").primary(e.span, why.to_string()).note("assignable places are variables, struct fields, tuple fields and array elements"),
        );
    }

    fn check_assign(
        &mut self,
        e: &Expr,
        op: Option<BinOp>,
        target: &Expr,
        value: &Expr,
    ) -> HirExpr {
        let hv = self.check_expr(value);
        let Some((ht, _)) = self.check_place(target, false) else {
            return self.err_expr(e);
        };
        match op {
            None => {
                self.demand(
                    value.span,
                    &ht.ty.clone(),
                    &hv.ty.clone(),
                    Why::origin(
                        target.span,
                        &format!(
                            "expected due to the type of this place: `{}`",
                            self.infer.zonk(&ht.ty)
                        ),
                    ),
                );
            }
            Some(op) => {
                let res = self.binary_ty(e, op, &ht, &hv);
                self.demand(e.span, &ht.ty.clone(), &res, Why::default());
            }
        }
        self.mk(
            e,
            Ty::Unit,
            HirExprKind::Assign {
                op,
                target: Box::new(ht),
                value: Box::new(hv),
            },
        )
    }

    // ------------------------------------------------------------ structs and fields

    fn check_field(&mut self, e: &Expr, base: &Expr, name: &ast::Ident) -> HirExpr {
        let b = self.check_expr(base);
        self.field_of(e, b, name)
    }

    fn field_of(&mut self, e: &Expr, b: HirExpr, name: &ast::Ident) -> HirExpr {
        if name.name.is_empty() {
            return self.err_expr(e);
        }
        match self.shallow(&b.ty) {
            Ty::Adt(def, tn, args) if self.prog.struct_info(def).is_some() => {
                let info = self.prog.struct_info(def).cloned();
                let Some(info) = info else {
                    return self.err_expr(e);
                };
                match info.fields.iter().position(|f| f.name == name.name) {
                    Some(i) => {
                        let map: Vec<(Name, Ty)> = info
                            .generics
                            .iter()
                            .cloned()
                            .zip(args.iter().cloned())
                            .collect();
                        self.index.refs.push((name.span, info.fields[i].def));
                        let ty = info.fields[i].ty.subst(&map);
                        self.mk(e, ty, HirExprKind::Field(Box::new(b), i))
                    }
                    None => {
                        let names: Vec<&str> =
                            info.fields.iter().map(|f| f.name.as_str()).collect();
                        let mut d = Diagnostic::error(
                            "E0203",
                            format!("no field `{}` on type `{tn}`", name.name),
                        )
                        .primary(name.span, "unknown field")
                        .secondary(
                            self.prog.defs[def.0 as usize].span,
                            format!("`{tn}` defined here"),
                        );
                        if let Some(best) = closest_match(&name.name, names.iter().copied()) {
                            d = d.suggest(
                                format!("a field with a similar name exists: `{best}`"),
                                name.span,
                                best,
                                Applicability::MaybeIncorrect,
                            );
                        } else if !names.is_empty() {
                            d = d.note(format!("available fields: {}", names.join(", ")));
                        }
                        self.diags.push(d);
                        self.err_expr(e)
                    }
                }
            }
            Ty::Error => self.err_expr(e),
            Ty::Var(_) => self.needs_annotation(e, "accessing a field"),
            t => {
                let t = self.infer.zonk(&t);
                self.diags.push(
                    Diagnostic::error("E0306", format!("type `{t}` has no fields")).primary(
                        name.span,
                        format!("`{}` is not a field of `{t}`", name.name),
                    ),
                );
                self.err_expr(e)
            }
        }
    }

    fn check_struct_lit(
        &mut self,
        e: &Expr,
        path: &ast::Path,
        inits: &[ast::FieldInit],
    ) -> HirExpr {
        // Resolve to (kind, owner display name, fields, generics, def of type, variant index)
        let resolved: Option<LitTarget> = if path.segments.len() == 2 {
            self.resolve_variant(path).and_then(|(ed, vi)| {
                let info = self.prog.enum_info(ed)?.clone();
                let v = &info.variants[vi];
                if v.shape != VariantShape::Struct {
                    let kind = if v.shape == VariantShape::Unit {
                        "unit"
                    } else {
                        "tuple"
                    };
                    self.diags.push(
                        Diagnostic::error(
                            "E0314",
                            format!(
                                "`{}::{}` is a {kind} variant, not a struct variant",
                                info.name, v.name
                            ),
                        )
                        .primary(path.span, format!("{kind} variant")),
                    );
                    return None;
                }
                Some((
                    format!("{}::{}", info.name, v.name),
                    v.fields.clone(),
                    info.generics.clone(),
                    ed,
                    Some(vi),
                ))
            })
        } else {
            let id = &path.segments[0];
            match self.types.get(id.name.as_str()).copied() {
                Some(def) => {
                    self.index.refs.push((id.span, def));
                    self.note_type_use(def);
                    match self.prog.struct_info(def).cloned() {
                        Some(s) => Some((
                            s.name.clone(),
                            s.fields.clone(),
                            s.generics.clone(),
                            def,
                            None,
                        )),
                        None => {
                            self.diags.push(
                                Diagnostic::error(
                                    "E0314",
                                    format!("`{}` is an enum, not a struct", id.name),
                                )
                                .primary(id.span, "enums are built from their variants")
                                .help(format!("use `{}::Variant`", id.name)),
                            );
                            None
                        }
                    }
                }
                None => {
                    if !id.name.is_empty() {
                        let cands: Vec<&str> = self.types.keys().map(|k| k.as_str()).collect();
                        let mut d = Diagnostic::error(
                            "E0202",
                            format!("cannot find struct `{}` in this scope", id.name),
                        )
                        .primary(id.span, "not found");
                        if let Some(b) = closest_match(&id.name, cands) {
                            d = d.suggest(
                                format!("a struct with a similar name exists: `{b}`"),
                                id.span,
                                b,
                                Applicability::MaybeIncorrect,
                            );
                        }
                        self.diags.push(d);
                    }
                    None
                }
            }
        };
        let Some((owner, fields, generics, def, variant)) = resolved else {
            inits.iter().for_each(|f| {
                self.check_expr(&f.value);
            });
            return self.err_expr(e);
        };
        let targs = self.fresh_args(&generics);
        let map: Vec<(Name, Ty)> = generics
            .iter()
            .cloned()
            .zip(targs.iter().cloned())
            .collect();
        let mut out: Vec<(usize, HirExpr)> = Vec::new();
        let mut seen: Vec<(usize, Span)> = Vec::new();
        for init in inits {
            let h = self.check_expr(&init.value);
            let Some(fi) = fields.iter().position(|f| f.name == init.name.name) else {
                if !init.name.name.is_empty() {
                    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                    let mut d = Diagnostic::error(
                        "E0203",
                        format!("`{owner}` has no field named `{}`", init.name.name),
                    )
                    .primary(init.name.span, "unknown field");
                    if let Some(best) = closest_match(&init.name.name, names.iter().copied()) {
                        d = d.suggest(
                            format!("a field with a similar name exists: `{best}`"),
                            init.name.span,
                            best,
                            Applicability::MaybeIncorrect,
                        );
                    } else {
                        d = d.note(format!("available fields: {}", names.join(", ")));
                    }
                    self.diags.push(d);
                }
                continue;
            };
            self.index.refs.push((init.name.span, fields[fi].def));
            if let Some((_, first)) = seen.iter().find(|(i, _)| *i == fi) {
                self.diags.push(
                    Diagnostic::error(
                        "E0313",
                        format!("field `{}` is specified more than once", init.name.name),
                    )
                    .primary(init.name.span, "used more than once")
                    .secondary(*first, "first use of the field"),
                );
                continue;
            }
            seen.push((fi, init.name.span));
            let fty = fields[fi].ty.subst(&map);
            self.demand(
                h.span,
                &fty,
                &h.ty.clone(),
                Why::origin(
                    self.prog.defs[fields[fi].def.0 as usize].span,
                    &format!("field `{}` is declared as `{fty}`", init.name.name),
                ),
            );
            out.push((fi, h));
        }
        let missing: Vec<&str> = fields
            .iter()
            .enumerate()
            .filter(|(i, _)| !seen.iter().any(|(s, _)| s == i))
            .map(|(_, f)| f.name.as_str())
            .collect();
        if !missing.is_empty() {
            let list = missing
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ");
            self.diags.push(
                Diagnostic::error(
                    "E0312",
                    format!(
                        "missing {} in initialiser of `{owner}`: {list}",
                        if missing.len() == 1 {
                            "field"
                        } else {
                            "fields"
                        }
                    ),
                )
                .primary(path.span, format!("missing {list}")),
            );
        }
        let name: Name = Name::from(owner.split("::").next().unwrap_or(&owner));
        let ty = Ty::Adt(def, name, targs);
        match variant {
            Some(vi) => self.mk(
                e,
                ty,
                HirExprKind::Variant {
                    enum_def: def,
                    variant: vi,
                    fields: out,
                },
            ),
            None => self.mk(e, ty, HirExprKind::Struct { def, fields: out }),
        }
    }
}

/// Replace any remaining inference variables by `Error` (after reporting them).
fn erase_vars(t: &Ty) -> Ty {
    match t {
        Ty::Var(_) => Ty::Error,
        _ => t.map_children(&mut erase_vars),
    }
}
