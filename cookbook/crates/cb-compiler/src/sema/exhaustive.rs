//! Pattern-match analysis: exhaustiveness, useless (unreachable) arms and
//! refutable `let`s, using Maranget's "usefulness" algorithm over a pattern matrix.
//!
//! A pattern is a wildcard, a constructor applied to sub-patterns, or an
//! or-pattern. A row is *useful* with respect to a matrix if some value matches
//! the row but no row of the matrix. A match is exhaustive iff the all-wildcard
//! row is not useful; the algorithm can also *construct* such a missing value
//! (a "witness"), which we print in the error message.
//!
//! Reference: Luc Maranget, "Warnings for pattern matching" (JFP 2007).

use super::hir::*;
use super::types::{DefId, Ty};
use crate::diag::Diagnostic;
use crate::syntax::ast::Lit;

#[derive(Clone, Debug, PartialEq)]
pub enum Ctor {
    Variant(DefId, usize),
    Struct(DefId),
    Tuple(usize),
    Bool(bool),
    Int(i64),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Wild,
    Ctor(Ctor, Vec<Pat>),
    Or(Vec<Pat>),
}

/// Lower a typed pattern to the algorithm's representation.
pub fn lower(p: &HirPat) -> Pat {
    match &p.kind {
        HirPatKind::Wild | HirPatKind::Bind(..) | HirPatKind::Error => Pat::Wild,
        HirPatKind::Lit(Lit::Int(v)) => Pat::Ctor(Ctor::Int(*v), vec![]),
        HirPatKind::Lit(Lit::Bool(b)) => Pat::Ctor(Ctor::Bool(*b), vec![]),
        HirPatKind::Lit(Lit::Str(s)) => Pat::Ctor(Ctor::Str(s.clone()), vec![]),
        HirPatKind::Lit(Lit::Unit) => Pat::Ctor(Ctor::Tuple(0), vec![]),
        HirPatKind::Tuple(ps) => Pat::Ctor(Ctor::Tuple(ps.len()), ps.iter().map(lower).collect()),
        HirPatKind::Struct(d, ps) => Pat::Ctor(Ctor::Struct(*d), ps.iter().map(lower).collect()),
        HirPatKind::Variant {
            enum_def,
            variant,
            fields,
        } => Pat::Ctor(
            Ctor::Variant(*enum_def, *variant),
            fields.iter().map(lower).collect(),
        ),
        HirPatKind::Or(ps) => Pat::Or(ps.iter().map(lower).collect()),
    }
}

/// All constructors of a type with their field types; `None` for open types (`int`, `str`).
fn ctors(prog: &Program, ty: &Ty) -> Option<Vec<(Ctor, Vec<Ty>)>> {
    match ty {
        Ty::Bool => Some(vec![
            (Ctor::Bool(false), vec![]),
            (Ctor::Bool(true), vec![]),
        ]),
        Ty::Unit => Some(vec![(Ctor::Tuple(0), vec![])]),
        Ty::Tuple(ts) => Some(vec![(Ctor::Tuple(ts.len()), ts.clone())]),
        Ty::Adt(def, _, args) => {
            if let Some(e) = prog.enum_info(*def) {
                Some(
                    (0..e.variants.len())
                        .map(|i| {
                            (
                                Ctor::Variant(*def, i),
                                prog.field_types(*def, Some(i), args),
                            )
                        })
                        .collect(),
                )
            } else {
                prog.struct_info(*def)
                    .map(|_| vec![(Ctor::Struct(*def), prog.field_types(*def, None, args))])
            }
        }
        _ => None,
    }
}

fn field_tys(prog: &Program, ty: &Ty, c: &Ctor, arity: usize) -> Vec<Ty> {
    if let Some(all) = ctors(prog, ty)
        && let Some((_, f)) = all.into_iter().find(|(k, _)| k == c)
        && f.len() == arity
    {
        return f;
    }
    vec![Ty::Error; arity]
}

type Row = Vec<Pat>;

fn expand(rows: &[Row]) -> Vec<Row> {
    let mut out = Vec::new();
    for r in rows {
        expand_row(r, &mut out);
    }
    out
}

fn expand_row(r: &Row, out: &mut Vec<Row>) {
    match r.first() {
        Some(Pat::Or(alts)) => {
            for a in alts {
                let mut nr = r.clone();
                nr[0] = a.clone();
                expand_row(&nr, out);
            }
        }
        _ => out.push(r.clone()),
    }
}

fn specialize(rows: &[Row], c: &Ctor, arity: usize) -> Vec<Row> {
    let mut out = Vec::new();
    for r in rows {
        match &r[0] {
            Pat::Wild => {
                let mut nr = vec![Pat::Wild; arity];
                nr.extend_from_slice(&r[1..]);
                out.push(nr);
            }
            Pat::Ctor(k, args) if k == c && args.len() == arity => {
                let mut nr = args.clone();
                nr.extend_from_slice(&r[1..]);
                out.push(nr);
            }
            _ => {}
        }
    }
    out
}

