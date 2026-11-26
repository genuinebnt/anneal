//! Syntax highlighting.
//!
//! Two layers: [`lexical`] classifies every byte of the file from the token
//! stream alone (works on broken code, no analysis needed); [`semantic`] refines
//! identifiers using the [`SemanticIndex`](crate::analysis::sema::SemanticIndex) so a
//! function, a local, a parameter and a struct field each get their own class.
//!
//! INVARIANT (tested): the returned spans are contiguous, start at 0, end at
//! `src.len()`, and their texts concatenate to the source.

use crate::analysis::Analysis;
use crate::analysis::sema::{DefId, DefInfo, DefKind};
use crate::analysis::source::Span;
use crate::analysis::syntax::{CommentKind, TokenKind, lex};
use crate::docs;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Whitespace,
    Keyword,
    ControlKeyword,
    Type,
    Function,
    Variable,
    Parameter,
    Constant,
    EnumVariant,
    Property,
    String,
    Escape,
    Number,
    Operator,
    Punctuation,
    Comment,
    DocComment,
    /// Modifier-like keywords (`mut`). PEBBLE has no attribute syntax; the class is
    /// kept so themes written for other languages still have a slot to colour.
    Attribute,
    Error,
}

impl Class {
    pub const ALL: [Class; 19] = [
        Class::Whitespace,
        Class::Keyword,
        Class::ControlKeyword,
        Class::Type,
        Class::Function,
        Class::Variable,
        Class::Parameter,
        Class::Constant,
        Class::EnumVariant,
        Class::Property,
        Class::String,
        Class::Escape,
        Class::Number,
        Class::Operator,
        Class::Punctuation,
        Class::Comment,
        Class::DocComment,
        Class::Attribute,
        Class::Error,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Class::Whitespace => "whitespace",
            Class::Keyword => "keyword",
            Class::ControlKeyword => "control-keyword",
            Class::Type => "type",
            Class::Function => "function",
            Class::Variable => "variable",
            Class::Parameter => "parameter",
            Class::Constant => "constant",
            Class::EnumVariant => "enum-variant",
            Class::Property => "property",
            Class::String => "string",
            Class::Escape => "escape",
            Class::Number => "number",
            Class::Operator => "operator",
            Class::Punctuation => "punctuation",
            Class::Comment => "comment",
            Class::DocComment => "doc-comment",
            Class::Attribute => "attribute",
            Class::Error => "error",
        }
    }

    /// 256-colour palette index used by the ANSI renderer.
    fn ansi(self) -> Option<u8> {
        Some(match self {
            Class::Keyword => 176,
            Class::ControlKeyword => 204,
            Class::Type => 79,
            Class::Function => 111,
            Class::Parameter => 216,
            Class::Constant => 179,
            Class::EnumVariant => 150,
            Class::Property => 117,
            Class::String => 107,
            Class::Escape => 215,
            Class::Number => 173,
            Class::Operator => 145,
            Class::Comment => 244,
            Class::DocComment => 108,
            Class::Attribute => 139,
            Class::Error => 196,
            Class::Whitespace | Class::Variable | Class::Punctuation => return None,
        })
    }
}

/// Modifier bits attached to a semantic token.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    /// A `mut` local or parameter.
    pub mutable: bool,
    /// The defining occurrence of a name.
    pub declaration: bool,
    /// Never used (rendered faded).
    pub unused: bool,
    /// The definition's doc comment says it is deprecated (rendered faded).
    pub deprecated: bool,
    /// A built-in function.
    pub builtin: bool,
}

impl Mods {
    pub fn names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        for (on, n) in [
            (self.mutable, "mutable"),
            (self.declaration, "declaration"),
            (self.unused, "unused"),
            (self.deprecated, "deprecated"),
            (self.builtin, "builtin"),
        ] {
            if on {
                v.push(n);
            }
        }
        v
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HlToken {
    pub span: Span,
    pub class: Class,
    pub mods: Mods,
}

fn keyword_class(k: &TokenKind) -> Class {
    use TokenKind::*;
    match k {
        If | Else | Match | While | Loop | Break | Continue | Return => Class::ControlKeyword,
        Mut => Class::Attribute,
        _ => Class::Keyword,
    }
}

fn is_operator(k: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        k,
        Arrow
            | FatArrow
            | Eq
            | EqEq
            | NotEq
            | Lt
            | Le
            | Gt
            | Ge
            | Plus
            | Minus
            | Star
            | Slash
            | Percent
            | PlusEq
            | MinusEq
            | StarEq
            | SlashEq
            | PercentEq
            | AndAnd
            | OrOr
            | Bang
            | Pipe
            | DotDot
    )
}

