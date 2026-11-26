//! The semantic index: everything an editor (LSP) wants to ask about a checked
//! file, answered from tables built during checking.
//!
//! * [`SemanticIndex::node_at`], [`type_at`](SemanticIndex::type_at): what is under the cursor?
//! * [`definition_at`](SemanticIndex::definition_at), [`references`](SemanticIndex::references): go-to-definition / find-references.
//! * [`symbols`](SemanticIndex::symbols): the outline.
//! * [`completions_at`](SemanticIndex::completions_at): names visible at an offset.

use super::hir::{Builtin, DefInfo, DefKind};
use super::types::{DefId, Ty};
use crate::source::Span;
use crate::syntax::ast::NodeId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexNodeKind {
    Expr,
    Pattern,
}

/// An expression or pattern with its id and span.
#[derive(Clone, Copy, Debug)]
pub struct IndexNode {
    pub id: NodeId,
    pub span: Span,
    pub kind: IndexNodeKind,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub def: DefId,
    pub name: String,
    pub kind: DefKind,
    /// The defining name.
    pub span: Span,
    /// The whole item (for ranges/folding).
    pub full_span: Span,
    pub container: Option<DefId>,
    /// E.g. the function signature or the variable's type.
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Local,
    Param,
    Function,
    Const,
    Struct,
    Enum,
    Variant,
    Field,
    Builtin,
    Keyword,
}

#[derive(Clone, Debug)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticIndex {
    pub defs: Vec<DefInfo>,
    pub(crate) nodes: Vec<IndexNode>,
    pub(crate) types: HashMap<NodeId, Ty>,
    /// (span of the name at the use site, what it refers to)
    pub(crate) refs: Vec<(Span, DefId)>,
    /// A local `def` is visible in offsets `start..=end`.
    pub(crate) visibility: Vec<(DefId, u32, u32)>,
}

const KEYWORDS: [&str; 14] = [
    "fn", "struct", "enum", "const", "let", "mut", "if", "else", "match", "while", "loop", "break",
    "return", "continue",
];

impl SemanticIndex {
    pub fn def(&self, id: DefId) -> &DefInfo {
        &self.defs[id.0 as usize]
    }

    /// Innermost expression or pattern containing `offset`.
    pub fn node_at(&self, offset: usize) -> Option<IndexNode> {
        self.nodes
            .iter()
            .filter(|n| n.span.contains(offset))
            .min_by_key(|n| n.span.len())
            .copied()
    }

    pub fn type_of(&self, id: NodeId) -> Option<&Ty> {
        self.types.get(&id)
    }

    /// The type of the innermost typed node under `offset` (what a hover shows).
    pub fn type_at(&self, offset: usize) -> Option<&Ty> {
        self.node_at(offset).and_then(|n| self.types.get(&n.id))
    }

    /// The definition referenced at `offset`, or the definition whose name is at `offset`.
    pub fn def_at(&self, offset: usize) -> Option<DefId> {
        self.refs
            .iter()
            .filter(|(s, _)| s.contains(offset))
            .min_by_key(|(s, _)| s.len())
            .map(|(_, d)| *d)
            .or_else(|| {
                self.defs
                    .iter()
                    .find(|d| d.span.contains(offset) && !d.name.is_empty())
                    .map(|d| d.id)
            })
    }

    /// Where the identifier at `offset` is defined.
    pub fn definition_at(&self, offset: usize) -> Option<Span> {
        self.def_at(offset).map(|d| self.def(d).span)
    }

    /// Every (use-site name span, definition) pair, unsorted. Lets tools build their own
    /// offset lookup tables instead of calling the linear `def_at` per token.
    pub fn use_sites(&self) -> &[(Span, DefId)] {
        &self.refs
    }

    /// Every use of `def` (plus its declaration when `include_declaration`), in source order.
    pub fn references(&self, def: DefId, include_declaration: bool) -> Vec<Span> {
        let mut v: Vec<Span> = self
            .refs
            .iter()
            .filter(|(_, d)| *d == def)
            .map(|(s, _)| *s)
            .collect();
        if include_declaration {
            v.push(self.def(def).span);
        }
        v.sort();
        v.dedup();
        v
    }

    /// All named definitions, in source order, for an outline view.
    pub fn symbols(&self) -> Vec<Symbol> {
        let mut out: Vec<Symbol> = self
            .defs
            .iter()
            .filter(|d| !d.name.is_empty() && d.kind != DefKind::TypeParam)
            .map(|d| Symbol {
                def: d.id,
                name: d.name.clone(),
                kind: d.kind,
                span: d.span,
                full_span: d.full_span,
                container: d.parent,
                detail: d.ty.as_ref().map(|t| t.to_string()).unwrap_or_default(),
            })
            .collect();
        out.sort_by_key(|s| (s.full_span.start, std::cmp::Reverse(s.full_span.end)));
        out
    }

    /// Candidates visible at `offset`: locals in scope, items, enum variants, built-ins, keywords.
    pub fn completions_at(&self, offset: usize) -> Vec<Completion> {
        let mut out = Vec::new();
        let off = offset as u32;
        for (def, start, end) in &self.visibility {
            if *start <= off && off <= *end {
                let d = self.def(*def);
                out.push(Completion {
                    label: d.name.clone(),
                    kind: if d.kind == DefKind::Param {
                        CompletionKind::Param
                    } else {
                        CompletionKind::Local
                    },
                    detail: d.ty.as_ref().map(|t| t.to_string()).unwrap_or_default(),
                });
            }
        }
        for d in &self.defs {
            if d.name.is_empty() {
                continue;
            }
            let detail = d.ty.as_ref().map(|t| t.to_string()).unwrap_or_default();
            match d.kind {
                DefKind::Fn => out.push(Completion {
                    label: d.name.clone(),
                    kind: CompletionKind::Function,
                    detail,
                }),
                DefKind::Const => out.push(Completion {
                    label: d.name.clone(),
                    kind: CompletionKind::Const,
                    detail,
                }),
                DefKind::Struct => out.push(Completion {
                    label: d.name.clone(),
                    kind: CompletionKind::Struct,
                    detail,
                }),
                DefKind::Enum => out.push(Completion {
                    label: d.name.clone(),
                    kind: CompletionKind::Enum,
                    detail,
                }),
                DefKind::Variant => {
                    let owner = d
                        .parent
                        .map(|p| self.def(p).name.clone())
                        .unwrap_or_default();
                    out.push(Completion {
                        label: format!("{owner}::{}", d.name),
                        kind: CompletionKind::Variant,
                        detail: owner,
                    })
                }
                _ => {}
            }
        }
        for b in Builtin::ALL {
            out.push(Completion {
                label: b.name().to_string(),
                kind: CompletionKind::Builtin,
                detail: "built-in".into(),
            });
        }
        for k in KEYWORDS {
            out.push(Completion {
                label: k.to_string(),
                kind: CompletionKind::Keyword,
                detail: String::new(),
            });
        }
        out
    }

    /// Fields available after `expr.` when the expression has type `ty`.
    pub fn fields_of(&self, ty: &Ty) -> Vec<Completion> {
        let mut out = Vec::new();
        if let Ty::Adt(def, ..) = ty {
            for d in self
                .defs
                .iter()
                .filter(|d| d.kind == DefKind::Field && d.parent == Some(*def))
            {
                out.push(Completion {
                    label: d.name.clone(),
                    kind: CompletionKind::Field,
                    detail: d.ty.as_ref().map(|t| t.to_string()).unwrap_or_default(),
                });
            }
        }
        out
    }
}
