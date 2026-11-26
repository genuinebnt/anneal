//! Source text bookkeeping: byte [`Span`]s, a [`SourceFile`], and a [`LineIndex`]
//! that converts byte offsets to (line, column) in UTF-8, char and UTF-16 units.
//!
//! WHY three column units: the compiler works in bytes (cheap slicing), humans
//! read characters, and the Language Server Protocol counts UTF-16 code units.

use std::fmt;

/// A half-open byte range `start..end` into a source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        let start = start.min(u32::MAX as usize) as u32;
        let end = (end.min(u32::MAX as usize) as u32).max(start);
        Span { start, end }
    }
    /// An empty span at `at`.
    pub fn point(at: usize) -> Span {
        Span::new(at, at)
    }
    pub fn len(&self) -> usize {
        (self.end - self.start) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    /// True if the byte at `offset` lies inside the span.
    pub fn contains(&self, offset: usize) -> bool {
        (self.start as usize) <= offset && offset < self.end as usize
    }
    /// True if `other` lies entirely inside `self`.
    pub fn contains_span(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }
    /// Smallest span covering both.
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
    /// The text of the span; empty if the span is out of range or splits a char.
    pub fn slice(self, src: &str) -> &str {
        src.get(self.start as usize..self.end as usize)
            .unwrap_or("")
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// A zero-based line/column pair. The unit of `col` depends on the method used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineCol {
    pub line: usize,
    pub col: usize,
}

/// Start offsets of every line, enabling O(log n) offset -> line lookups.
#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    line_starts: Vec<u32>,
}

/// Clamp `offset` into `text` and back up to a char boundary (never panics).
fn clamp(text: &str, offset: usize) -> usize {
    let mut o = offset.min(text.len());
    while !text.is_char_boundary(o) {
        o -= 1;
    }
    o
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1).min(u32::MAX as usize) as u32);
            }
        }
        LineIndex { line_starts }
    }
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
    /// Zero-based line containing `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        match self
            .line_starts
            .binary_search(&(offset.min(u32::MAX as usize) as u32))
        {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts
            .get(line)
            .copied()
            .unwrap_or(*self.line_starts.last().unwrap_or(&0)) as usize
    }
    /// Byte range of a line, excluding the trailing `\n` / `\r\n`.
    pub fn line_range(&self, text: &str, line: usize) -> (usize, usize) {
        let start = self.line_start(line);
        let mut end = match self.line_starts.get(line + 1) {
            Some(&n) => n as usize - 1,
            None => text.len(),
        };
        if end > start && text.as_bytes().get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
        (start.min(text.len()), end.max(start).min(text.len()))
    }
    pub fn line_text<'a>(&self, text: &'a str, line: usize) -> &'a str {
        let (s, e) = self.line_range(text, line);
        text.get(s..e).unwrap_or("")
    }
    /// Column counted in UTF-8 bytes.
    pub fn line_col_utf8(&self, text: &str, offset: usize) -> LineCol {
        let o = clamp(text, offset);
        let line = self.line_of(o);
        LineCol {
            line,
            col: o - self.line_start(line),
        }
    }
    /// Column counted in Unicode scalar values.
    pub fn line_col_chars(&self, text: &str, offset: usize) -> LineCol {
        let o = clamp(text, offset);
        let line = self.line_of(o);
        let s = self.line_start(line);
        LineCol {
            line,
            col: text[s..o].chars().count(),
        }
    }
    /// Column counted in UTF-16 code units (what LSP calls a `character`).
    pub fn line_col_utf16(&self, text: &str, offset: usize) -> LineCol {
        let o = clamp(text, offset);
        let line = self.line_of(o);
        let s = self.line_start(line);
        LineCol {
            line,
            col: text[s..o].chars().map(char::len_utf16).sum(),
        }
    }
    /// Inverse of [`line_col_utf16`](Self::line_col_utf16). Columns past the end
    /// of the line clamp to the line end; a column inside a surrogate pair
    /// rounds down to the start of that char.
    pub fn offset_from_utf16(&self, text: &str, line: usize, col: usize) -> Option<usize> {
        if line >= self.line_count() {
            return None;
        }
        let (s, e) = self.line_range(text, line);
        let mut units = 0;
        for (i, c) in text[s..e].char_indices() {
            if units + c.len_utf16() > col {
                return Some(s + i);
            }
            units += c.len_utf16();
        }
        Some(e)
    }
}

/// A named source text together with its line index.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
    pub index: LineIndex,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> SourceFile {
        let text = text.into();
        let index = LineIndex::new(&text);
        SourceFile {
            name: name.into(),
            text,
            index,
        }
    }
    pub fn slice(&self, span: Span) -> &str {
        span.slice(&self.text)
    }
    pub fn line_col_utf8(&self, offset: usize) -> LineCol {
        self.index.line_col_utf8(&self.text, offset)
    }
    pub fn line_col_chars(&self, offset: usize) -> LineCol {
        self.index.line_col_chars(&self.text, offset)
    }
    pub fn line_col_utf16(&self, offset: usize) -> LineCol {
        self.index.line_col_utf16(&self.text, offset)
    }
    pub fn line_text(&self, line: usize) -> &str {
        self.index.line_text(&self.text, line)
    }
}
