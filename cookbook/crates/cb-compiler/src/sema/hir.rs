//! The typed, name-resolved program ("HIR"). Every expression carries a fully
//! resolved [`Ty`]; every name is a [`DefId`]; method sugar and `Paren` nodes are
//! gone. This is the input of the interpreter and (in stage 2) of MIR lowering.
//!
//! Invariant for a program without errors: no `Ty::Var`, `Ty::Error` or
//! `HirExprKind::Error` appears anywhere.

use super::types::{DefId, Name, Ty};
use crate::source::Span;
use crate::syntax::ast::{BinOp, Lit, NodeId, UnOp};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DefKind {
    Fn,
    Struct,
    Enum,
    Variant,
    Field,
    Const,
    Local,
    Param,
    TypeParam,
}

#[derive(Clone, Debug)]
pub struct DefInfo {
    pub id: DefId,
    pub name: String,
    pub kind: DefKind,
    /// Span of the defining name (what "go to definition" jumps to).
    pub span: Span,
    /// Span of the whole definition (for outlines and folding).
    pub full_span: Span,
    /// Fn: its `Ty::Fn`; Local/Param/Const/Field: its type; Struct/Enum: the self type.
    pub ty: Option<Ty>,
    pub parent: Option<DefId>,
    pub mutable: bool,
}

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub def: DefId,
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug)]
pub struct StructInfo {
    pub def: DefId,
    pub name: String,
    pub generics: Vec<Name>,
    pub fields: Vec<FieldInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantShape {
    Unit,
    Tuple,
    Struct,
}

#[derive(Clone, Debug)]
pub struct VariantInfo {
    pub def: DefId,
    pub name: String,
    pub shape: VariantShape,
    pub fields: Vec<FieldInfo>,
}

#[derive(Clone, Debug)]
pub struct EnumInfo {
    pub def: DefId,
    pub name: String,
    pub generics: Vec<Name>,
    pub variants: Vec<VariantInfo>,
}

#[derive(Clone, Debug)]
pub struct HirFn {
    pub def: DefId,
    pub name: String,
    pub generics: Vec<Name>,
    /// Parameter locals, in order.
    pub params: Vec<DefId>,
    pub ret: Ty,
    pub body: HirExpr,
    pub span: Span,
    /// All locals of this function (params first) have ids `locals_start..locals_start + locals_len`.
    pub locals_start: u32,
    pub locals_len: u32,
}

#[derive(Clone, Debug)]
pub struct HirConst {
    pub def: DefId,
    pub name: String,
    pub ty: Ty,
    pub body: HirExpr,
    pub span: Span,
    pub locals_start: u32,
    pub locals_len: u32,
}

/// The whole checked program.
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub defs: Vec<DefInfo>,
    pub structs: Vec<StructInfo>,
    pub enums: Vec<EnumInfo>,
    pub fns: Vec<HirFn>,
    pub consts: Vec<HirConst>,
    pub main: Option<DefId>,
    pub(crate) struct_ix: HashMap<DefId, usize>,
    pub(crate) enum_ix: HashMap<DefId, usize>,
    pub(crate) fn_ix: HashMap<DefId, usize>,
    pub(crate) const_ix: HashMap<DefId, usize>,
}

