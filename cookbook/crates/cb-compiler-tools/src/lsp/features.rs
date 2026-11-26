//! Request handlers that answer from one [`Snapshot`]: hover, navigation, rename,
//! outline, signature help, code actions, inlay hints and folding.

use super::server::{INVALID_PARAMS, RpcError, RpcResult, diagnostic_json, rpc_err};
use super::snapshot::Snapshot;
use crate::analysis::diag::Applicability;
use crate::analysis::sema::index::CompletionKind;
use crate::analysis::sema::{Builtin, DefId, DefInfo, DefKind};
use crate::analysis::source::Span;
use crate::analysis::syntax::ast::*;
use crate::analysis::syntax::{CommentKind, TokenKind, lex};
use crate::docs;
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) fn walk<'a>(n: Node<'a>, f: &mut impl FnMut(Node<'a>)) {
    f(n);
    for c in n.children() {
        walk(c, f);
    }
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// The identifier-shaped word touching `offset` (works even where the parser gave no node).
pub(super) fn word_at(text: &str, offset: usize) -> Option<(Span, &str)> {
    let mut o = offset.min(text.len());
    while !text.is_char_boundary(o) {
        o -= 1;
    }
    let start = text[..o]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident_char(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(o);
    let end = o + text[o..]
        .char_indices()
        .find(|(_, c)| !is_ident_char(*c))
        .map(|(i, _)| i)
        .unwrap_or(text.len() - o);
    (start < end).then(|| (Span::new(start, end), &text[start..end]))
}

/// (signature, one-line summary) of every built-in, also used for completion and signature help.
pub(super) fn builtin_info(
    name: &str,
) -> Option<(&'static str, &'static [&'static str], &'static str)> {
    Some(match name {
        "print" => (
            "print(x) -> ()",
            &["x"],
            "Prints an `int`, `bool`, `str` or `()` without a newline.",
        ),
        "println" => (
            "println(x) -> ()",
            &["x"],
            "Prints a value followed by a newline; `println()` prints just the newline.",
        ),
        "assert" => (
            "assert(cond: bool, msg: str) -> ()",
            &["cond: bool", "msg: str"],
            "Traps when `cond` is false.",
        ),
        "panic" => (
            "panic(msg: str) -> !",
            &["msg: str"],
            "Stops the program with a message.",
        ),
        "to_str" => (
            "to_str(x) -> str",
            &["x"],
            "Converts an `int`, `bool` or `str` to `str`.",
        ),
        "len" => (
            "len(x) -> int",
            &["x"],
            "The length of an array, or the byte length of a string.",
        ),
        "push" => (
            "push(xs: [T], v: T) -> ()",
            &["xs: [T]", "v: T"],
            "Appends to an array (the array must be `mut`).",
        ),
        "pop" => (
            "pop(xs: [T]) -> T",
            &["xs: [T]"],
            "Removes and returns the last element; traps when empty.",
        ),
        "chars" => (
            "chars(s: str) -> [str]",
            &["s: str"],
            "Splits a string into one string per Unicode scalar.",
        ),
        _ => return None,
    })
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A one-line declaration for hover, assembled from the source text and the def's type.
pub(super) fn signature(s: &Snapshot, d: &DefInfo) -> String {
    let full = d.full_span.slice(&s.src);
    let ty = d.ty.as_ref().map(|t| t.to_string()).unwrap_or_default();
    match d.kind {
        DefKind::Fn => collapse(full.split('{').next().unwrap_or(full)),
        DefKind::Struct | DefKind::Enum => {
            let one = collapse(full);
            if one.chars().count() <= 160 {
                one
            } else {
                format!(
                    "{} {{ ... }}",
                    collapse(full.split('{').next().unwrap_or(full))
                )
            }
        }
        DefKind::Const => collapse(full.split('=').next().unwrap_or(full)),
        DefKind::Local => format!(
            "let {}{}: {ty}",
            if d.mutable { "mut " } else { "" },
            d.name
        ),
        DefKind::Param => format!(
            "(parameter) {}{}: {ty}",
            if d.mutable { "mut " } else { "" },
            d.name
        ),
        DefKind::Field => format!("(field) {}: {ty}", d.name),
        DefKind::Variant => {
            let owner = d
                .parent
                .map(|p| s.an.index.def(p).name.clone())
                .unwrap_or_default();
            format!("{owner}::{}", collapse(full))
        }
        DefKind::TypeParam => format!("type parameter {}", d.name),
    }
}

fn markdown(code: &str, doc: Option<String>) -> Value {
    let mut v = format!("```pebble\n{code}\n```");
    if let Some(d) = doc {
        v.push_str("\n\n---\n\n");
        v.push_str(&d);
    }
    json!({"kind": "markdown", "value": v})
}

pub fn hover(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    if let Some((sp, def)) = s.names.lookup(off) {
        let d = s.an.index.def(def);
        let doc = docs::doc_comment(&s.src, &s.an.comments, d.full_span.start as usize);
        let mut value = markdown(&signature(s, d), doc);
        if d.kind == DefKind::Fn
            && let Some(t) = s.an.index.type_at(off)
            && Some(t.to_string()) != d.ty.as_ref().map(|t| t.to_string())
            && !t.has_error()
        {
            // The use site's instantiated type (generics solved by inference).
            value["value"] = json!(format!(
                "{}\n\nhere: `{t}`",
                value["value"].as_str().unwrap_or("")
            ));
        }
        return json!({"contents": value, "range": s.range(sp)});
    }
    if let Some((sp, w)) = word_at(&s.src, off)
        && let Some((sig, _, doc)) = builtin_info(w)
    {
        return json!({"contents": markdown(&format!("fn {sig}"), Some(format!("built-in: {doc}"))), "range": s.range(sp)});
    }
    match (s.an.index.node_at(off), s.an.index.type_at(off)) {
        (Some(n), Some(t)) if !t.has_error() => {
            json!({"contents": markdown(&t.to_string(), None), "range": s.range(n.span)})
        }
        _ => Value::Null,
    }
}

pub fn definition(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    match s.names.lookup(off) {
        Some((_, def)) if !s.an.index.def(def).span.is_empty() => {
            s.location(s.an.index.def(def).span)
        }
        _ => Value::Null,
    }
}

pub fn references(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    let include = p
        .get("context")
        .and_then(|c| c.get("includeDeclaration"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    match s.names.lookup(off) {
        Some((_, def)) => Value::Array(
            s.an.index
                .references(def, include)
                .into_iter()
                .map(|sp| s.location(sp))
                .collect(),
        ),
        None => Value::Null,
    }
}

/// Names whose definition span is the identifier itself (tuple-variant fields are named `0`, `1`
/// but their span is the type, which must not be rewritten).
fn renamable(s: &Snapshot, sp: Span, def: crate::analysis::sema::DefId) -> bool {
    sp.slice(&s.src) == s.an.index.def(def).name
}

/// Why `off` cannot be renamed, or the name and definition under it.
fn rename_target(s: &Snapshot, off: usize) -> Result<(Span, DefId), RpcError> {
    if let Some((sp, def)) = s.names.lookup(off) {
        return if renamable(s, sp, def) {
            Ok((sp, def))
        } else {
            Err(rpc_err(
                INVALID_PARAMS,
                "tuple fields are positional and have no name to rename",
            ))
        };
    }
    if let Some((_, w)) = word_at(&s.src, off) {
        if crate::analysis::syntax::token::keyword(w).is_some() {
            return Err(rpc_err(
                INVALID_PARAMS,
                format!("`{w}` is a keyword and cannot be renamed"),
            ));
        }
        if matches!(w, "int" | "bool" | "str") || Builtin::from_name(w).is_some() {
            return Err(rpc_err(
                INVALID_PARAMS,
                format!("`{w}` is built in and cannot be renamed"),
            ));
        }
    }
    Err(rpc_err(INVALID_PARAMS, "nothing to rename here"))
}

/// Reject names the language would not accept as an identifier, with the reason.
fn check_new_name(new: &str) -> Result<(), RpcError> {
    let bad = |m: String| Err(rpc_err(INVALID_PARAMS, m));
    if new.is_empty() {
        return bad("the new name is empty".into());
    }
    if crate::analysis::syntax::token::keyword(new).is_some() {
        return bad(format!("`{new}` is a keyword"));
    }
    if new == "_" {
        return bad("`_` is the wildcard pattern, not a name".into());
    }
    if matches!(new, "int" | "bool" | "str") {
        return bad(format!("`{new}` is a built-in type name"));
    }
    if !valid_identifier(new) {
        return bad(format!("`{new}` is not a valid identifier"));
    }
    Ok(())
}

/// `textDocument/prepareRename`. Standard: the range and placeholder, or an error when the
/// position cannot be renamed (keyword, built-in, literal, nothing). Extension: when the
/// request also carries `newName`, that name is validated too (same checks as `rename`).
pub fn prepare_rename(s: &Snapshot, p: &Value) -> RpcResult {
    let off = s.offset(&p["position"]);
    let (sp, _) = rename_target(s, off)?;
    if let Some(new) = p.get("newName").and_then(Value::as_str) {
        rename_edits(s, off, new)?;
    }
    Ok(json!({"range": s.range(sp), "placeholder": sp.slice(&s.src)}))
}

fn valid_identifier(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && cs.all(is_ident_char)
        && n != "_"
        && crate::analysis::syntax::token::keyword(n).is_none()
}

/// Spans where a struct pattern writes a field and its binding with one name (`Point { x }`).
fn shorthand_spans(s: &Snapshot) -> HashSet<Span> {
    let mut out = HashSet::new();
    walk(s.an.ast.node(), &mut |n| {
        if let Node::FieldPat(f) = n
            && f.pat.span == f.name.span
        {
            out.insert(f.name.span);
        }
    });
    out
}

fn line_of(s: &Snapshot, sp: Span) -> usize {
    s.lines.line_of(sp.start as usize) + 1
}

/// The cheap, precise clash checks (so the message names the actual conflict).
/// The post-condition check in [`verify_rename`] is what guarantees safety.
fn precheck_clash(s: &Snapshot, def: DefId, new: &str) -> Result<(), RpcError> {
    let d = s.an.index.def(def);
    let clash = |m: String| Err(rpc_err(INVALID_PARAMS, m));
    match d.kind {
        DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Const => {
            if let Some(o) = s.an.index.defs.iter().find(|o| {
                o.id != d.id
                    && o.name == new
                    && matches!(
                        o.kind,
                        DefKind::Fn | DefKind::Struct | DefKind::Enum | DefKind::Const
                    )
            }) {
                return clash(format!(
                    "`{new}` already names another item (line {})",
                    line_of(s, o.span)
                ));
            }
            if d.kind == DefKind::Fn && Builtin::from_name(new).is_some() {
                return clash(format!(
                    "`{new}` would shadow the built-in function `{new}`"
                ));
            }
        }
        DefKind::Field | DefKind::Variant => {
            if let Some(o) =
                s.an.index.defs.iter().find(|o| {
                    o.id != d.id && o.kind == d.kind && o.parent == d.parent && o.name == new
                })
            {
                let owner = d
                    .parent
                    .map(|p| s.an.index.def(p).name.clone())
                    .unwrap_or_default();
                return clash(format!(
                    "`{owner}` already has a {} named `{new}` (line {})",
                    if d.kind == DefKind::Field {
                        "field"
                    } else {
                        "variant"
                    },
                    line_of(s, o.span)
                ));
            }
        }
        DefKind::Local | DefKind::Param => {
            // Another local or parameter with the new name visible where this one is declared
            // or used: the rename would shadow it or be shadowed by it.
            let others: Vec<&DefInfo> =
                s.an.index
                    .defs
                    .iter()
                    .filter(|o| {
                        o.id != d.id
                            && o.name == new
                            && matches!(o.kind, DefKind::Local | DefKind::Param)
                    })
                    .collect();
            if !others.is_empty() {
                for site in s.an.index.references(def, true) {
                    let hit = s
                        .an
                        .index
                        .completions_at(site.start as usize)
                        .into_iter()
                        .any(|c| {
                            c.label == new
                                && matches!(c.kind, CompletionKind::Local | CompletionKind::Param)
                        });
                    if hit {
                        let o = others
                            .iter()
                            .min_by_key(|o| (o.span.start as i64 - site.start as i64).abs())
                            .map(|o| line_of(s, o.span))
                            .unwrap_or(0);
                        return clash(format!(
                            "`{new}` is already a local or parameter in scope (declared on line {o}); the rename would shadow it or be shadowed"
                        ));
                    }
                }
            }
        }
        DefKind::TypeParam => {}
    }
    Ok(())
}

/// Apply `(span, text)` edits to `src` (non-overlapping, any order).
fn apply(src: &str, edits: &[(Span, String)]) -> String {
    let mut sorted: Vec<&(Span, String)> = edits.iter().collect();
    sorted.sort_by_key(|e| e.0.start);
    let mut out = String::with_capacity(src.len());
    let mut at = 0usize;
    for (sp, text) in sorted {
        out.push_str(&src[at..sp.start as usize]);
        out.push_str(text);
        at = sp.end as usize;
    }
    out.push_str(&src[at..]);
    out
}

/// The safety net: re-analyse the renamed text and require that every use of a name still
/// resolves to the same definition and that no new error appeared. Name-resolution rules
/// (shadowing, namespaces, built-ins) are the compiler's, so the check cannot disagree with it.
fn verify_rename(s: &Snapshot, edits: &[(Span, String)], new: &str) -> Result<(), RpcError> {
    let text = apply(&s.src, edits);
    let an2 = crate::analysis::analyze(&text);
    let errors =
        |a: &crate::analysis::Analysis| a.diagnostics.iter().filter(|d| d.is_error()).count();
    let refuse = |why: String| {
        Err(rpc_err(
            INVALID_PARAMS,
            format!("renaming to `{new}` would change the program: {why}"),
        ))
    };
    if errors(&an2) > errors(&s.an) {
        let msg = an2
            .diagnostics
            .iter()
            .find(|d| d.is_error())
            .map(|d| d.message.clone())
            .unwrap_or_default();
        return refuse(format!("it introduces an error ({msg})"));
    }
    let key = |a: &crate::analysis::Analysis| {
        let mut v: Vec<(u32, u32, u32)> = a
            .index
            .use_sites()
            .iter()
            .map(|(sp, d)| (sp.start, sp.end, d.0))
            .collect();
        v.sort();
        v
    };
    let (before, after) = (key(&s.an), key(&an2));
    if s.an.index.defs.len() != an2.index.defs.len() || before.len() != after.len() {
        return refuse(
            "the set of resolved names differs (a built-in or another item would be captured)"
                .into(),
        );
    }
    for (b, a) in before.iter().zip(&after) {
        if b.2 != a.2 {
            let at = s.lines.line_col_utf16(&s.src, b.0 as usize);
            return refuse(format!(
                "the name at line {}, column {} would refer to a different definition",
                at.line + 1,
                at.col + 1
            ));
        }
    }
    Ok(())
}

/// All edits for renaming the symbol at `off` to `new`, or why not.
fn rename_edits(s: &Snapshot, off: usize, new: &str) -> Result<Vec<(Span, String)>, RpcError> {
    check_new_name(new)?;
    let (_, def) = rename_target(s, off)?;
    let d = s.an.index.def(def);
    if d.name == new {
        return Ok(vec![]);
    }
    precheck_clash(s, def, new)?;
    let shorthand = shorthand_spans(s);
    let edits: Vec<(Span, String)> =
        s.an.index
            .references(def, true)
            .into_iter()
            .map(|sp| {
                if !shorthand.contains(&sp) {
                    return (sp, new.to_string());
                }
                // `Point { x }` names a field and a binding at once: keep the other meaning intact.
                let start = sp.start as usize;
                let mut_prefix = s.src[..start].ends_with("mut ");
                let full = Span::new(if mut_prefix { start - 4 } else { start }, sp.end as usize);
                let old = sp.slice(&s.src);
                let m = if mut_prefix { "mut " } else { "" };
                let text = if d.kind == DefKind::Field {
                    format!("{new}: {m}{old}")
                } else {
                    format!("{old}: {m}{new}")
                };
                (full, text)
            })
            .collect();
    verify_rename(s, &edits, new)?;
    Ok(edits)
}

pub fn rename(s: &Snapshot, p: &Value) -> RpcResult {
    let off = s.offset(&p["position"]);
    let new = p.get("newName").and_then(Value::as_str).unwrap_or("");
    let edits = rename_edits(s, off, new)?;
    let edits: Vec<Value> = edits
        .into_iter()
        .map(|(sp, t)| s.text_edit(sp, &t))
        .collect();
    Ok(json!({"changes": {s.uri.clone(): edits}}))
}

// ---------------------------------------------------------------- outline

fn symbol(
    s: &Snapshot,
    name: &Ident,
    full: Span,
    kind: u32,
    detail: String,
    children: Vec<Value>,
) -> Value {
    json!({
        "name": if name.name.is_empty() { "<missing>" } else { &name.name },
        "detail": detail,
        "kind": kind,
        "range": s.range(full),
        "selectionRange": s.range(name.span),
        "children": children
    })
}

fn field_symbols(s: &Snapshot, fields: &[FieldDecl]) -> Vec<Value> {
    fields
        .iter()
        .map(|f| {
            symbol(
                s,
                &f.name,
                f.span,
                8,
                f.ty.span.slice(&s.src).to_string(),
                vec![],
            )
        })
        .collect()
}

pub fn document_symbols(s: &Snapshot) -> Value {
    let mut out = Vec::new();
    for item in &s.an.ast.items {
        out.push(match &item.kind {
            ItemKind::Fn(f) => {
                let ret = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", t.span.slice(&s.src)))
                    .unwrap_or_default();
                symbol(
                    s,
                    &f.name,
                    item.span,
                    12,
                    format!("({} params){ret}", f.params.len()),
                    vec![],
                )
            }
            ItemKind::Struct(st) => symbol(
                s,
                &st.name,
                item.span,
                23,
                String::new(),
                field_symbols(s, &st.fields),
            ),
            ItemKind::Enum(e) => {
                let vs = e
                    .variants
                    .iter()
                    .map(|v| {
                        let kids = if let VariantKind::Struct(fs) = &v.kind {
                            field_symbols(s, fs)
                        } else {
                            vec![]
                        };
                        symbol(s, &v.name, v.span, 22, String::new(), kids)
                    })
                    .collect();
                symbol(s, &e.name, item.span, 10, String::new(), vs)
            }
            ItemKind::Const(c) => symbol(
                s,
                &c.name,
                item.span,
                14,
                c.ty.span.slice(&s.src).to_string(),
                vec![],
            ),
            ItemKind::Error => continue,
        });
    }
    Value::Array(out)
}

// ---------------------------------------------------------------- signature help

fn call_signature(s: &Snapshot, callee: &Expr) -> Option<(String, Vec<String>, Option<String>)> {
    let ExprKind::Path(path) = &callee.kind else {
        return None;
    };
    let name = path.segments.last()?;
    if let Some((_, def)) = s.names.lookup(name.span.start as usize) {
        let d = s.an.index.def(def);
        for item in &s.an.ast.items {
            if let ItemKind::Fn(f) = &item.kind
                && f.name.span == d.span
            {
                let params: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name.name, p.ty.span.slice(&s.src)))
                    .collect();
                let ret = f
                    .ret
                    .as_ref()
                    .map(|t| format!(" -> {}", t.span.slice(&s.src)))
                    .unwrap_or_default();
                let label = format!("fn {}({}){ret}", f.name.name, params.join(", "));
                return Some((
                    label,
                    params,
                    docs::doc_comment(&s.src, &s.an.comments, item.span.start as usize),
                ));
            }
        }
        return None;
    }
    let (sig, params, doc) = builtin_info(&name.name)?;
    Some((
        format!("fn {sig}"),
        params.iter().map(|p| p.to_string()).collect(),
        Some(doc.to_string()),
    ))
}

/// Commas at nesting depth 0 between `open` (a `(`) and `offset`.
fn active_parameter(s: &Snapshot, open: usize, offset: usize) -> usize {
    let (toks, _, _) = lex(&s.src);
    let mut depth = 0i32;
    let mut commas = 0;
    for t in toks
        .iter()
        .filter(|t| t.span.start as usize > open && (t.span.end as usize) <= offset)
    {
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
            TokenKind::Comma if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas
}

pub fn signature_help(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    let mut best: Option<(&Expr, usize)> = None; // innermost call whose argument list holds `offset`
    walk(s.an.ast.node(), &mut |n| {
        if let Node::Expr(e) = n
            && let ExprKind::Call { callee, .. } = &e.kind
            && e.span.start as usize <= off
            && off <= e.span.end as usize
        {
            let open = s.src[callee.span.end as usize..]
                .find('(')
                .map(|i| i + callee.span.end as usize);
            if let Some(open) = open
                && open < off
                && best.is_none_or(|(b, _)| e.span.len() < b.span.len())
            {
                best = Some((e, open));
            }
        }
    });
    let Some((call, open)) = best else {
        return Value::Null;
    };
    let ExprKind::Call { callee, .. } = &call.kind else {
        return Value::Null;
    };
    let Some((label, params, doc)) = call_signature(s, callee) else {
        return Value::Null;
    };
    let mut sig = json!({"label": label, "parameters": params.iter().map(|p| json!({"label": p})).collect::<Vec<_>>()});
    if let Some(d) = doc {
        sig["documentation"] = json!({"kind": "markdown", "value": d});
    }
    let active = active_parameter(s, open, off).min(params.len().saturating_sub(1));
    json!({"signatures": [sig], "activeSignature": 0, "activeParameter": active})
}

// ---------------------------------------------------------------- code actions

pub fn code_actions(s: &Snapshot, p: &Value) -> Value {
    let want = s.span_of_range(&p["range"]);
    let mut actions = Vec::new();
    for d in &s.an.diagnostics {
        let Some(sp) = d.primary_span() else { continue };
        if sp.end < want.start || sp.start > want.end {
            continue;
        }
        for sug in &d.suggestions {
            let edits: Vec<Value> = sug
                .edits
                .iter()
                .map(|e| s.text_edit(e.span, &e.replacement))
                .collect();
            actions.push(json!({
                "title": sug.message,
                "kind": "quickfix",
                "diagnostics": [diagnostic_json(s, d)],
                "isPreferred": sug.applicability == Applicability::MachineApplicable,
                "edit": {"changes": {s.uri.clone(): edits}}
            }));
        }
    }
    Value::Array(actions)
}

// ---------------------------------------------------------------- inlay hints

pub fn inlay_hints(s: &Snapshot, p: &Value) -> Value {
    let want = if p.get("range").is_some() {
        s.span_of_range(&p["range"])
    } else {
        Span::new(0, s.src.len())
    };
    let mut hints = Vec::new();
    let inside = |at: u32| want.start <= at && at <= want.end;
    walk(s.an.ast.node(), &mut |n| match n {
        Node::Stmt(st) => {
            if let StmtKind::Let {
                pat,
                ty: None,
                init: Some(_),
            } = &st.kind
                && let PatKind::Binding { name, .. } = &pat.kind
                && inside(name.span.end)
                && let Some((_, def)) = s.names.lookup(name.span.start as usize)
                && let Some(t) =
                    s.an.index
                        .def(def)
                        .ty
                        .as_ref()
                        .filter(|t| !t.has_error() && !t.has_var())
            {
                hints.push(json!({"position": s.position(name.span.end as usize), "label": format!(": {t}"), "kind": 1, "paddingLeft": false}));
            }
        }
        Node::Expr(e) => {
            if let ExprKind::Call { callee, args } = &e.kind
                && let Some((_, params, _)) = call_signature(s, callee)
            {
                for (arg, param) in args.iter().zip(&params) {
                    let pname = param.split(':').next().unwrap_or("").trim();
                    let same = matches!(&arg.kind, ExprKind::Path(p) if p.segments.len() == 1 && p.segments[0].name == pname);
                    if !same && !pname.is_empty() && params.len() > 1 && inside(arg.span.start) {
                        hints.push(json!({"position": s.position(arg.span.start as usize), "label": format!("{pname}:"), "kind": 2, "paddingRight": true}));
                    }
                }
            }
        }
        _ => {}
    });
    Value::Array(hints)
}

// ---------------------------------------------------------------- folding

pub fn folding_ranges(s: &Snapshot) -> Value {
    let mut out: Vec<(usize, usize, &'static str)> = Vec::new();
    let line = |o: u32| s.lines.line_of(o as usize);
    let fold =
        |open: u32, close: u32, kind: &'static str, out: &mut Vec<(usize, usize, &'static str)>| {
            let (a, b) = (line(open), line(close));
            if b > a + 1 || (b == a + 1 && kind != "region") {
                out.push((a, b - 1, kind));
            }
        };
    walk(s.an.ast.node(), &mut |n| match n {
        Node::Block(b) => fold(
            b.span.start,
            b.span.end.saturating_sub(1),
            "region",
            &mut out,
        ),
        Node::Item(i) if matches!(i.kind, ItemKind::Struct(_) | ItemKind::Enum(_)) => {
            if let Some(open) = s.src[i.span.start as usize..i.span.end as usize].find('{') {
                fold(
                    (i.span.start as usize + open) as u32,
                    i.span.end.saturating_sub(1),
                    "region",
                    &mut out,
                );
            }
        }
        Node::Expr(e) if matches!(e.kind, ExprKind::Match { .. }) => {
            if let Some(open) = s.src[e.span.start as usize..e.span.end as usize].find('{') {
                fold(
                    (e.span.start as usize + open) as u32,
                    e.span.end.saturating_sub(1),
                    "region",
                    &mut out,
                );
            }
        }
        _ => {}
    });
    // Runs of consecutive line comments, and multi-line block comments.
    let mut run: Option<(usize, usize)> = None;
    for c in &s.an.comments {
        let (a, b) = (
            line(c.span.start),
            line(c.span.end.saturating_sub(1).max(c.span.start)),
        );
        match (c.kind, run) {
            (CommentKind::Block, _) => {
                if b > a {
                    out.push((a, b - 1, "comment"));
                }
            }
            (_, Some((ra, rb))) if a == rb + 1 => run = Some((ra, a)),
            (_, prev) => {
                if let Some((ra, rb)) = prev
                    && rb > ra
                {
                    out.push((ra, rb, "comment"));
                }
                run = Some((a, a));
            }
        }
    }
    if let Some((ra, rb)) = run
        && rb > ra
    {
        out.push((ra, rb, "comment"));
    }
    out.sort();
    out.dedup();
    Value::Array(
        out.into_iter()
            .map(|(a, b, k)| json!({"startLine": a, "endLine": b, "kind": k}))
            .collect(),
    )
}

/// Builtin names for completion.
pub(super) fn builtin_names() -> Vec<&'static str> {
    Builtin::ALL.iter().map(|b| b.name()).collect()
}