const BUILTIN_TYPES: [&str; 3] = ["int", "bool", "str"];

/// Split a string token into `String` and `Escape` pieces.
fn string_pieces(src: &str, sp: Span, out: &mut Vec<HlToken>) {
    let text = sp.slice(src);
    let base = sp.start as usize;
    let mut piece_start = 0usize;
    let mut chars = text.char_indices().peekable();
    let push = |a: usize, b: usize, class: Class, out: &mut Vec<HlToken>| {
        if a < b {
            out.push(HlToken {
                span: Span::new(base + a, base + b),
                class,
                mods: Mods::default(),
            });
        }
    };
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            continue;
        }
        push(piece_start, i, Class::String, out);
        let mut end = i + 1;
        if let Some(&(j, n)) = chars.peek() {
            chars.next();
            end = j + n.len_utf8();
            if n == 'u' && text[end..].starts_with('{') {
                if let Some(close) = text[end..].find('}') {
                    end += close + 1;
                    while chars.peek().is_some_and(|&(k, _)| k < end) {
                        chars.next();
                    }
                }
            }
        }
        push(i, end, Class::Escape, out);
        piece_start = end;
    }
    push(piece_start, text.len(), Class::String, out);
}

/// Classify every byte of `src` from tokens and comments only.
pub fn lexical(src: &str) -> Vec<HlToken> {
    let (toks, comments, _) = lex(src);
    let mut items: Vec<HlToken> = Vec::with_capacity(toks.len() + comments.len());
    let sig: Vec<&crate::analysis::syntax::Token> =
        toks.iter().filter(|t| t.kind != TokenKind::Eof).collect();
    for (i, t) in sig.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| &sig[j].kind);
        let next = sig.get(i + 1).map(|t| &t.kind);
        let class = match &t.kind {
            k if k.is_keyword() => keyword_class(k),
            TokenKind::Ident => ident_class(t.span.slice(src), prev, next),
            TokenKind::Underscore => Class::Keyword,
            TokenKind::Int(_) => Class::Number,
            TokenKind::Str(_) => {
                string_pieces(src, t.span, &mut items);
                continue;
            }
            TokenKind::Error => Class::Error,
            k if is_operator(k) => Class::Operator,
            _ => Class::Punctuation,
        };
        items.push(HlToken {
            span: t.span,
            class,
            mods: Mods::default(),
        });
    }
    for c in &comments {
        let class = if c.kind == CommentKind::DocLine {
            Class::DocComment
        } else {
            Class::Comment
        };
        items.push(HlToken {
            span: c.span,
            class,
            mods: Mods::default(),
        });
    }
    items.sort_by_key(|t| (t.span.start, t.span.end));
    fill_gaps(src, items)
}

fn ident_class(name: &str, prev: Option<&TokenKind>, next: Option<&TokenKind>) -> Class {
    if prev == Some(&TokenKind::ColonColon) {
        return Class::EnumVariant;
    }
    if prev == Some(&TokenKind::Fn) || next == Some(&TokenKind::LParen) {
        return Class::Function;
    }
    if prev.is_some_and(|k| matches!(k, TokenKind::Struct | TokenKind::Enum))
        || BUILTIN_TYPES.contains(&name)
    {
        return Class::Type;
    }
    if name.chars().next().is_some_and(char::is_uppercase) {
        return Class::Type;
    }
    Class::Variable
}

/// Insert `Whitespace` tokens for the gaps so the result tiles the whole file.
fn fill_gaps(src: &str, items: Vec<HlToken>) -> Vec<HlToken> {
    let mut out = Vec::with_capacity(items.len() * 2);
    let mut at = 0usize;
    for t in items {
        if (t.span.start as usize) > at {
            out.push(HlToken {
                span: Span::new(at, t.span.start as usize),
                class: Class::Whitespace,
                mods: Mods::default(),
            });
        }
        at = t.span.end as usize;
        out.push(t);
    }
    if at < src.len() {
        out.push(HlToken {
            span: Span::new(at, src.len()),
            class: Class::Whitespace,
            mods: Mods::default(),
        });
    }
    out
}