impl Program {
    pub fn def(&self, id: DefId) -> &DefInfo {
        &self.defs[id.0 as usize]
    }
    pub fn struct_info(&self, id: DefId) -> Option<&StructInfo> {
        self.struct_ix.get(&id).map(|&i| &self.structs[i])
    }
    pub fn enum_info(&self, id: DefId) -> Option<&EnumInfo> {
        self.enum_ix.get(&id).map(|&i| &self.enums[i])
    }
    pub fn fn_by_def(&self, id: DefId) -> Option<&HirFn> {
        self.fn_ix.get(&id).map(|&i| &self.fns[i])
    }
    pub fn const_by_def(&self, id: DefId) -> Option<&HirConst> {
        self.const_ix.get(&id).map(|&i| &self.consts[i])
    }
    pub(crate) fn rebuild_indexes(&mut self) {
        self.struct_ix = self
            .structs
            .iter()
            .enumerate()
            .map(|(i, s)| (s.def, i))
            .collect();
        self.enum_ix = self
            .enums
            .iter()
            .enumerate()
            .map(|(i, s)| (s.def, i))
            .collect();
        self.fn_ix = self
            .fns
            .iter()
            .enumerate()
            .map(|(i, s)| (s.def, i))
            .collect();
        self.const_ix = self
            .consts
            .iter()
            .enumerate()
            .map(|(i, s)| (s.def, i))
            .collect();
    }
    /// Field types of an ADT value with the given type arguments substituted.
    pub fn field_types(&self, def: DefId, variant: Option<usize>, args: &[Ty]) -> Vec<Ty> {
        let (generics, fields): (&[Name], &[FieldInfo]) = match variant {
            Some(v) => match self.enum_info(def) {
                Some(e) => (&e.generics, &e.variants[v].fields),
                None => return vec![],
            },
            None => match self.struct_info(def) {
                Some(s) => (&s.generics, &s.fields),
                None => return vec![],
            },
        };
        let map: Vec<(Name, Ty)> = generics.iter().cloned().zip(args.iter().cloned()).collect();
        fields.iter().map(|f| f.ty.subst(&map)).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    Print,
    Println,
    Assert,
    Panic,
    ToStr,
    Len,
    Push,
    Pop,
    Chars,
}

impl Builtin {
    pub fn from_name(n: &str) -> Option<Builtin> {
        Some(match n {
            "print" => Builtin::Print,
            "println" => Builtin::Println,
            "assert" => Builtin::Assert,
            "panic" => Builtin::Panic,
            "to_str" => Builtin::ToStr,
            "len" => Builtin::Len,
            "push" => Builtin::Push,
            "pop" => Builtin::Pop,
            "chars" => Builtin::Chars,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Println => "println",
            Builtin::Assert => "assert",
            Builtin::Panic => "panic",
            Builtin::ToStr => "to_str",
            Builtin::Len => "len",
            Builtin::Push => "push",
            Builtin::Pop => "pop",
            Builtin::Chars => "chars",
        }
    }
    pub const ALL: [Builtin; 9] = [
        Builtin::Print,
        Builtin::Println,
        Builtin::Assert,
        Builtin::Panic,
        Builtin::ToStr,
        Builtin::Len,
        Builtin::Push,
        Builtin::Pop,
        Builtin::Chars,
    ];
}

#[derive(Clone, Debug)]
pub struct HirExpr {
    pub id: NodeId,
    pub span: Span,
    pub ty: Ty,
    pub kind: HirExprKind,
}

#[derive(Clone, Debug)]
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub tail: Option<Box<HirExpr>>,
}

#[derive(Clone, Debug)]
pub enum HirStmt {
    Let { pat: HirPat, init: HirExpr },
    Expr(HirExpr),
}

#[derive(Clone, Debug)]
pub struct HirArm {
    pub id: NodeId,
    pub span: Span,
    pub pat: HirPat,
    pub guard: Option<HirExpr>,
    pub body: HirExpr,
}

#[derive(Clone, Debug)]
pub enum HirExprKind {
    Lit(Lit),
    Local(DefId),
    /// A function used as a value or callee, with its (inferred) type arguments.
    Fn(DefId, Vec<Ty>),
    Const(DefId),
    Unary(UnOp, Box<HirExpr>),
    Binary(BinOp, Box<HirExpr>, Box<HirExpr>),
    Assign {
        op: Option<BinOp>,
        target: Box<HirExpr>,
        value: Box<HirExpr>,
    },
    Call(Box<HirExpr>, Vec<HirExpr>),
    Builtin(Builtin, Vec<HirExpr>),
    /// Struct field by declaration index.
    Field(Box<HirExpr>, usize),
    TupleField(Box<HirExpr>, usize),
    Index(Box<HirExpr>, Box<HirExpr>),
    /// Fields as (declaration index, value) in *source* evaluation order.
    Struct {
        def: DefId,
        fields: Vec<(usize, HirExpr)>,
    },
    Variant {
        enum_def: DefId,
        variant: usize,
        fields: Vec<(usize, HirExpr)>,
    },
    Tuple(Vec<HirExpr>),
    Array(Vec<HirExpr>),
    If(Box<HirExpr>, Box<HirExpr>, Option<Box<HirExpr>>),
    Match(Box<HirExpr>, Vec<HirArm>),
    Block(HirBlock),
    While(Box<HirExpr>, Box<HirExpr>),
    Loop(Box<HirExpr>),
    Break(Option<Box<HirExpr>>),
    Continue,
    Return(Option<Box<HirExpr>>),
    /// `bool as int`.
    Cast(Box<HirExpr>),
    Error,
}

#[derive(Clone, Debug)]
pub struct HirPat {
    pub id: NodeId,
    pub span: Span,
    pub ty: Ty,
    pub kind: HirPatKind,
}

#[derive(Clone, Debug)]
pub enum HirPatKind {
    Wild,
    Bind(DefId, bool),
    Lit(Lit),
    Tuple(Vec<HirPat>),
    /// All fields in declaration order (omitted ones are `Wild`).
    Struct(DefId, Vec<HirPat>),
    Variant {
        enum_def: DefId,
        variant: usize,
        fields: Vec<HirPat>,
    },
    Or(Vec<HirPat>),
    Error,
}

/// Visit every expression and pattern below (and including) `e`, parents first.
pub fn walk_expr(
    e: &mut HirExpr,
    fe: &mut dyn FnMut(&mut HirExpr),
    fp: &mut dyn FnMut(&mut HirPat),
) {
    fe(e);
    match &mut e.kind {
        HirExprKind::Lit(_)
        | HirExprKind::Local(_)
        | HirExprKind::Fn(..)
        | HirExprKind::Const(_)
        | HirExprKind::Continue
        | HirExprKind::Error => {}
        HirExprKind::Unary(_, a)
        | HirExprKind::Field(a, _)
        | HirExprKind::TupleField(a, _)
        | HirExprKind::Cast(a)
        | HirExprKind::Loop(a) => walk_expr(a, fe, fp),
        HirExprKind::Binary(_, a, b) | HirExprKind::Index(a, b) | HirExprKind::While(a, b) => {
            walk_expr(a, fe, fp);
            walk_expr(b, fe, fp);
        }
        HirExprKind::Assign { target, value, .. } => {
            walk_expr(target, fe, fp);
            walk_expr(value, fe, fp);
        }
        HirExprKind::Call(c, args) => {
            walk_expr(c, fe, fp);
            args.iter_mut().for_each(|a| walk_expr(a, fe, fp));
        }
        HirExprKind::Builtin(_, xs) | HirExprKind::Tuple(xs) | HirExprKind::Array(xs) => {
            xs.iter_mut().for_each(|a| walk_expr(a, fe, fp))
        }
        HirExprKind::Struct { fields, .. } | HirExprKind::Variant { fields, .. } => {
            fields.iter_mut().for_each(|(_, a)| walk_expr(a, fe, fp))
        }
        HirExprKind::If(c, t, el) => {
            walk_expr(c, fe, fp);
            walk_expr(t, fe, fp);
            if let Some(el) = el {
                walk_expr(el, fe, fp);
            }
        }
        HirExprKind::Match(s, arms) => {
            walk_expr(s, fe, fp);
            for arm in arms {
                walk_pat(&mut arm.pat, fp);
                if let Some(g) = &mut arm.guard {
                    walk_expr(g, fe, fp);
                }
                walk_expr(&mut arm.body, fe, fp);
            }
        }
        HirExprKind::Block(b) => {
            for s in &mut b.stmts {
                match s {
                    HirStmt::Let { pat, init } => {
                        walk_pat(pat, fp);
                        walk_expr(init, fe, fp);
                    }
                    HirStmt::Expr(x) => walk_expr(x, fe, fp),
                }
            }
            if let Some(t) = &mut b.tail {
                walk_expr(t, fe, fp);
            }
        }
        HirExprKind::Break(x) | HirExprKind::Return(x) => {
            if let Some(x) = x {
                walk_expr(x, fe, fp);
            }
        }
    }
}

pub fn walk_pat(p: &mut HirPat, fp: &mut dyn FnMut(&mut HirPat)) {
    fp(p);
    match &mut p.kind {
        HirPatKind::Tuple(ps)
        | HirPatKind::Or(ps)
        | HirPatKind::Struct(_, ps)
        | HirPatKind::Variant { fields: ps, .. } => ps.iter_mut().for_each(|q| walk_pat(q, fp)),
        _ => {}
    }
}
