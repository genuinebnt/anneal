//! `textDocument/semanticTokens/full`: the semantic highlighter's output in the
//! LSP wire encoding (five integers per token, positions relative to the previous token).

use super::snapshot::Snapshot;
use crate::highlight::{Class, HlToken, Mods, semantic};
use serde_json::{Value, json};

const TYPES: [&str; 11] = [
    "function",
    "variable",
    "parameter",
    "type",
    "enumMember",
    "property",
    "keyword",
    "string",
    "number",
    "operator",
    "comment",
];
const MODIFIERS: [&str; 6] = [
    "declaration",
    "readonly",
    "mutable",
    "defaultLibrary",
    "deprecated",
    "unused",
];

pub fn legend() -> Value {
    json!({"tokenTypes": TYPES, "tokenModifiers": MODIFIERS})
}

fn type_index(c: Class) -> Option<usize> {
    Some(match c {
        Class::Function => 0,
        Class::Variable | Class::Constant => 1,
        Class::Parameter => 2,
        Class::Type => 3,
        Class::EnumVariant => 4,
        Class::Property => 5,
        Class::Keyword | Class::ControlKeyword | Class::Attribute => 6,
        Class::String | Class::Escape => 7,
        Class::Number => 8,
        Class::Operator => 9,
        Class::Comment | Class::DocComment => 10,
        Class::Whitespace | Class::Punctuation | Class::Error => return None,
    })
}

fn modifier_bits(t: &HlToken) -> u32 {
    let m: Mods = t.mods;
    let flags = [
        m.declaration,
        t.class == Class::Constant,
        m.mutable,
        m.builtin,
        m.deprecated,
        m.unused,
    ];
    flags
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .fold(0, |acc, (i, _)| acc | (1 << i))
}

/// (line, start col, length) in UTF-16 units, split per line (the protocol forbids multi-line tokens).
fn pieces(s: &Snapshot, t: &HlToken) -> Vec<(usize, usize, usize)> {
    let text = t.span.slice(&s.src);
    let base = t.span.start as usize;
    let mut line_start = 0usize;
    let mut out = Vec::new();
    for line in text.split('\n') {
        let trimmed = line.trim_end_matches('\r');
        if !trimmed.is_empty() {
            let lc = s.lines.line_col_utf16(&s.src, base + line_start);
            out.push((lc.line, lc.col, trimmed.chars().map(char::len_utf16).sum()));
        }
        line_start += line.len() + 1;
    }
    out
}

/// One token in absolute coordinates: (line, UTF-16 column, length, type index, modifier bits).
pub type AbsToken = (usize, usize, usize, u32, u32);

pub fn absolute(s: &Snapshot) -> Vec<AbsToken> {
    let toks = semantic(&s.src, &s.an);
    // Merge adjacent string/escape pieces (and equal neighbours) to keep the payload small.
    let mut merged: Vec<(usize, u32, HlToken)> = Vec::new();
    for t in toks {
        let Some(ty) = type_index(t.class) else {
            continue;
        };
        let bits = modifier_bits(&t);
        if let Some((pty, pbits, prev)) = merged.last_mut()
            && *pty == ty
            && *pbits == bits
            && prev.span.end == t.span.start
            && ty == 7
        {
            prev.span.end = t.span.end;
            continue;
        }
        merged.push((ty, bits, t));
    }
    let mut out = Vec::new();
    for (ty, bits, t) in &merged {
        for (line, col, len) in pieces(s, t) {
            out.push((line, col, len, *ty as u32, *bits));
        }
    }
    out
}

/// The wire encoding: five integers per token, positions relative to the previous token.
pub fn encode(toks: &[AbsToken]) -> Vec<u32> {
    let mut data: Vec<u32> = Vec::with_capacity(toks.len() * 5);
    let (mut prev_line, mut prev_col) = (0usize, 0usize);
    for &(line, col, len, ty, bits) in toks {
        let dl = line - prev_line;
        let dc = if dl == 0 { col - prev_col } else { col };
        data.extend([dl as u32, dc as u32, len as u32, ty, bits]);
        (prev_line, prev_col) = (line, col);
    }
    data
}

pub fn full_data(s: &Snapshot) -> Vec<u32> {
    encode(&absolute(s))
}

/// Tokens overlapping the range `start..end` (LSP positions as (line, character)); the
/// encoding is still relative to the start of the document, as the protocol requires.
pub fn range_data(s: &Snapshot, start: (usize, usize), end: (usize, usize)) -> Vec<u32> {
    let kept: Vec<AbsToken> = absolute(s)
        .into_iter()
        .filter(|&(l, c, n, _, _)| (l, c) < end && (l, c + n) > start)
        .collect();
    encode(&kept)
}

/// Apply a `SemanticTokensEdit` list to `old` (what a client does with a delta response).
#[cfg(test)]
pub fn apply_edits(old: &[u32], edits: &[(usize, usize, Vec<u32>)]) -> Vec<u32> {
    let mut out = old.to_vec();
    for (start, delete, data) in edits.iter().rev() {
        let end = (*start + *delete).min(out.len());
        out.splice((*start).min(out.len())..end, data.iter().copied());
    }
    out
}

/// One edit turning `old` into `new` (common prefix and suffix trimmed); `None` if equal.
pub fn diff(old: &[u32], new: &[u32]) -> Option<(usize, usize, Vec<u32>)> {
    if old == new {
        return None;
    }
    let pre = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let max_suf = old.len().min(new.len()) - pre;
    let suf = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suf)
        .take_while(|(a, b)| a == b)
        .count();
    Some((
        pre,
        old.len() - pre - suf,
        new[pre..new.len() - suf].to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_then_apply_is_identity() {
        let cases: [(&[u32], &[u32]); 5] = [
            (&[1, 2, 3], &[1, 2, 3]),
            (&[1, 2, 3], &[1, 9, 3]),
            (&[], &[4, 5]),
            (&[4, 5], &[]),
            (&[1, 2, 3, 4], &[1, 4]),
        ];
        for (old, new) in cases {
            let edits: Vec<_> = diff(old, new).into_iter().collect();
            assert_eq!(apply_edits(old, &edits), new);
        }
    }
}
