//! The PEBBLE formatter: parse, build a [`doc::Doc`] from the AST (re-attaching
//! comments from the lexer's side table), print at 100 columns.
//!
//! Policy for broken input: [`format`] returns [`FormatError::Syntax`] when the
//! file has any lexer or parser error. A formatter that guesses at error regions
//! risks destroying code the user is still typing, and the editor already shows
//! the diagnostics, so the honest answer is "fix the errors first".
//!
//! Safety net: the result is re-parsed; if it has errors or a different number
//! of comments, [`FormatError::Internal`] is returned instead of the bad text.
//!
//! Line endings: output uses the input's dominant terminator (`\n`, `\r\n` or a
//! lone `\r`, see [`crate::eol`]).
//!
//! BEST-EFFORT MODE ([`format_best_effort`], [`format_range`], [`format_on_type`]),
//! used by the language server while a file does not parse, works item by item:
//! a top-level item is re-printed only if (1) no error diagnostic touches its span,
//! (2) its own text parses without errors when taken alone, and (3) the formatted
//! item re-parses to the same AST. Everything else (error items, stray tokens,
//! the gaps between items with their comments and blank lines) stays BYTE-FOR-BYTE.
//! The strict [`format`] (used by `pebble-tools fmt --check`) never guesses.

mod build;
pub mod doc;

use crate::analysis::diag::Diagnostic;
use crate::analysis::syntax::ast::ItemKind;
use crate::analysis::syntax::{CommentKind, parse};
use crate::eol::{self, Eol};

/// Target line width.
pub const WIDTH: usize = 100;
/// Spaces per indentation level.
pub const INDENT: usize = 4;

#[derive(Debug)]
pub enum FormatError {
    /// The input does not parse; carries the parser's diagnostics.
    Syntax(Vec<Diagnostic>),
    /// A bug guard: the formatted text failed to re-parse or lost a comment.
    Internal(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Syntax(d) => write!(
                f,
                "cannot format: {} syntax error(s), first: {}",
                d.len(),
                d.first().map(|d| d.message.as_str()).unwrap_or("?")
            ),
            FormatError::Internal(m) => write!(f, "formatter bug: {m}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// Format PEBBLE source text (strict: any syntax error is refused).
pub fn format(src: &str) -> Result<String, FormatError> {
    let lf = eol::to_lf(src);
    let out = format_lf(&lf)?;
    Ok(eol::from_lf(&out, eol::detect(src)))
}

fn format_lf(lf: &str) -> Result<String, FormatError> {
    let out = format_lf_unchecked(lf)?;
    let again = parse(&out);
    if again.diagnostics.iter().any(|d| d.is_error())
        || again.comments.len() != parse(lf).comments.len()
    {
        return Err(FormatError::Internal(
            "output does not re-parse with the same comments".into(),
        ));
    }
    Ok(out)
}

/// [`format`] without the re-parse safety net (used to debug the formatter itself).
pub fn format_unchecked(src: &str) -> Result<String, FormatError> {
    let lf = eol::to_lf(src);
    Ok(eol::from_lf(&format_lf_unchecked(&lf)?, eol::detect(src)))
}

fn format_lf_unchecked(src: &str) -> Result<String, FormatError> {
    let p = parse(src);
    let errors: Vec<Diagnostic> = p
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .cloned()
        .collect();
    if !errors.is_empty() || p.fuel_exhausted {
        return Err(FormatError::Syntax(errors));
    }
    let doc = build::module_doc(src, &p.ast, &p.comments);
    let mut out = doc::print(&doc, WIDTH);
    out.truncate(out.trim_end().len());
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// The AST of `src` rendered with every span and node id erased: two programs
/// with equal fingerprints are the same program modulo layout.
pub fn ast_fingerprint(src: &str) -> String {
    erase(&format!("{:?}", parse(&eol::analysis_text(src)).ast))
}

/// Remove `Span { .. }` and `NodeId(..)` payloads from a derived `Debug` string.
fn erase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        let next = ["Span {", "NodeId("]
            .iter()
            .filter_map(|p| rest.find(p).map(|i| (i, *p)))
            .min();
        let Some((i, pat)) = next else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..i]);
        let close = if pat == "NodeId(" { ')' } else { '}' };
        let tail = &rest[i..];
        let end = tail.find(close).map(|e| e + 1).unwrap_or(tail.len());
        out.push_str(if pat == "NodeId(" { "_id" } else { "_span" });
        rest = &tail[end..];
    }
    out
}

/// The smallest single edit turning `old` into `new`, as `(start, end, replacement)`
/// in byte offsets of `old`, found by trimming the common prefix and suffix.
/// `None` when the texts are equal.
pub fn minimal_edit(old: &str, new: &str) -> Option<(usize, usize, String)> {
    if old == new {
        return None;
    }
    let pre = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let pre = (0..=pre)
        .rev()
        .find(|&i| old.is_char_boundary(i) && new.is_char_boundary(i))
        .unwrap_or(0);
    let max_suf = old.len().min(new.len()) - pre;
    let suf = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suf)
        .take_while(|(a, b)| a == b)
        .count();
    let suf = (0..=suf)
        .rev()
        .find(|&i| old.is_char_boundary(old.len() - i) && new.is_char_boundary(new.len() - i))
        .unwrap_or(0);
    Some((pre, old.len() - suf, new[pre..new.len() - suf].to_string()))
}

