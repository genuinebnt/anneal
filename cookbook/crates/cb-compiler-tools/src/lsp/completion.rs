//! `textDocument/completion` and `completionItem/resolve`.
//!
//! The context is found from the text before the cursor (the AST of half-typed
//! code is unreliable): after `.` we look up the type of the expression that ends
//! at the dot; after `Name::` we list that enum's variants; otherwise we offer
//! what is visible at the offset (locals in scope, items, built-ins, keywords).

use super::features::{builtin_info, builtin_names, signature, walk};
use super::snapshot::Snapshot;
use crate::analysis::sema::index::CompletionKind;
use crate::analysis::sema::{DefKind, Ty};
use crate::analysis::syntax::ast::Node;
use crate::docs;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::rc::Rc;

/// Keywords that expand to a snippet (the rest are offered as plain keywords).
const SNIPPETS: [(&str, &str); 8] = [
    ("fn", "fn ${1:name}(${2}) {\n\t$0\n}"),
    ("struct", "struct ${1:Name} {\n\t${2:field}: ${3:int},\n}"),
    ("enum", "enum ${1:Name} {\n\t${2:Variant},\n}"),
    ("match", "match ${1:value} {\n\t${2:_} => $0,\n}"),
    ("if", "if ${1:cond} {\n\t$0\n}"),
    ("while", "while ${1:cond} {\n\t$0\n}"),
    ("loop", "loop {\n\t$0\n}"),
    ("let", "let ${1:name} = $0;"),
];

fn lsp_kind(k: CompletionKind) -> u32 {
    match k {
        CompletionKind::Local | CompletionKind::Param => 6,
        CompletionKind::Function | CompletionKind::Builtin => 3,
        CompletionKind::Const => 21,
        CompletionKind::Struct => 22,
        CompletionKind::Enum => 13,
        CompletionKind::Variant => 20,
        CompletionKind::Field => 5,
        CompletionKind::Keyword => 14,
    }
}

fn sort_group(k: CompletionKind) -> &'static str {
    match k {
        CompletionKind::Local | CompletionKind::Param | CompletionKind::Field => "0",
        CompletionKind::Function | CompletionKind::Const => "1",
        CompletionKind::Struct | CompletionKind::Enum | CompletionKind::Variant => "2",
        CompletionKind::Builtin => "3",
        CompletionKind::Keyword => "4",
    }
}

enum Context {
    Member { receiver_end: usize },
    Path { qualifier: String },
    Plain,
}

fn context(text: &str, offset: usize) -> Context {
    let mut o = offset.min(text.len());
    while !text.is_char_boundary(o) {
        o -= 1;
    }
    let before = &text[..o];
    let prefix_len = before
        .chars()
        .rev()
        .take_while(|c| *c == '_' || c.is_alphanumeric())
        .map(char::len_utf8)
        .sum::<usize>();
    let rest = &before[..before.len() - prefix_len];
    if let Some(q) = rest.strip_suffix("::") {
        let name: String = q
            .chars()
            .rev()
            .take_while(|c| *c == '_' || c.is_alphanumeric())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Context::Path { qualifier: name };
    }
    if rest.ends_with('.') && !rest.ends_with("..") {
        return Context::Member {
            receiver_end: rest[..rest.len() - 1].trim_end().len(),
        };
    }
    Context::Plain
}

fn item(label: &str, kind: u32, detail: &str, sort: &str, uri: &str) -> Value {
    json!({"label": label, "kind": kind, "detail": detail, "sortText": format!("{sort}{label}"), "data": {"uri": uri, "label": label}})
}

/// Built-in method sugar available on a receiver of type `ty`.
fn methods_of(ty: &Ty) -> Vec<(&'static str, &'static str)> {
    match ty {
        Ty::Array(_) => vec![
            ("len", "fn() -> int"),
            ("push", "fn(T) -> ()"),
            ("pop", "fn() -> T"),
        ],
        Ty::Str => vec![
            ("len", "fn() -> int"),
            ("chars", "fn() -> [str]"),
            ("to_str", "fn() -> str"),
        ],
        Ty::Int | Ty::Bool => vec![("to_str", "fn() -> str")],
        _ => vec![],
    }
}

