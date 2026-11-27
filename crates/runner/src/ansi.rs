//! Terminal escape sequences in cargo's, rustc's and the learner's output. The runner keeps them (the web app draws
//! the colours), so whatever reads that output must read [`strip_ansi`] of it, and whatever cuts it must use
//! [`cut`] so no half a sequence is left behind. The web app's tokenizer (`web/src/workspace/ansiText.ts`) follows
//! the same rules.

use std::borrow::Cow;

/// The length in bytes of the escape sequence `s` starts with (`s[0]` is ESC), and whether it is complete.
/// An incomplete one never runs past the end of its line.
fn sequence(s: &[u8]) -> (usize, bool) {
    let run = |from: usize, ok: fn(u8) -> bool| from + s[from..].iter().take_while(|&&b| ok(b)).count();
    match s.get(1) {
        None => (1, false),
        // CSI: parameters, intermediates, one final byte.
        Some(b'[') => {
            let i = run(run(2, |b| (0x30..=0x3f).contains(&b)), |b| (0x20..=0x2f).contains(&b));
            match s.get(i) {
                Some(0x40..=0x7e) => (i + 1, true),
                _ => (i, false),
            }
        }
        // OSC (links, window titles) and the other string sequences: up to BEL or ESC \.
        Some(b']' | b'P' | b'X' | b'^' | b'_') => {
            let i = run(2, |b| !matches!(b, 0x07 | 0x1b | b'\n'));
            match (s.get(i), s.get(i + 1)) {
                (Some(0x07), _) => (i + 1, true),
                (Some(0x1b), Some(b'\\')) => (i + 2, true),
                _ => (i, false),
            }
        }
        Some(_) => {
            let i = run(1, |b| (0x20..=0x2f).contains(&b));
            match s.get(i) {
                Some(0x30..=0x7e) => (i + 1, true),
                _ => (i, false),
            }
        }
    }
}

/// `s` without any escape sequence. Text without one is returned as it is.
pub fn strip_ansi(s: &str) -> Cow<'_, str> {
    if !s.contains('\x1b') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    // Every byte a sequence takes is ASCII, so the cuts fall on char boundaries.
    while let Some(at) = rest.find('\x1b') {
        out.push_str(&rest[..at]);
        rest = &rest[at + sequence(&rest.as_bytes()[at..]).0..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Shortens `s` to at most `limit` bytes without splitting a character or an escape sequence, and resets the colour
/// if there was any, so that whatever the caller appends is drawn plain.
pub(crate) fn cut(s: &mut String, limit: usize) {
    let mut end = limit.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    if let Some(at) = s.rfind('\x1b') {
        let (len, complete) = sequence(&s.as_bytes()[at..]);
        if !complete && at + len == s.len() {
            s.truncate(at);
        }
    }
    if s.contains('\x1b') {
        s.push_str("\x1b[0m");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colour_links_cursor_moves_and_leaves_plain_text_alone() {
        let s = "\x1b[1m\x1b[38;5;9merror\x1b[0m: é\x1b[2J \x1b]8;;http://x\x07link\x1b]8;;\x1b\\ \x1b(Bok";
        assert_eq!(strip_ansi(s), "error: é link ok");
        assert!(matches!(strip_ansi("plain [0m é"), Cow::Borrowed("plain [0m é")));
        assert_eq!(strip_ansi("a\x1b[38;5\nb\x1b]8;;http\nc\x1b"), "a\nb\nc");
    }

    #[test]
    fn a_cut_never_leaves_half_a_sequence() {
        let full = "ab\x1b[38;5;208mcd\x1b]8;;u\x07é\x1b[0mz";
        for limit in 0..=full.len() {
            let mut s = full.to_owned();
            cut(&mut s, limit);
            s.push_str("\n… cut");
            let plain = strip_ansi(&s);
            assert!("abcdéz".starts_with(plain.strip_suffix("\n… cut").unwrap()), "limit {limit}: {s:?}");
            assert!(!s.contains('\x1b') || s.ends_with("\x1b[0m\n… cut"), "limit {limit}: {s:?}");
        }
    }
}
