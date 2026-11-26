//! Navigation features that answer from one snapshot: `documentHighlight`,
//! `selectionRange`, `typeDefinition` and the call hierarchy.

use super::features::walk;
use super::server::{INVALID_PARAMS, RpcResult, rpc_err};
use super::snapshot::Snapshot;
use crate::analysis::sema::{DefId, DefKind, Ty};
use crate::analysis::source::Span;
use crate::analysis::syntax::ast::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

// ---------------------------------------------------------------- documentHighlight

/// Spans written by an assignment (`x = ..`, `x += ..`, `v[i] = ..`, `p.f = ..`).
fn write_spans(s: &Snapshot) -> HashSet<Span> {
    let mut out = HashSet::new();
    walk(s.an.ast.node(), &mut |n| {
        if let Node::Expr(e) = n
            && let ExprKind::Assign { target, .. } = &e.kind
        {
            let mut t: &Expr = target;
            loop {
                match &t.kind {
                    ExprKind::Field(b, id) => {
                        if std::ptr::eq(t, &**target) {
                            out.insert(id.span); // `p.x = ..` writes the field `x` (and the place `p`)
                        }
                        t = b;
                    }
                    ExprKind::Index(b, _) | ExprKind::TupleField(b, _, _) | ExprKind::Paren(b) => {
                        t = b
                    }
                    ExprKind::Path(p) => {
                        if let Some(last) = p.segments.last() {
                            out.insert(last.span);
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
    });
    out
}

/// Every occurrence of the symbol under the cursor: kind 3 (write) for the declaration
/// and assignment targets, kind 2 (read) for the rest.
pub fn document_highlight(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    let Some((_, def)) = s.names.lookup(off) else {
        return Value::Null;
    };
    let d = s.an.index.def(def);
    let writes = write_spans(s);
    Value::Array(
        s.an.index
            .references(def, true)
            .into_iter()
            .map(|sp| json!({"range": s.range(sp), "kind": if sp == d.span || writes.contains(&sp) { 3 } else { 2 }}))
            .collect(),
    )
}

// ---------------------------------------------------------------- selectionRange

/// Spans of the nodes containing `offset`, innermost first, strictly growing, each
/// containing the previous one, ending with the whole file.
pub fn selection_chain(s: &Snapshot, offset: usize) -> Vec<Span> {
    let mut chain: Vec<Span> = vec![Span::new(0, s.src.len())];
    let mut node = s.an.ast.node();
    loop {
        chain.push(node.span());
        let next = node.children().into_iter().find(|c| {
            c.span().start as usize <= offset
                && offset <= c.span().end as usize
                && !c.span().is_empty()
        });
        match next {
            Some(c) if c.span() != node.span() => node = c,
            _ => break,
        }
    }
    chain.reverse(); // innermost first
    let mut out: Vec<Span> = Vec::new();
    for sp in chain {
        match out.last() {
            None => out.push(sp),
            Some(prev) => {
                let merged = prev.to(sp);
                if merged != *prev {
                    out.push(merged);
                }
            }
        }
    }
    out
}

pub fn selection_ranges(s: &Snapshot, p: &Value) -> Value {
    let positions = p
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        positions
            .iter()
            .map(|pos| {
                let off = s.offset(pos);
                let chain = selection_chain(s, off);
                let mut parent = Value::Null;
                for sp in chain.iter().rev() {
                    let mut v = json!({"range": s.range(*sp)});
                    if !parent.is_null() {
                        v["parent"] = parent;
                    }
                    parent = v;
                }
                parent
            })
            .collect(),
    )
}

// ---------------------------------------------------------------- typeDefinition

fn adt_of(t: &Ty) -> Option<DefId> {
    match t {
        Ty::Adt(d, ..) => Some(*d),
        Ty::Array(inner) => adt_of(inner),
        Ty::Fn(_, ret) => adt_of(ret),
        _ => None,
    }
}

pub fn type_definition(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    let target = if let Some((_, def)) = s.names.lookup(off) {
        let d = s.an.index.def(def);
        match d.kind {
            DefKind::Struct | DefKind::Enum => Some(def),
            DefKind::Variant => d.parent,
            _ => d.ty.as_ref().and_then(adt_of),
        }
    } else {
        s.an.index.type_at(off).and_then(adt_of)
    };
    match target.map(|t| s.an.index.def(t)) {
        Some(d) if !d.span.is_empty() => s.location(d.span),
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------- call hierarchy

fn fn_item(s: &Snapshot, def: DefId) -> Option<Value> {
    let d = s.an.index.def(def);
    if d.kind != DefKind::Fn || d.span.is_empty() {
        return None;
    }
    let detail = d.ty.as_ref().map(|t| t.to_string()).unwrap_or_default();
    Some(json!({
        "name": d.name, "kind": 12, "detail": detail, "uri": s.uri,
        "range": s.range(d.full_span), "selectionRange": s.range(d.span),
        "data": {"def": def.0}
    }))
}

/// The function definition that the hierarchy item (or position) denotes.
fn def_of_item(s: &Snapshot, item: &Value) -> Option<DefId> {
    let off = s.offset(&item["selectionRange"]["start"]);
    s.names
        .lookup(off)
        .map(|(_, d)| d)
        .filter(|d| s.an.index.def(*d).kind == DefKind::Fn)
}

pub fn prepare_call_hierarchy(s: &Snapshot, p: &Value) -> Value {
    let off = s.offset(&p["position"]);
    match s.names.lookup(off).and_then(|(_, d)| fn_item(s, d)) {
        Some(item) => json!([item]),
        None => Value::Null,
    }
}

/// (callee path span, enclosing fn def, call expression) for every `f(..)` call in the file.
fn calls(s: &Snapshot) -> Vec<(Span, Option<DefId>, Span)> {
    let mut out = Vec::new();
    for item in &s.an.ast.items {
        let ItemKind::Fn(f) = &item.kind else {
            continue;
        };
        let owner = s.names.lookup(f.name.span.start as usize).map(|(_, d)| d);
        walk(Node::Item(item), &mut |n| {
            if let Node::Expr(e) = n
                && let ExprKind::Call { callee, .. } = &e.kind
                && let ExprKind::Path(path) = &callee.kind
                && let Some(last) = path.segments.last()
            {
                out.push((last.span, owner, e.span));
            }
        });
    }
    out
}

pub fn incoming_calls(s: &Snapshot, p: &Value) -> RpcResult {
    let Some(def) = def_of_item(s, &p["item"]) else {
        return Err(rpc_err(INVALID_PARAMS, "not a function"));
    };
    let refs: HashSet<Span> = s.an.index.references(def, false).into_iter().collect();
    let mut by_caller: BTreeMap<u32, (DefId, Vec<Span>)> = BTreeMap::new();
    for (callee_span, owner, _) in calls(s) {
        if !refs.contains(&callee_span) {
            continue;
        }
        let Some(owner) = owner else { continue };
        by_caller
            .entry(s.an.index.def(owner).span.start)
            .or_insert((owner, vec![]))
            .1
            .push(callee_span);
    }
    Ok(Value::Array(
        by_caller
            .into_values()
            .filter_map(|(owner, spans)| {
                let item = fn_item(s, owner)?;
                Some(json!({"from": item, "fromRanges": spans.into_iter().map(|sp| s.range(sp)).collect::<Vec<_>>()}))
            })
            .collect(),
    ))
}

pub fn outgoing_calls(s: &Snapshot, p: &Value) -> RpcResult {
    let Some(def) = def_of_item(s, &p["item"]) else {
        return Err(rpc_err(INVALID_PARAMS, "not a function"));
    };
    let mut by_target: BTreeMap<u32, (DefId, Vec<Span>)> = BTreeMap::new();
    for (callee_span, owner, _) in calls(s) {
        if owner != Some(def) {
            continue;
        }
        let Some((_, target)) = s.names.lookup(callee_span.start as usize) else {
            continue;
        };
        if s.an.index.def(target).kind != DefKind::Fn {
            continue;
        }
        by_target
            .entry(s.an.index.def(target).span.start)
            .or_insert((target, vec![]))
            .1
            .push(callee_span);
    }
    Ok(Value::Array(
        by_target
            .into_values()
            .filter_map(|(target, spans)| {
                let item = fn_item(s, target)?;
                Some(json!({"to": item, "fromRanges": spans.into_iter().map(|sp| s.range(sp)).collect::<Vec<_>>()}))
            })
            .collect(),
    ))
}