// ---------------------------------------------------------------- semantic

/// Sorted table from identifier spans to the definition they name or declare;
/// lookups are O(log n) (the index's own `def_at` scans linearly).
#[derive(Clone, Debug, Default)]
pub struct NameTable {
    entries: Vec<(u32, u32, DefId)>,
}

impl NameTable {
    pub fn new(an: &Analysis) -> NameTable {
        let mut entries: Vec<(u32, u32, DefId)> = an
            .index
            .use_sites()
            .iter()
            .map(|(s, d)| (s.start, s.end, *d))
            .collect();
        entries.extend(
            an.index
                .defs
                .iter()
                .filter(|d| !d.name.is_empty() && !d.span.is_empty())
                .map(|d| (d.span.start, d.span.end, d.id)),
        );
        entries.sort_by_key(|e| (e.0, e.1));
        entries.dedup_by_key(|e| (e.0, e.1));
        NameTable { entries }
    }

    /// The definition whose name (or use) covers byte `offset`.
    pub fn lookup(&self, offset: usize) -> Option<(Span, DefId)> {
        let o = offset as u32;
        let i = self.entries.partition_point(|e| e.0 <= o);
        self.entries[..i]
            .iter()
            .rev()
            .take(2)
            .find(|e| e.0 <= o && o < e.1)
            .map(|e| {
                (
                    Span {
                        start: e.0,
                        end: e.1,
                    },
                    e.2,
                )
            })
    }
}

fn class_of_def(d: &DefInfo) -> Class {
    match d.kind {
        DefKind::Fn => Class::Function,
        DefKind::Struct | DefKind::Enum | DefKind::TypeParam => Class::Type,
        DefKind::Variant => Class::EnumVariant,
        DefKind::Field => Class::Property,
        DefKind::Const => Class::Constant,
        DefKind::Local => Class::Variable,
        DefKind::Param => Class::Parameter,
    }
}

/// Spans of names the checker reported as unused (W0501, W0502, W0507, W0508).
fn unused_spans(an: &Analysis) -> HashSet<Span> {
    an.diagnostics
        .iter()
        .filter(|d| matches!(d.code, "W0501" | "W0502" | "W0507" | "W0508"))
        .filter_map(|d| d.primary_span())
        .collect()
}

/// Definitions whose doc comment declares them deprecated.
fn deprecated_defs(src: &str, an: &Analysis) -> HashSet<DefId> {
    an.index
        .symbols()
        .iter()
        .filter(|s| {
            docs::doc_comment(src, &an.comments, s.full_span.start as usize)
                .is_some_and(|d| docs::is_deprecated(&d))
        })
        .map(|s| s.def)
        .collect()
}

/// Lexical classes refined with what the checker knows about each identifier.
pub fn semantic(src: &str, an: &Analysis) -> Vec<HlToken> {
    let table = NameTable::new(an);
    let unused = unused_spans(an);
    let deprecated = deprecated_defs(src, an);
    let mut toks = lexical(src);
    for t in toks.iter_mut() {
        if !matches!(
            t.class,
            Class::Variable | Class::Function | Class::Type | Class::EnumVariant
        ) {
            continue;
        }
        if let Some((sp, def)) = table
            .lookup(t.span.start as usize)
            .filter(|(sp, _)| *sp == t.span)
        {
            let d = an.index.def(def);
            t.class = class_of_def(d);
            t.mods = Mods {
                mutable: d.mutable && matches!(d.kind, DefKind::Local | DefKind::Param),
                declaration: sp == d.span,
                unused: sp == d.span && unused.contains(&sp),
                deprecated: deprecated.contains(&def),
                builtin: false,
            };
        } else if t.class == Class::Function
            && crate::analysis::sema::Builtin::from_name(t.span.slice(src)).is_some()
        {
            t.mods.builtin = true;
        }
    }
    toks
}

// ---------------------------------------------------------------- renderers

