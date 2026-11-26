//! Checking patterns against the type they match (`impl Checker`, continued).
//! Bindings become new local definitions; or-patterns reuse the first
//! alternative's definitions so every alternative binds the *same* variables.

use super::check::{Checker, Why};
use super::hir::*;
use super::types::*;
use crate::diag::{Applicability, Diagnostic, closest_match};
use crate::source::Span;
use crate::syntax::ast::{self, Lit, PatFields, PatKind};

pub(super) type Binds = Vec<(String, DefId, Span)>;

impl Checker<'_> {
    fn hp(&self, p: &ast::Pattern, ty: &Ty, kind: HirPatKind) -> HirPat {
        HirPat {
            id: p.id,
            span: p.span,
            ty: ty.clone(),
            kind,
        }
    }

    pub(super) fn check_pat(
        &mut self,
        p: &ast::Pattern,
        expected: &Ty,
        binds: &mut Binds,
        reuse: Option<&Binds>,
    ) -> HirPat {
        match &p.kind {
            PatKind::Error => self.hp(p, &Ty::Error, HirPatKind::Error),
            PatKind::Wild => self.hp(p, expected, HirPatKind::Wild),
            PatKind::Binding { mutable, name } => {
                self.check_binding(p, *mutable, name, expected, binds, reuse)
            }
            PatKind::Lit(l) => {
                let lt = match l {
                    Lit::Int(_) => Ty::Int,
                    Lit::Bool(_) => Ty::Bool,
                    Lit::Str(_) => Ty::Str,
                    Lit::Unit => Ty::Unit,
                };
                self.demand(
                    p.span,
                    expected,
                    &lt,
                    Why::msg("mismatched types in pattern"),
                );
                self.hp(p, expected, HirPatKind::Lit(l.clone()))
            }
            PatKind::Tuple(ps) => {
                if let Ty::Tuple(ts) = self.shallow(expected)
                    && ts.len() != ps.len()
                {
                    self.diags.push(
                        Diagnostic::error(
                            "E0322",
                            format!(
                                "this pattern has {} elements, but the tuple has {}",
                                ps.len(),
                                ts.len()
                            ),
                        )
                        .primary(p.span, format!("expected {} elements", ts.len()))
                        .note(format!("the matched type is `{}`", Ty::Tuple(ts.clone()))),
                    );
                    return self.hp(p, &Ty::Error, HirPatKind::Error);
                }
                let elems: Vec<Ty> = ps.iter().map(|_| self.fresh()).collect();
                self.demand(
                    p.span,
                    expected,
                    &Ty::Tuple(elems.clone()),
                    Why::msg("mismatched types in pattern"),
                );
                let hs = ps
                    .iter()
                    .zip(&elems)
                    .map(|(q, t)| self.check_pat(q, t, binds, reuse))
                    .collect();
                self.hp(p, expected, HirPatKind::Tuple(hs))
            }
            PatKind::Or(alts) => {
                let mut first: Binds = Vec::new();
                let mut hs = vec![self.check_pat(&alts[0], expected, &mut first, reuse)];
                for alt in &alts[1..] {
                    let mut b: Binds = Vec::new();
                    hs.push(self.check_pat(alt, expected, &mut b, Some(&first)));
                    self.compare_alt_binds(alt, &first, &b);
                }
                binds.extend(first);
                self.hp(p, expected, HirPatKind::Or(hs))
            }
            PatKind::Adt { path, fields } => {
                self.check_adt_pat(p, path, fields, expected, binds, reuse)
            }
        }
    }

    fn compare_alt_binds(&mut self, alt: &ast::Pattern, first: &Binds, this: &Binds) {
        for (n, _, span) in first {
            if !this.iter().any(|(m, ..)| m == n) {
                self.diags.push(
                    Diagnostic::error(
                        "E0213",
                        format!("variable `{n}` is not bound in all alternatives"),
                    )
                    .primary(alt.span, format!("this alternative does not bind `{n}`"))
                    .secondary(*span, "bound in the first alternative"),
                );
            }
        }
        for (n, _, span) in this {
            if !first.iter().any(|(m, ..)| m == n) {
                self.diags.push(
                    Diagnostic::error(
                        "E0213",
                        format!("variable `{n}` is not bound in all alternatives"),
                    )
                    .primary(*span, format!("`{n}` is missing in the first alternative"))
                    .note("every alternative of an or-pattern must bind the same names"),
                );
            }
        }
    }

    fn check_binding(
        &mut self,
        p: &ast::Pattern,
        mutable: bool,
        name: &ast::Ident,
        expected: &Ty,
        binds: &mut Binds,
        reuse: Option<&Binds>,
    ) -> HirPat {
        if name.name.is_empty() {
            return self.hp(p, &Ty::Error, HirPatKind::Error);
        }
        if let Some((_, _, prev)) = binds.iter().find(|(n, ..)| *n == name.name) {
            self.diags.push(
                Diagnostic::error(
                    "E0212",
                    format!(
                        "identifier `{}` is bound more than once in the same pattern",
                        name.name
                    ),
                )
                .primary(name.span, "used as a binding twice")
                .secondary(*prev, "first binding"),
            );
            return self.hp(p, expected, HirPatKind::Wild);
        }
        // A capitalised name that is a variant of some enum is almost certainly a missing `Enum::`.
        if name.name.chars().next().is_some_and(|c| c.is_uppercase())
            && let Some(v) = self
                .prog
                .defs
                .iter()
                .find(|d| d.kind == DefKind::Variant && d.name == name.name)
            && let Some(parent) = v.parent
        {
            let en = self.prog.defs[parent.0 as usize].name.clone();
            self.diags.push(
                Diagnostic::warning(
                    "W0514",
                    format!(
                        "this pattern binds a new variable named `{}`, it does not match a variant",
                        name.name
                    ),
                )
                .primary(name.span, "this is a binding")
                .suggest(
                    format!("to match the variant, qualify it: `{en}::{}`", name.name),
                    name.span,
                    format!("{en}::{}", name.name),
                    Applicability::MaybeIncorrect,
                ),
            );
        }
        let def = match reuse.and_then(|r| r.iter().find(|(n, ..)| *n == name.name)) {
            Some((_, def, _)) => {
                let (def, lty) = (
                    *def,
                    self.locals
                        .get(def)
                        .map(|l| l.ty.clone())
                        .unwrap_or(Ty::Error),
                );
                self.demand(
                    p.span,
                    &lty,
                    expected,
                    Why::msg("mismatched types in or-pattern alternative"),
                );
                def
            }
            None => self.new_local(
                &name.name,
                name.span,
                p.span,
                mutable,
                expected.clone(),
                false,
            ),
        };
        binds.push((name.name.clone(), def, name.span));
        self.hp(p, expected, HirPatKind::Bind(def, mutable))
    }

    fn check_adt_pat(
        &mut self,
        p: &ast::Pattern,
        path: &ast::Path,
        fields: &PatFields,
        expected: &Ty,
        binds: &mut Binds,
        reuse: Option<&Binds>,
    ) -> HirPat {
        let err = |this: &Self| this.hp(p, &Ty::Error, HirPatKind::Error);
        // Resolve the path to a struct or an enum variant.
        let (def, variant, name, generics, field_infos, shape) = if path.segments.len() == 2 {
            let Some((ed, vi)) = self.resolve_variant(path) else {
                self.check_subpatterns_for_errors(fields, binds, reuse);
                return err(self);
            };
            let Some(info) = self.prog.enum_info(ed).cloned() else {
                return err(self);
            };
            let v = &info.variants[vi];
            (
                ed,
                Some(vi),
                format!("{}::{}", info.name, v.name),
                info.generics.clone(),
                v.fields.clone(),
                v.shape,
            )
        } else {
            let id = &path.segments[0];
            match self
                .types
                .get(id.name.as_str())
                .copied()
                .and_then(|d| self.prog.struct_info(d).cloned())
            {
                Some(s) => {
                    self.index.refs.push((id.span, s.def));
                    self.note_type_use(s.def);
                    (
                        s.def,
                        None,
                        s.name.clone(),
                        s.generics.clone(),
                        s.fields.clone(),
                        VariantShape::Struct,
                    )
                }
                None => {
                    self.check_subpatterns_for_errors(fields, binds, reuse);
                    let n = id.name.as_str();
                    let owner = self
                        .prog
                        .defs
                        .iter()
                        .find(|d| d.kind == DefKind::Variant && d.name == n)
                        .and_then(|d| d.parent)
                        .map(|d| self.prog.defs[d.0 as usize].name.clone());
                    let mut d = Diagnostic::error(
                        "E0204",
                        format!("cannot find struct or variant `{n}` in this scope"),
                    )
                    .primary(id.span, "not found");
                    if let Some(en) = owner {
                        d = d.suggest(
                            format!("enum variants are qualified: `{en}::{n}`"),
                            id.span,
                            format!("{en}::{n}"),
                            Applicability::MaybeIncorrect,
                        );
                    } else if let Some(b) = closest_match(n, self.types.keys().map(|k| k.as_str()))
                    {
                        d = d.suggest(
                            format!("a type with a similar name exists: `{b}`"),
                            id.span,
                            b,
                            Applicability::MaybeIncorrect,
                        );
                    }
                    self.diags.push(d);
                    return err(self);
                }
            }
        };
        let targs = self.fresh_args(&generics);
        let self_ty = Ty::Adt(
            def,
            Name::from(name.split("::").next().unwrap_or(&name)),
            targs.clone(),
        );
        self.demand(
            p.span,
            expected,
            &self_ty,
            Why::msg("mismatched types in pattern"),
        );
        let map: Vec<(Name, Ty)> = generics
            .iter()
            .cloned()
            .zip(targs.iter().cloned())
            .collect();
        let mut subs: Vec<HirPat> = Vec::new();
        let wild = |_this: &Self, ty: Ty| HirPat {
            id: p.id,
            span: p.span,
            ty,
            kind: HirPatKind::Wild,
        };
        match fields {
            PatFields::Unit => {
                if shape != VariantShape::Unit {
                    self.diags.push(
                        Diagnostic::error(
                            "E0314",
                            format!("`{name}` has fields, so the pattern must list them"),
                        )
                        .primary(p.span, "missing fields")
                        .help(if shape == VariantShape::Tuple {
                            format!("use `{name}(..)`")
                        } else {
                            format!("use `{name} {{ .. }}`")
                        }),
                    );
                    return err(self);
                }
            }
            PatFields::Tuple(ps) => {
                if shape != VariantShape::Tuple {
                    self.check_subpatterns_for_errors(fields, binds, reuse);
                    self.diags.push(
                        Diagnostic::error("E0314", format!("`{name}` is not a tuple variant"))
                            .primary(
                                p.span,
                                "tuple pattern on a variant without positional fields",
                            ),
                    );
                    return err(self);
                }
                if ps.len() != field_infos.len() {
                    self.check_subpatterns_for_errors(fields, binds, reuse);
                    self.diags.push(
                        Diagnostic::error(
                            "E0322",
                            format!(
                                "`{name}` has {} but the pattern has {}",
                                crate_plural(field_infos.len()),
                                crate_plural(ps.len())
                            ),
                        )
                        .primary(
                            p.span,
                            format!("expected {}", crate_plural(field_infos.len())),
                        ),
                    );
                    return err(self);
                }
                for (q, f) in ps.iter().zip(&field_infos) {
                    let fty = f.ty.subst(&map);
                    subs.push(self.check_pat(q, &fty, binds, reuse));
                }
            }
            PatFields::Struct { fields: fps, rest } => {
                if shape != VariantShape::Struct {
                    self.check_subpatterns_for_errors(fields, binds, reuse);
                    self.diags.push(
                        Diagnostic::error(
                            "E0314",
                            format!("`{name}` is not a struct-like variant"),
                        )
                        .primary(p.span, "struct pattern on a variant without named fields"),
                    );
                    return err(self);
                }
                let mut slots: Vec<Option<HirPat>> = field_infos.iter().map(|_| None).collect();
                for fp in fps {
                    match field_infos.iter().position(|f| f.name == fp.name.name) {
                        Some(i) => {
                            self.index.refs.push((fp.name.span, field_infos[i].def));
                            let fty = field_infos[i].ty.subst(&map);
                            let h = self.check_pat(&fp.pat, &fty, binds, reuse);
                            if slots[i].is_some() {
                                self.diags.push(
                                    Diagnostic::error(
                                        "E0212",
                                        format!(
                                            "field `{}` is matched more than once",
                                            fp.name.name
                                        ),
                                    )
                                    .primary(fp.name.span, "duplicate field in pattern"),
                                );
                            }
                            slots[i] = Some(h);
                        }
                        None => {
                            let names: Vec<&str> =
                                field_infos.iter().map(|f| f.name.as_str()).collect();
                            let mut d = Diagnostic::error(
                                "E0203",
                                format!("`{name}` has no field named `{}`", fp.name.name),
                            )
                            .primary(fp.name.span, "unknown field");
                            if let Some(b) = closest_match(&fp.name.name, names.iter().copied()) {
                                d = d.suggest(
                                    format!("a field with a similar name exists: `{b}`"),
                                    fp.name.span,
                                    b,
                                    Applicability::MaybeIncorrect,
                                );
                            }
                            self.diags.push(d);
                            self.check_pat(&fp.pat, &Ty::Error, binds, reuse);
                        }
                    }
                }
                let missing: Vec<&str> = field_infos
                    .iter()
                    .zip(&slots)
                    .filter(|(_, s)| s.is_none())
                    .map(|(f, _)| f.name.as_str())
                    .collect();
                if !missing.is_empty() && !rest {
                    let list = missing
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.diags.push(
                        Diagnostic::error(
                            "E0312",
                            format!(
                                "pattern does not mention {} {list}",
                                if missing.len() == 1 {
                                    "field"
                                } else {
                                    "fields"
                                }
                            ),
                        )
                        .primary(p.span, format!("missing {list}"))
                        .help("add `..` to ignore the remaining fields"),
                    );
                }
                for (i, s) in slots.into_iter().enumerate() {
                    subs.push(s.unwrap_or_else(|| wild(self, field_infos[i].ty.subst(&map))));
                }
            }
        }
        // Unit variants carry no sub-patterns; keep the HIR shape consistent with the declaration.
        let kind = match variant {
            Some(vi) => HirPatKind::Variant {
                enum_def: def,
                variant: vi,
                fields: subs,
            },
            None => HirPatKind::Struct(def, subs),
        };
        self.hp(p, expected, kind)
    }

    /// After a resolution error, still check nested patterns so their bindings exist
    /// (otherwise every use of those names would be reported as unresolved).
    fn check_subpatterns_for_errors(
        &mut self,
        fields: &PatFields,
        binds: &mut Binds,
        reuse: Option<&Binds>,
    ) {
        match fields {
            PatFields::Unit => {}
            PatFields::Tuple(ps) => {
                for q in ps {
                    self.check_pat(q, &Ty::Error, binds, reuse);
                }
            }
            PatFields::Struct { fields, .. } => {
                for f in fields {
                    self.check_pat(&f.pat, &Ty::Error, binds, reuse);
                }
            }
        }
    }
}

fn crate_plural(n: usize) -> String {
    if n == 1 {
        "1 field".to_string()
    } else {
        format!("{n} fields")
    }
}
