//! Line-ending policy shared by the formatter and the language server.
//!
//! Three terminators exist in the wild: `\n`, `\r\n` and a lone `\r` (classic Mac;
//! the LSP spec counts it as a line break too). The compiler only knows `\n`
//! (a `\r` is whitespace), so:
//!
//! * [`analysis_text`] maps every LONE `\r` to `\n`. The mapping keeps the byte length,
//!   so every span, offset and column computed on the mapped text is valid on the
//!   original one. `\r\n` is left alone (the lexer treats `\r` as whitespace and
//!   [`LineIndex`](crate::analysis::source::LineIndex) excludes it from the line).
//! * The formatter works on LF text and writes its output with the file's dominant
//!   terminator ([`detect`]): CRLF stays CRLF, lone CR stays lone CR.

use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eol {
    Lf,
    CrLf,
    Cr,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::CrLf => "\r\n",
            Eol::Cr => "\r",
        }
    }
}

/// The dominant line terminator (ties go to the one that appears first; no terminator: LF).
pub fn detect(text: &str) -> Eol {
    let b = text.as_bytes();
    let mut counts = [0usize; 3]; // lf, crlf, cr
    let mut first = [usize::MAX; 3];
    let mut i = 0;
    while i < b.len() {
        let k = match b[i] {
            b'\n' => 0,
            b'\r' if b.get(i + 1) == Some(&b'\n') => {
                i += 1;
                1
            }
            b'\r' => 2,
            _ => {
                i += 1;
                continue;
            }
        };
        counts[k] += 1;
        first[k] = first[k].min(i);
        i += 1;
    }
    let best = (0..3)
        .max_by_key(|&k| (counts[k], std::cmp::Reverse(first[k])))
        .unwrap_or(0);
    if counts[best] == 0 {
        return Eol::Lf;
    }
    [Eol::Lf, Eol::CrLf, Eol::Cr][best]
}

/// Lone `\r` to `\n`, same byte length (see the module docs).
pub fn analysis_text(text: &str) -> Cow<'_, str> {
    let b = text.as_bytes();
    let lone = |i: usize| b[i] == b'\r' && b.get(i + 1) != Some(&b'\n');
    if !(0..b.len()).any(lone) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.char_indices() {
        out.push(if c == '\r' && lone(i) { '\n' } else { c });
    }
    Cow::Owned(out)
}

/// `\r\n` and lone `\r` to `\n`.
pub fn to_lf(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Write LF text with the terminator `eol`.
pub fn from_lf(text: &str, eol: Eol) -> String {
    match eol {
        Eol::Lf => text.to_string(),
        other => text.replace('\n', other.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_and_mapping() {
        assert_eq!(detect("a\nb\n"), Eol::Lf);
        assert_eq!(detect("a\r\nb\r\n"), Eol::CrLf);
        assert_eq!(detect("a\rb\rc"), Eol::Cr);
        assert_eq!(detect("no newline"), Eol::Lf);
        assert_eq!(detect("a\r\nb\nc\r\n"), Eol::CrLf);
        assert_eq!(analysis_text("a\rb\r\nc\r").as_ref(), "a\nb\r\nc\n");
        assert_eq!(to_lf("a\rb\r\nc\r").as_ref(), "a\nb\nc\n");
        assert_eq!(from_lf("a\nb\n", Eol::CrLf), "a\r\nb\r\n");
    }
}