pub fn render_ansi(src: &str, toks: &[HlToken]) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    for t in toks {
        let text = t.span.slice(src);
        let mut sgr = String::new();
        if let Some(c) = t.class.ansi() {
            sgr.push_str(&format!("\x1b[38;5;{c}m"));
        }
        if t.class == Class::Comment || t.class == Class::DocComment {
            sgr.push_str("\x1b[3m");
        }
        if t.mods.unused || t.mods.deprecated {
            sgr.push_str("\x1b[2m");
        }
        if t.mods.mutable {
            sgr.push_str("\x1b[4m");
        }
        if t.class == Class::Error {
            sgr.push_str("\x1b[1m");
        }
        if sgr.is_empty() || t.class == Class::Whitespace {
            out.push_str(text);
        } else {
            out.push_str(&sgr);
            out.push_str(text);
            out.push_str("\x1b[0m");
        }
    }
    out
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const LIGHT: [(Class, &str); 16] = [
    (Class::Keyword, "#8250df"),
    (Class::ControlKeyword, "#cf222e"),
    (Class::Type, "#0a7d6b"),
    (Class::Function, "#0550ae"),
    (Class::Parameter, "#953800"),
    (Class::Constant, "#7d4e00"),
    (Class::EnumVariant, "#116329"),
    (Class::Property, "#0969da"),
    (Class::String, "#0a3069"),
    (Class::Escape, "#bc4c00"),
    (Class::Number, "#953800"),
    (Class::Operator, "#57606a"),
    (Class::Comment, "#6e7781"),
    (Class::DocComment, "#4d7c56"),
    (Class::Attribute, "#6639ba"),
    (Class::Error, "#cf222e"),
];

const DARK: [(Class, &str); 16] = [
    (Class::Keyword, "#d2a8ff"),
    (Class::ControlKeyword, "#ff7b72"),
    (Class::Type, "#56d4bc"),
    (Class::Function, "#79c0ff"),
    (Class::Parameter, "#ffa657"),
    (Class::Constant, "#e3b341"),
    (Class::EnumVariant, "#7ee787"),
    (Class::Property, "#a5d6ff"),
    (Class::String, "#a5d6ff"),
    (Class::Escape, "#ffa657"),
    (Class::Number, "#f2cc60"),
    (Class::Operator, "#8b949e"),
    (Class::Comment, "#8b949e"),
    (Class::DocComment, "#7ee787"),
    (Class::Attribute, "#d2a8ff"),
    (Class::Error, "#ff7b72"),
];

/// A standalone HTML page with inline CSS; colours follow `prefers-color-scheme`.
pub fn render_html(src: &str, toks: &[HlToken], title: &str) -> String {
    let rules = |table: &[(Class, &str)]| {
        table
            .iter()
            .map(|(c, col)| format!(".c-{}{{color:{col}}}", c.name()))
            .collect::<Vec<_>>()
            .join("")
    };
    let mut body = String::with_capacity(src.len() * 3);
    for t in toks {
        let text = escape_html(t.span.slice(src));
        if t.class == Class::Whitespace || (t.class == Class::Variable && t.mods == Mods::default())
        {
            body.push_str(&text);
            continue;
        }
        let mut cls = format!("c-{}", t.class.name());
        for m in t.mods.names() {
            cls.push_str(&format!(" m-{m}"));
        }
        body.push_str(&format!("<span class=\"{cls}\">{text}</span>"));
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\n\
         :root{{color-scheme:light dark}}\nbody{{margin:0;background:#fff;color:#1f2328}}\npre{{margin:0;padding:16px;font:13px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;overflow:auto}}\n\
         {}\n.c-comment,.c-doc-comment{{font-style:italic}}\n.m-unused,.m-deprecated{{opacity:.55}}\n.m-mutable{{text-decoration:underline}}\n.c-error{{text-decoration:underline wavy}}\n\
         @media (prefers-color-scheme:dark){{body{{background:#0d1117;color:#e6edf3}}{}}}\n</style>\n</head>\n<body>\n<pre class=\"pebble\"><code>{}</code></pre>\n</body>\n</html>\n",
        escape_html(title),
        rules(&LIGHT),
        rules(&DARK),
        body
    )
}

pub fn render_json(src: &str, toks: &[HlToken]) -> String {
    let arr: Vec<serde_json::Value> = toks
        .iter()
        .filter(|t| t.class != Class::Whitespace)
        .map(|t| {
            serde_json::json!({
                "start": t.span.start,
                "end": t.span.end,
                "class": t.class.name(),
                "modifiers": t.mods.names(),
                "text": t.span.slice(src),
            })
        })
        .collect();
    serde_json::to_string_pretty(&arr).unwrap_or_default()
}
