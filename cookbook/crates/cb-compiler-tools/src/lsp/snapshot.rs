//! One analysed version of one document.
//!
//! WHY a snapshot: analysis (parse + check + index) is the expensive step, and
//! every request about the same text can share it. The server keeps the latest
//! snapshot per document and drops it on any edit, so "analysis cached by
//! version" is just `Option<Rc<Snapshot>>` on the document.

use crate::analysis::Analysis;
use crate::analysis::source::{LineIndex, Span};
use crate::eol;
use crate::highlight::NameTable;
use serde_json::{Value, json};

pub struct Snapshot {
    pub uri: String,
    pub version: i64,
    /// The document exactly as the client has it (what edits are measured against).
    pub text: String,
    /// `text` with lone `\r` mapped to `\n` (same length): what the compiler and every
    /// position conversion see. Identical to `text` unless the file uses lone-CR line endings.
    pub src: String,
    pub lines: LineIndex,
    pub an: Analysis,
    pub names: NameTable,
}

impl Snapshot {
    pub fn new(uri: &str, version: i64, text: &str) -> Snapshot {
        let src = eol::analysis_text(text).into_owned();
        let an = crate::analysis::analyze(&src);
        let names = NameTable::new(&an);
        let lines = LineIndex::new(&src);
        Snapshot {
            uri: uri.to_string(),
            version,
            text: text.to_string(),
            src,
            lines,
            an,
            names,
        }
    }

    /// LSP position (`line`, UTF-16 `character`) to a byte offset, clamped into the text.
    pub fn offset(&self, pos: &Value) -> usize {
        let line = pos.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
        let col = pos.get("character").and_then(Value::as_u64).unwrap_or(0) as usize;
        self.lines
            .offset_from_utf16(&self.src, line, col)
            .unwrap_or(self.src.len())
    }

    pub fn position(&self, offset: usize) -> Value {
        let lc = self.lines.line_col_utf16(&self.src, offset);
        json!({"line": lc.line, "character": lc.col})
    }

    pub fn range(&self, span: Span) -> Value {
        json!({"start": self.position(span.start as usize), "end": self.position(span.end as usize)})
    }

    pub fn span_of_range(&self, range: &Value) -> Span {
        let a = self.offset(&range["start"]);
        let b = self.offset(&range["end"]);
        Span::new(a.min(b), a.max(b))
    }

    pub fn location(&self, span: Span) -> Value {
        json!({"uri": self.uri, "range": self.range(span)})
    }

    pub fn text_edit(&self, span: Span, new_text: &str) -> Value {
        json!({"range": self.range(span), "newText": new_text})
    }
}

/// Apply one `contentChanges` entry to `text` (a range edit counted in UTF-16 units,
/// or a full replacement when `range` is absent).
pub fn apply_change(text: &mut String, change: &Value) {
    let Some(new) = change.get("text").and_then(Value::as_str) else {
        return;
    };
    let Some(range) = change.get("range").filter(|r| r.is_object()) else {
        *text = new.to_string();
        return;
    };
    let mapped = eol::analysis_text(text).into_owned();
    let lines = LineIndex::new(&mapped);
    let at = |p: &Value| {
        let line = p.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
        let col = p.get("character").and_then(Value::as_u64).unwrap_or(0) as usize;
        lines
            .offset_from_utf16(&mapped, line, col)
            .unwrap_or(text.len())
    };
    let (a, b) = (at(&range["start"]), at(&range["end"]));
    let (a, b) = (a.min(b), a.max(b));
    text.replace_range(a..b, new);
}