fn default_rows(rows: &[Row]) -> Vec<Row> {
    rows.iter()
        .filter(|r| matches!(r[0], Pat::Wild))
        .map(|r| r[1..].to_vec())
        .collect()
}

fn heads(rows: &[Row]) -> Vec<Ctor> {
    let mut hs: Vec<Ctor> = Vec::new();
    for r in rows {
        if let Pat::Ctor(c, _) = &r[0]
            && !hs.contains(c)
        {
            hs.push(c.clone());
        }
    }
    hs
}

/// Is `q` useful with respect to `rows`, i.e. does it match a value no row matches?
pub fn is_useful(prog: &Program, rows: &[Row], q: &[Pat], tys: &[Ty]) -> bool {
    if q.is_empty() {
        return rows.is_empty();
    }
    let rows = expand(rows);
    match &q[0] {
        Pat::Or(alts) => alts.iter().any(|a| {
            let mut q2 = q.to_vec();
            q2[0] = a.clone();
            is_useful(prog, &rows, &q2, tys)
        }),
        Pat::Ctor(c, args) => {
            let mut tys2 = field_tys(prog, &tys[0], c, args.len());
            tys2.extend_from_slice(&tys[1..]);
            let mut q2 = args.clone();
            q2.extend_from_slice(&q[1..]);
            is_useful(prog, &specialize(&rows, c, args.len()), &q2, &tys2)
        }
        Pat::Wild => {
            let hs = heads(&rows);
            match ctors(prog, &tys[0]) {
                Some(all) if all.iter().all(|(c, _)| hs.contains(c)) => {
                    all.iter().any(|(c, ftys)| {
                        let mut q2 = vec![Pat::Wild; ftys.len()];
                        q2.extend_from_slice(&q[1..]);
                        let mut tys2 = ftys.clone();
                        tys2.extend_from_slice(&tys[1..]);
                        is_useful(prog, &specialize(&rows, c, ftys.len()), &q2, &tys2)
                    })
                }
                _ => is_useful(prog, &default_rows(&rows), &q[1..], &tys[1..]),
            }
        }
    }
}

/// A vector of patterns (one per type in `tys`) matching a value that no row matches, if any.
pub fn witness(prog: &Program, rows: &[Row], tys: &[Ty]) -> Option<Vec<Pat>> {
    if tys.is_empty() {
        return if rows.is_empty() { Some(vec![]) } else { None };
    }
    let rows = expand(rows);
    let hs = heads(&rows);
    let all = ctors(prog, &tys[0]);
    if let Some(all) = &all
        && all.iter().all(|(c, _)| hs.contains(c))
    {
        for (c, ftys) in all {
            let n = ftys.len();
            let mut tys2 = ftys.clone();
            tys2.extend_from_slice(&tys[1..]);
            if let Some(mut w) = witness(prog, &specialize(&rows, c, n), &tys2) {
                let rest = w.split_off(n);
                let mut out = vec![Pat::Ctor(c.clone(), w)];
                out.extend(rest);
                return Some(out);
            }
        }
        return None;
    }
    let mut w = witness(prog, &default_rows(&rows), &tys[1..])?;
    let head = match &all {
        Some(all) => match all.iter().find(|(c, _)| !hs.contains(c)) {
            Some((c, f)) => Pat::Ctor(c.clone(), vec![Pat::Wild; f.len()]),
            None => Pat::Wild,
        },
        None => Pat::Wild,
    };
    let mut out = vec![head];
    out.append(&mut w);
    Some(out)
}

/// Up to `max` distinct missing patterns for a single column of type `ty`.
pub fn missing_patterns(prog: &Program, rows: &[Pat], ty: &Ty, max: usize) -> Vec<Pat> {
    let mut matrix: Vec<Row> = rows.iter().map(|p| vec![p.clone()]).collect();
    let mut found = Vec::new();
    while found.len() < max {
        match witness(prog, &matrix, std::slice::from_ref(ty)) {
            Some(mut w) if !w.is_empty() => {
                let p = w.remove(0);
                matrix.push(vec![p.clone()]);
                found.push(p);
            }
            _ => break,
        }
    }
    found
}