fn member_items(s: &Snapshot, receiver_end: usize) -> Vec<Value> {
    // The outermost expression that ends right before the dot.
    let mut best: Option<&crate::analysis::syntax::ast::Expr> = None;
    walk(s.an.ast.node(), &mut |n| {
        if let Node::Expr(e) = n
            && e.span.end as usize == receiver_end
            && !e.span.is_empty()
            && best.is_none_or(|b| e.span.len() > b.span.len())
        {
            best = Some(e);
        }
    });
    let Some(recv) = best else { return vec![] };
    // The checker skips typing a field access whose name is missing (`p.`), so fall back to
    // the declared type of a plain variable receiver.
    let ty = s.an.index.type_of(recv.id).cloned().or_else(|| {
        s.names
            .lookup(recv.span.start as usize)
            .and_then(|(_, d)| s.an.index.def(d).ty.clone())
    });
    let Some(ty) = ty.as_ref() else { return vec![] };
    let mut out: Vec<Value> =
        s.an.index
            .fields_of(ty)
            .iter()
            .map(|c| item(&c.label, 5, &c.detail, "0", &s.uri))
            .collect();
    for (name, detail) in methods_of(ty) {
        let mut it = item(name, 2, detail, "1", &s.uri);
        it["insertText"] = json!(format!("{name}($0)"));
        it["insertTextFormat"] = json!(2);
        out.push(it);
    }
    if let Ty::Tuple(ts) = ty {
        out.extend((0..ts.len()).map(|i| item(&i.to_string(), 5, &ts[i].to_string(), "0", &s.uri)));
    }
    out
}

fn variant_items(s: &Snapshot, qualifier: &str) -> Vec<Value> {
    s.an.index
        .defs
        .iter()
        .filter(|d| {
            d.kind == DefKind::Variant
                && d.parent
                    .is_some_and(|p| s.an.index.def(p).name == qualifier)
        })
        .map(|d| item(&d.name, 20, &signature(s, d), "0", &s.uri))
        .collect()
}

fn scope_items(s: &Snapshot, offset: usize) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in s.an.index.completions_at(offset) {
        if !seen.insert((c.label.clone(), c.kind as u8)) {
            continue;
        }
        if c.kind == CompletionKind::Keyword
            && let Some((_, body)) = SNIPPETS.iter().find(|(k, _)| *k == c.label)
        {
            let mut it = item(&c.label, 15, "snippet", sort_group(c.kind), &s.uri);
            it["insertText"] = json!(body);
            it["insertTextFormat"] = json!(2);
            out.push(it);
            continue;
        }
        out.push(item(
            &c.label,
            lsp_kind(c.kind),
            &c.detail,
            sort_group(c.kind),
            &s.uri,
        ));
    }
    out
}

pub fn complete(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    let items = match context(&s.src, off) {
        Context::Member { receiver_end } => member_items(s, receiver_end),
        Context::Path { qualifier } => variant_items(s, &qualifier),
        Context::Plain => scope_items(s, off),
    };
    json!({"isIncomplete": false, "items": items})
}

/// Fill in `documentation` for the item the user highlighted.
pub fn resolve(p: &Value, snapshot_of: &mut dyn FnMut(&str) -> Option<Rc<Snapshot>>) -> Value {
    let mut it = p.clone();
    let label = p.get("label").and_then(Value::as_str).unwrap_or("");
    let uri = p
        .get("data")
        .and_then(|d| d.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let bare = label.rsplit("::").next().unwrap_or(label);
    if let Some(s) = snapshot_of(uri)
        && let Some(d) =
            s.an.index
                .defs
                .iter()
                .find(|d| d.name == bare && d.kind != DefKind::TypeParam && !d.name.is_empty())
    {
        let sig = signature(&s, d);
        let doc = docs::doc_comment(&s.src, &s.an.comments, d.full_span.start as usize)
            .unwrap_or_default();
        it["documentation"] =
            json!({"kind": "markdown", "value": format!("```pebble\n{sig}\n```\n\n{doc}")});
    } else if let Some((sig, _, doc)) = builtin_info(bare) {
        it["documentation"] =
            json!({"kind": "markdown", "value": format!("```pebble\nfn {sig}\n```\n\n{doc}")});
    } else if builtin_names().contains(&bare) {
        it["documentation"] = json!({"kind": "markdown", "value": "built-in"});
    }
    it
}
