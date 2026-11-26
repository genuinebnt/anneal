//! Declaration collection and type lowering: the "name resolution" for items.
//!
//! Pass 1 gives every item a [`DefId`] and fills the type and value namespaces.
//! Pass 2 lowers declared types (`ast::Type` -> [`Ty`]) into signatures, struct
//! and enum layouts. Local names are resolved while checking bodies (`check.rs`),
//! which lets resolution and inference share one scope stack.

use super::check::{Checker, FnSig};
use super::hir::*;
use super::types::*;
use crate::diag::{Applicability, Diagnostic, closest_match};
use crate::source::Span;
use crate::syntax::ast::{self, ItemKind, TypeKind};

impl Checker<'_> {
    pub(super) fn new_def(
        &mut self,
        name: &str,
        kind: DefKind,
        span: Span,
        full_span: Span,
        parent: Option<DefId>,
        ty: Option<Ty>,
    ) -> DefId {
        let id = DefId(self.prog.defs.len() as u32);
        self.prog.defs.push(DefInfo {
            id,
            name: name.to_string(),
            kind,
            span,
            full_span,
            ty,
            parent,
            mutable: false,
        });
        id
    }

    /// Pass 1: create definitions and register names, reporting duplicates.
    pub(super) fn collect_items(&mut self) {
        for item in &self.m.items {
            let (name, kind, is_type) = match &item.kind {
                ItemKind::Fn(f) => (&f.name, DefKind::Fn, false),
                ItemKind::Struct(s) => (&s.name, DefKind::Struct, true),
                ItemKind::Enum(e) => (&e.name, DefKind::Enum, true),
                ItemKind::Const(c) => (&c.name, DefKind::Const, false),
                ItemKind::Error => {
                    self.has_item_errors = true;
                    self.item_defs.push(None);
                    continue;
                }
            };
            let def = self.new_def(&name.name, kind, name.span, item.span, None, None);
            self.item_defs.push(Some(def));
            if name.name.is_empty() {
                continue;
            }
            let map = if is_type {
                &mut self.types
            } else {
                &mut self.values
            };
            if let Some(&prev) = map.get(&name.name) {
                let prev_span = self.prog.defs[prev.0 as usize].span;
                let what = if is_type { "type" } else { "value" };
                self.diags.push(
                    Diagnostic::error(
                        "E0205",
                        format!("the {what} `{}` is defined multiple times", name.name),
                    )
                    .primary(name.span, format!("`{}` redefined here", name.name))
                    .secondary(prev_span, "previous definition here"),
                );
            } else {
                map.insert(name.name.clone(), def);
            }
            if let ItemKind::Struct(ast::StructDecl { generics, .. })
            | ItemKind::Enum(ast::EnumDecl { generics, .. }) = &item.kind
            {
                let g: Vec<Name> = generics
                    .iter()
                    .map(|i| Name::from(i.name.as_str()))
                    .collect();
                self.adt_generics.insert(def, g);
            }
        }
    }

    /// Check generic parameter lists for duplicates and create their definitions.
    fn declare_generics(&mut self, owner: DefId, generics: &[ast::Ident]) -> Vec<Name> {
        let mut out: Vec<Name> = Vec::new();
        for g in generics {
            if g.name.is_empty() {
                continue;
            }
            if let Some(prev) = generics
                .iter()
                .find(|o| o.name == g.name && o.span.start < g.span.start)
            {
                self.diags.push(
                    Diagnostic::error(
                        "E0207",
                        format!("the type parameter `{}` is declared twice", g.name),
                    )
                    .primary(g.span, "duplicate type parameter")
                    .secondary(prev.span, "first declared here"),
                );
                continue;
            }
            self.new_def(
                &g.name,
                DefKind::TypeParam,
                g.span,
                g.span,
                Some(owner),
                None,
            );
            out.push(Name::from(g.name.as_str()));
        }
        out
    }

    /// Pass 2: lower all declared types.
    pub(super) fn lower_signatures(&mut self) {
        for (ix, item) in self.m.items.iter().enumerate() {
            let Some(def) = self.item_defs[ix] else {
                continue;
            };
            self.cur_item = Some(def);
            match &item.kind {
                ItemKind::Fn(f) => self.lower_fn_sig(def, f),
                ItemKind::Struct(s) => self.lower_struct(def, s),
                ItemKind::Enum(e) => self.lower_enum(def, e),
                ItemKind::Const(c) => {
                    let ty = self.lower_type(&c.ty, &[]);
                    self.prog.defs[def.0 as usize].ty = Some(ty);
                }
                ItemKind::Error => {}
            }
        }
        self.cur_item = None;
        self.prog.rebuild_indexes();
    }

    fn lower_fn_sig(&mut self, def: DefId, f: &ast::FnDecl) {
        let generics = self.declare_generics(def, &f.generics);
        let mut params = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            params.push(self.lower_type(&p.ty, &generics));
            if let Some(prev) = f.params[..i]
                .iter()
                .find(|o| o.name.name == p.name.name && !p.name.name.is_empty())
            {
                self.diags.push(
                    Diagnostic::error(
                        "E0207",
                        format!("parameter `{}` is declared twice", p.name.name),
                    )
                    .primary(p.name.span, "duplicate parameter")
                    .secondary(prev.name.span, "first declared here"),
                );
            }
        }
        let ret = match &f.ret {
            Some(t) => self.lower_type(t, &generics),
            None => Ty::Unit,
        };
        let ty = Ty::Fn(params.clone(), Box::new(ret.clone()));
        self.prog.defs[def.0 as usize].ty = Some(ty);
        self.sigs.insert(
            def,
            FnSig {
                generics,
                params,
                ret,
                ret_span: f.ret.as_ref().map(|t| t.span),
            },
        );
    }

    fn lower_fields(
        &mut self,
        owner: DefId,
        fields: &[ast::FieldDecl],
        generics: &[Name],
        what: &str,
    ) -> Vec<FieldInfo> {
        let mut out: Vec<FieldInfo> = Vec::new();
        for f in fields {
            let ty = self.lower_type(&f.ty, generics);
            let def = self.new_def(
                &f.name.name,
                DefKind::Field,
                f.name.span,
                f.span,
                Some(owner),
                Some(ty.clone()),
            );
            if let Some(prev) = fields.iter().find(|o| {
                o.name.name == f.name.name && o.span.start < f.span.start && !f.name.name.is_empty()
            }) {
                self.diags.push(
                    Diagnostic::error(
                        "E0206",
                        format!("field `{}` is declared twice in {what}", f.name.name),
                    )
                    .primary(f.name.span, "duplicate field")
                    .secondary(prev.name.span, "first declared here"),
                );
                continue;
            }
            out.push(FieldInfo {
                def,
                name: f.name.name.clone(),
                ty,
            });
        }
        out
    }

    fn lower_struct(&mut self, def: DefId, s: &ast::StructDecl) {
        let generics = self.declare_generics(def, &s.generics);
        let args: Vec<Ty> = generics.iter().map(|g| Ty::Param(g.clone())).collect();
        self.prog.defs[def.0 as usize].ty =
            Some(Ty::Adt(def, Name::from(s.name.name.as_str()), args));
        let fields = self.lower_fields(
            def,
            &s.fields,
            &generics,
            &format!("struct `{}`", s.name.name),
        );
        self.prog.structs.push(StructInfo {
            def,
            name: s.name.name.clone(),
            generics,
            fields,
        });
    }

    fn lower_enum(&mut self, def: DefId, e: &ast::EnumDecl) {
        let generics = self.declare_generics(def, &e.generics);
        let args: Vec<Ty> = generics.iter().map(|g| Ty::Param(g.clone())).collect();
        self.prog.defs[def.0 as usize].ty =
            Some(Ty::Adt(def, Name::from(e.name.name.as_str()), args));
        let mut variants: Vec<VariantInfo> = Vec::new();
        for v in &e.variants {
            let vdef = self.new_def(
                &v.name.name,
                DefKind::Variant,
                v.name.span,
                v.span,
                Some(def),
                None,
            );
            let (shape, fields) = match &v.kind {
                ast::VariantKind::Unit => (VariantShape::Unit, vec![]),
                ast::VariantKind::Tuple(ts) => {
                    let mut fs = Vec::new();
                    for (i, t) in ts.iter().enumerate() {
                        let ty = self.lower_type(t, &generics);
                        let fd = self.new_def(
                            &i.to_string(),
                            DefKind::Field,
                            t.span,
                            t.span,
                            Some(vdef),
                            Some(ty.clone()),
                        );
                        fs.push(FieldInfo {
                            def: fd,
                            name: i.to_string(),
                            ty,
                        });
                    }
                    (VariantShape::Tuple, fs)
                }
                ast::VariantKind::Struct(fs) => (
                    VariantShape::Struct,
                    self.lower_fields(vdef, fs, &generics, &format!("variant `{}`", v.name.name)),
                ),
            };
            if let Some(prev) = variants
                .iter()
                .find(|o| o.name == v.name.name && !v.name.name.is_empty())
            {
                let prev_span = self.prog.defs[prev.def.0 as usize].span;
                self.diags.push(
                    Diagnostic::error(
                        "E0206",
                        format!(
                            "variant `{}` is declared twice in enum `{}`",
                            v.name.name, e.name.name
                        ),
                    )
                    .primary(v.name.span, "duplicate variant")
                    .secondary(prev_span, "first declared here"),
                );
                continue;
            }
            variants.push(VariantInfo {
                def: vdef,
                name: v.name.name.clone(),
                shape,
                fields,
            });
        }
        self.prog.enums.push(EnumInfo {
            def,
            name: e.name.name.clone(),
            generics,
            variants,
        });
    }

    /// Lower a syntactic type; unknown names are reported and become `Ty::Error`.
    pub(super) fn lower_type(&mut self, t: &ast::Type, generics: &[Name]) -> Ty {
        match &t.kind {
            TypeKind::Error => Ty::Error,
            TypeKind::Never => Ty::Never,
            TypeKind::Array(e) => Ty::Array(Box::new(self.lower_type(e, generics))),
            TypeKind::Tuple(ts) if ts.is_empty() => Ty::Unit,
            TypeKind::Tuple(ts) => {
                Ty::Tuple(ts.iter().map(|x| self.lower_type(x, generics)).collect())
            }
            TypeKind::Fn { params, ret } => {
                let ps = params
                    .iter()
                    .map(|x| self.lower_type(x, generics))
                    .collect();
                let r = ret
                    .as_ref()
                    .map(|r| self.lower_type(r, generics))
                    .unwrap_or(Ty::Unit);
                Ty::Fn(ps, Box::new(r))
            }
            TypeKind::Path { name, args } => self.lower_type_path(t, name, args, generics),
        }
    }

    fn lower_type_path(
        &mut self,
        t: &ast::Type,
        name: &ast::Ident,
        args: &[ast::Type],
        generics: &[Name],
    ) -> Ty {
        let n = name.name.as_str();
        if n.is_empty() {
            return Ty::Error;
        }
        let largs: Vec<Ty> = args.iter().map(|a| self.lower_type(a, generics)).collect();
        let arity_err = |this: &mut Self, expected: usize| {
            this.diags.push(
                Diagnostic::error(
                    "E0210",
                    format!(
                        "`{n}` takes {expected} type argument{} but {} {} supplied",
                        if expected == 1 { "" } else { "s" },
                        largs.len(),
                        if largs.len() == 1 { "was" } else { "were" }
                    ),
                )
                .primary(
                    t.span,
                    format!(
                        "expected {expected} type argument{}",
                        if expected == 1 { "" } else { "s" }
                    ),
                ),
            );
        };
        let builtin = match n {
            "int" => Some(Ty::Int),
            "bool" => Some(Ty::Bool),
            "str" => Some(Ty::Str),
            _ => None,
        };
        if let Some(b) = builtin {
            if !largs.is_empty() {
                arity_err(self, 0);
                return Ty::Error;
            }
            return b;
        }
        if let Some(g) = generics.iter().find(|g| &***g == n) {
            if !largs.is_empty() {
                arity_err(self, 0);
                return Ty::Error;
            }
            // Record the reference to the type parameter.
            if let Some(owner) = self.cur_item
                && let Some(d) = self.prog.defs.iter().rev().find(|d| {
                    d.kind == DefKind::TypeParam && d.parent == Some(owner) && d.name == n
                })
            {
                let id = d.id;
                self.index.refs.push((name.span, id));
            }
            return Ty::Param(g.clone());
        }
        if let Some(&def) = self.types.get(n) {
            self.note_type_use(def);
            self.index.refs.push((name.span, def));
            let expected = self.adt_generics.get(&def).map(|g| g.len()).unwrap_or(0);
            if expected != largs.len() {
                arity_err(self, expected);
                return Ty::Error;
            }
            return Ty::Adt(def, Name::from(n), largs);
        }
        let candidates: Vec<&str> = ["int", "bool", "str"]
            .into_iter()
            .chain(generics.iter().map(|g| &**g))
            .chain(self.types.keys().map(|k| k.as_str()))
            .collect();
        let mut d = Diagnostic::error("E0202", format!("cannot find type `{n}` in this scope"))
            .primary(name.span, "not found");
        if let Some(best) = closest_match(n, candidates) {
            d = d.suggest(
                format!("a type with a similar name exists: `{best}`"),
                name.span,
                best,
                Applicability::MaybeIncorrect,
            );
        } else if n == "unit" {
            d = d.help("the unit type is written `()`");
        } else if n == "string" || n == "String" {
            d = d.help("the string type is called `str`");
        } else if n == "i64"
            || n == "i32"
            || n == "u32"
            || n == "u64"
            || n == "usize"
            || n == "integer"
        {
            d = d.help("the integer type is called `int` (64-bit signed)");
        }
        self.diags.push(d);
        Ty::Error
    }

    /// Record that a type is mentioned somewhere other than its own declaration.
    pub(super) fn note_type_use(&mut self, def: DefId) {
        if self.cur_item != Some(def) {
            self.used_types.insert(def);
        }
    }
}
