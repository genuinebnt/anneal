//! Diagnostics: the data model ([`Diagnostic`]), the error-code catalogue
//! ([`codes`]), a rustc-style text renderer and a JSON form ([`render`]), and
//! "did you mean" helpers ([`closest_match`]).
//!
//! Every phase (lexer, parser, checker) reports problems by *returning*
//! `Diagnostic`s instead of failing, so one run can show many errors.

pub mod codes;
pub mod render;

use crate::source::Span;

pub use render::{RenderOptions, apply_suggestions, render, render_all, to_json, to_json_array};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Help => "help",
        }
    }
}

/// A span with an explanatory text. Exactly one label is normally primary
/// (drawn with `^`); the others are secondary context (drawn with `-`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub text: String,
    pub primary: bool,
}

/// A single text replacement. Inserting is an empty span; deleting is an empty replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applicability {
    /// Safe to apply automatically (tools may do so without asking).
    MachineApplicable,
    /// A guess; show it to a human.
    MaybeIncorrect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub message: String,
    pub edits: Vec<Edit>,
    pub applicability: Applicability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable identifier such as `E0301`; see [`codes::CODES`] and `docs/ERRORS.md`.
    pub code: &'static str,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    fn new(severity: Severity, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message: message.into(),
            labels: vec![],
            notes: vec![],
            help: vec![],
            suggestions: vec![],
        }
    }
    pub fn error(code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Error, code, message)
    }
    pub fn warning(code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Warning, code, message)
    }
    pub fn primary(mut self, span: Span, text: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            text: text.into(),
            primary: true,
        });
        self
    }
    pub fn secondary(mut self, span: Span, text: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            text: text.into(),
            primary: false,
        });
        self
    }
    pub fn note(mut self, text: impl Into<String>) -> Diagnostic {
        self.notes.push(text.into());
        self
    }
    pub fn help(mut self, text: impl Into<String>) -> Diagnostic {
        self.help.push(text.into());
        self
    }
    pub fn suggest(
        mut self,
        message: impl Into<String>,
        span: Span,
        replacement: impl Into<String>,
        applicability: Applicability,
    ) -> Diagnostic {
        self.suggestions.push(Suggestion {
            message: message.into(),
            edits: vec![Edit {
                span,
                replacement: replacement.into(),
            }],
            applicability,
        });
        self
    }
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
    /// The span the diagnostic is "about": the first primary label, else the first label.
    pub fn primary_span(&self) -> Option<Span> {
        self.labels
            .iter()
            .find(|l| l.primary)
            .or(self.labels.first())
            .map(|l| l.span)
    }
}

/// Sort by position (stable, so equal positions keep emission order) and drop exact duplicates.
pub fn sort_and_dedup(diags: &mut Vec<Diagnostic>) {
    diags.sort_by_key(|d| d.primary_span().map(|s| (s.start, s.end)).unwrap_or((0, 0)));
    let mut seen: Vec<Diagnostic> = Vec::with_capacity(diags.len());
    for d in diags.drain(..) {
        if !seen.contains(&d) {
            seen.push(d);
        }
    }
    *diags = seen;
}

/// Levenshtein edit distance (insert, delete, substitute; plus adjacent transposition).
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        prev2 = prev;
        prev = cur;
    }
    prev[b.len()]
}

/// The candidate most similar to `name`, if any is close enough to be a plausible typo.
/// A case-insensitive exact match always wins; otherwise the distance must be at most
/// `max(1, len/3)` (the same rule of thumb rustc uses).
pub fn closest_match<'a, I>(name: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let limit = (name.chars().count() / 3).max(1);
    let mut best: Option<(usize, &'a str)> = None;
    for c in candidates {
        if c == name || c.is_empty() {
            continue;
        }
        let d = if c.eq_ignore_ascii_case(name) {
            0
        } else {
            edit_distance(name, c)
        };
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}