pub fn show(prog: &Program, p: &Pat) -> String {
    match p {
        Pat::Wild => "_".into(),
        Pat::Or(ps) => ps
            .iter()
            .map(|q| show(prog, q))
            .collect::<Vec<_>>()
            .join(" | "),
        Pat::Ctor(c, args) => {
            let list = |args: &[Pat]| {
                args.iter()
                    .map(|a| show(prog, a))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            match c {
                Ctor::Bool(b) => b.to_string(),
                Ctor::Int(i) => i.to_string(),
                Ctor::Str(s) => format!("{s:?}"),
                Ctor::Tuple(0) => "()".into(),
                Ctor::Tuple(_) => format!("({})", list(args)),
                Ctor::Struct(d) => format!("{} {{ .. }}", prog.def(*d).name),
                Ctor::Variant(d, i) => {
                    let (en, v) = match prog.enum_info(*d) {
                        Some(e) => (e.name.clone(), Some(&e.variants[*i])),
                        None => (String::from("?"), None),
                    };
                    let name = v.map(|v| v.name.clone()).unwrap_or_default();
                    match v.map(|v| v.shape) {
                        Some(VariantShape::Tuple) => format!("{en}::{name}({})", list(args)),
                        Some(VariantShape::Struct) => format!("{en}::{name} {{ .. }}"),
                        _ => format!("{en}::{name}"),
                    }
                }
            }
        }
    }
}

fn pat_has_error(p: &HirPat) -> bool {
    p.ty.has_error()
        || match &p.kind {
            HirPatKind::Error => true,
            HirPatKind::Tuple(ps)
            | HirPatKind::Or(ps)
            | HirPatKind::Struct(_, ps)
            | HirPatKind::Variant { fields: ps, .. } => ps.iter().any(pat_has_error),
            _ => false,
        }
}

fn check_match(prog: &Program, diags: &mut Vec<Diagnostic>, scrut: &HirExpr, arms: &[HirArm]) {
    if scrut.ty.has_error() || arms.iter().any(|a| pat_has_error(&a.pat)) {
        return;
    }
    let tys = std::slice::from_ref(&scrut.ty);
    let mut rows: Vec<Row> = Vec::new();
    for arm in arms {
        let lowered = lower(&arm.pat);
        // Check each top-level alternative separately so a dead `B` in `A | B` is reported.
        let alts: Vec<(Pat, crate::source::Span)> = match (&lowered, &arm.pat.kind) {
            (Pat::Or(ps), HirPatKind::Or(hs)) => {
                ps.iter().cloned().zip(hs.iter().map(|h| h.span)).collect()
            }
            _ => vec![(lowered.clone(), arm.pat.span)],
        };
        let mut local_rows = rows.clone();
        for (alt, span) in alts {
            if !is_useful(prog, &local_rows, std::slice::from_ref(&alt), tys) {
                diags.push(
                    Diagnostic::warning("W0504", "unreachable pattern")
                        .primary(span, "no value can reach this pattern")
                        .note("earlier arms already match every value this pattern matches"),
                );
            }
            local_rows.push(vec![alt]);
        }
        if arm.guard.is_none() {
            rows = local_rows;
        }
    }
    let pats: Vec<Pat> = rows.iter().map(|r| r[0].clone()).collect();
    let missing = missing_patterns(prog, &pats, &scrut.ty, 4);
    if missing.is_empty() {
        return;
    }
    let shown: Vec<String> = missing
        .iter()
        .take(3)
        .map(|p| format!("`{}`", show(prog, p)))
        .collect();
    let more = if missing.len() > 3 { " and more" } else { "" };
    let list = match shown.len() {
        1 => shown[0].clone(),
        2 => format!("{} and {}", shown[0], shown[1]),
        _ => format!("{}, {}{} and {}", shown[0], shown[1], more, shown[2]),
    };
    diags.push(
        Diagnostic::error(
            "E0401",
            format!("non-exhaustive patterns: {list} not covered"),
        )
        .primary(
            scrut.span,
            format!(
                "pattern{} {list} not covered",
                if shown.len() > 1 { "s" } else { "" }
            ),
        )
        .note(format!("the matched value has type `{}`", scrut.ty))
        .help("add the missing arms or a wildcard arm `_ => ...`"),
    );
}

fn check_let(prog: &Program, diags: &mut Vec<Diagnostic>, pat: &HirPat) {
    if pat_has_error(pat) {
        return;
    }
    let rows = [lower(pat)];
    let missing = missing_patterns(prog, &rows, &pat.ty, 1);
    if let Some(m) = missing.first() {
        diags.push(
            Diagnostic::error("E0402", "refutable pattern in `let` binding")
                .primary(pat.span, format!("pattern `{}` not covered", show(prog, m)))
                .note(format!("the bound value has type `{}`", pat.ty))
                .help("use `match` to handle the other cases"),
        );
    }
}

/// Run all pattern checks over one function or constant body.
pub fn check_body(prog: &Program, diags: &mut Vec<Diagnostic>, body: &mut HirExpr) {
    walk_expr(
        body,
        &mut |e| match &e.kind {
            HirExprKind::Match(s, arms) => check_match(prog, diags, s, arms),
            HirExprKind::Block(b) => {
                for s in &b.stmts {
                    if let HirStmt::Let { pat, .. } = s {
                        check_let(prog, diags, pat);
                    }
                }
            }
            _ => {}
        },
        &mut |_| {},
    );
}