// ---------------------------------------------------------------- best effort, ranges, on-type

/// Replace `start..end` (byte offsets of the ORIGINAL text) by `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Apply non-overlapping replacements (any order).
pub fn apply_replacements(src: &str, reps: &[Replacement]) -> String {
    let mut sorted: Vec<&Replacement> = reps.iter().collect();
    sorted.sort_by_key(|r| r.start);
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for r in sorted {
        out.push_str(&src[at..r.start]);
        out.push_str(&r.text);
        at = r.end;
    }
    out.push_str(&src[at..]);
    out
}

/// Format one item's text on its own; `None` when it does not parse alone or the
/// result is not provably the same program.
fn format_item_text(text: &str, eol: Eol) -> Option<String> {
    let lf = eol::to_lf(text);
    let out = format_lf(&lf).ok()?;
    let out = out.trim_end_matches('\n');
    if ast_fingerprint(&lf) != ast_fingerprint(out) {
        return None;
    }
    Some(eol::from_lf(out, eol))
}

/// Replacements for every error-free top-level item that overlaps `range` (all items
/// when `None`) and whose formatting differs from its text. See the module docs for
/// the rule; the replaced regions are minimal and never include text between items.
pub fn format_items(src: &str, range: Option<(usize, usize)>) -> Vec<Replacement> {
    let mapped = eol::analysis_text(src);
    let p = parse(&mapped);
    if p.fuel_exhausted {
        return vec![];
    }
    let errors: Vec<(usize, usize)> = p
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .flat_map(|d| {
            d.labels
                .iter()
                .map(|l| (l.span.start as usize, l.span.end as usize))
        })
        .collect();
    let eol = eol::detect(src);
    let mut out = Vec::new();
    for item in &p.ast.items {
        let (a, b) = (item.span.start as usize, item.span.end as usize);
        if matches!(item.kind, ItemKind::Error)
            || a >= b
            || b > src.len()
            || !src.is_char_boundary(a)
            || !src.is_char_boundary(b)
        {
            continue;
        }
        if let Some((rs, re)) = range {
            let hit = if rs >= re {
                a <= rs && rs <= b
            } else {
                a < re && rs < b
            };
            if !hit {
                continue;
            }
        }
        if errors.iter().any(|&(es, ee)| es <= b && a <= ee) {
            continue;
        }
        let old = &src[a..b];
        let Some(mut new) = format_item_text(old, eol) else {
            continue;
        };
        // `format_item_text` only proves the item right in isolation; the splice must
        // also be right in context. The builder may move a comment from inside the item
        // to after its last token, and a `//` comment would then swallow whatever
        // follows the item on the same line, so it has to be terminated.
        let same_line_rest = mapped[b..].split('\n').next().unwrap_or("");
        if ends_in_line_comment(&new) && !same_line_rest.trim().is_empty() {
            new.push_str(eol.as_str());
        }
        if let Some((x, y, text)) = minimal_edit(old, &new) {
            out.push(Replacement {
                start: a + x,
                end: a + y,
                text,
            });
        }
    }
    // The same safety net as strict mode, on the whole spliced file: never hand the
    // editor edits that change the program or lose a comment.
    if !out.is_empty() && !same_program(src, &apply_replacements(src, &out)) {
        return vec![];
    }
    out
}

fn ends_in_line_comment(text: &str) -> bool {
    let lf = eol::to_lf(text);
    parse(&lf)
        .comments
        .last()
        .is_some_and(|c| c.kind != CommentKind::Block && c.span.end as usize == lf.len())
}

/// Same AST modulo layout, and the same number of comments.
fn same_program(a: &str, b: &str) -> bool {
    ast_fingerprint(a) == ast_fingerprint(b)
        && parse(&eol::analysis_text(a)).comments.len()
            == parse(&eol::analysis_text(b)).comments.len()
}

/// What [`format_best_effort`] did.
#[derive(Debug, PartialEq, Eq)]
pub struct BestEffort {
    pub text: String,
    /// Items that were re-printed (changed).
    pub reformatted: usize,
}

/// Format every error-free item and keep everything else verbatim.
pub fn format_best_effort(src: &str) -> BestEffort {
    let reps = format_items(src, None);
    BestEffort {
        text: apply_replacements(src, &reps),
        reformatted: reps.len(),
    }
}

/// Range formatting: only the items that overlap `start..end`; the rest of the text is untouched.
pub fn format_range(src: &str, start: usize, end: usize) -> Vec<Replacement> {
    format_items(src, Some((start.min(end), start.max(end))))
}

/// Format-on-type for `}` and `;`: after the character just before `offset` was typed,
/// re-print the item containing it (only if that item is error-free).
pub fn format_on_type(src: &str, offset: usize, ch: char) -> Vec<Replacement> {
    if ch != '}' && ch != ';' {
        return vec![];
    }
    let end = offset.min(src.len());
    let mut start = end.saturating_sub(1);
    while start > 0 && !src.is_char_boundary(start) {
        start -= 1;
    }
    if start == end {
        return vec![];
    }
    format_items(src, Some((start, end)))
}
