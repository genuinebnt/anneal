//! RECIPE 62 (errors): compiler-style diagnostics: byte spans, line and column, and a caret under the problem.
//! LEVEL: intermediate
//! Use in: any parser or checker whose errors should point at the exact place.   Run: cargo run --release --example diagnostics_spans
//! COMMON USES: compilers, linters, config file checkers, query parsers.
//!
//! An error keeps a message and a span: the byte range of the source it is about. Only the renderer turns that range into
//! a line, a column and a row of carets. A line index finds the line by binary search.
//! The renderer accepts any span, even one outside the source or inside a character. It keeps tabs aligned, drops the `\r`
//! of a Windows line ending, and replaces control characters so a file cannot send commands to your terminal.
//! NOT COVERED: wide characters such as most emoji take two terminal cells, so carets after them drift. A long line is
//! printed whole. A span over several lines is underlined on its first line only. A tab inside a span gets one caret.
//! Each error has one span, and there is no colour.
//! Read: https://docs.rs/codespan-reporting and https://docs.rs/miette

use std::ops::Range;

#[derive(Debug)]
struct Diagnostic {
    message: String,
    /// Byte offsets into the source. The end is not included.
    span: Range<usize>,
    help: Option<String>,
}

/// The source with the offset at which each line starts.
struct Source<'a> {
    name: &'a str,
    text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> Source<'a> {
    fn new(name: &'a str, text: &'a str) -> Self {
        let after_newlines = text.match_indices('\n').map(|(at, _)| at + 1);
        let line_starts = std::iter::once(0).chain(after_newlines).collect();
        Source {
            name,
            text,
            line_starts,
        }
    }

    /// Line and column of a byte offset, both from 1. The column counts characters.
    /// An offset past the end, or inside a character, is moved back to the nearest place that exists.
    fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = self.text.floor_char_boundary(offset);
        // how many lines start at or before the offset
        let line = self.line_starts.partition_point(|&start| start <= offset);
        let column = self.text[self.line_starts[line - 1]..offset]
            .chars()
            .count()
            + 1;
        (line, column)
    }

    /// The text of a line (numbered from 1) without `\n` or `\r\n`.
    fn line(&self, line: usize) -> &'a str {
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        let text = &self.text[start..end];
        match text.strip_suffix('\n') {
            Some(text) => text.strip_suffix('\r').unwrap_or(text),
            None => text,
        }
    }

    fn render(&self, diagnostic: &Diagnostic) -> String {
        let start = self.text.floor_char_boundary(diagnostic.span.start);
        // an end before the start counts as an empty span
        let end = diagnostic.span.end.max(diagnostic.span.start);
        let end = self.text.ceil_char_boundary(end.min(self.text.len()));

        let (line, _) = self.line_col(start);
        let text = self.line(line);
        let line_start = self.line_starts[line - 1];
        // the part of the span that is on the shown text
        let from = (start - line_start).min(text.len());
        let to = (end - line_start).min(text.len()).max(from);

        let column = text[..from].chars().count() + 1;
        // copy the tabs, so the terminal moves the carets as far as it moved the text
        let indent: String = text[..from]
            .chars()
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        let carets = "^".repeat(text[from..to].chars().count().max(1));
        let number = line.to_string();
        let gutter = " ".repeat(number.len());

        let mut out = format!("error: {}\n", printable(&diagnostic.message));
        out += &format!("{gutter}--> {}:{line}:{column}\n", printable(self.name));
        out += &format!("{gutter} |\n");
        out += &format!("{number} | {}\n", printable(text));
        out += &format!("{gutter} | {indent}{carets}\n");
        if let Some(help) = &diagnostic.help {
            out += &format!("{gutter} = help: {}\n", printable(help));
        }
        out
    }
}

/// One character out for each character in. Control characters other than tab, the marks and controls that change text
/// direction, and the line and paragraph separators become the replacement character.
fn printable(text: &str) -> String {
    let unsafe_to_show = |c: char| {
        (c.is_control() && c != '\t')
            || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}')
            || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    };
    text.chars()
        .map(|c| if unsafe_to_show(c) { '\u{fffd}' } else { c })
        .collect()
}

/// A small checker to produce errors: every `key = value` line must use a known key.
fn check_keys(text: &str, known: &[&str]) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        if let Some((before, _)) = line.split_once('=') {
            let key = before.trim();
            if !known.contains(&key) {
                let start = line_start + (before.len() - before.trim_start().len());
                found.push(Diagnostic {
                    message: format!("unknown key `{key}`"),
                    span: start..start + key.len(),
                    help: Some(format!("known keys: {}", known.join(", "))),
                });
            }
        }
        line_start += line.len(); // the length includes the line ending
    }
    found
}

/// The obvious slow way, as an oracle: walk the characters and count.
fn line_col_by_walking(text: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column) = (1, 1);
    for (at, c) in text.char_indices() {
        if at + c.len_utf8() > offset {
            break;
        }
        if c == '\n' {
            (line, column) = (line + 1, 1);
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// A small seeded random generator (xorshift), so every run tests the same inputs.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn main() {
    let known = ["port", "workers", "log"];
    let text = "port = 8080\r\nworkres = 4\r\n\tlgo = info\r\n";
    let source = Source::new("server.conf", text);
    let found = check_keys(text, &known);
    for diagnostic in &found {
        print!("{}", source.render(diagnostic));
    }
    assert_eq!(found.len(), 2);
    assert_eq!(&text[found[0].span.clone()], "workres");
    assert_eq!(
        source.render(&found[0]),
        "error: unknown key `workres`\n --> server.conf:2:1\n  |\n2 | workres = 4\n  | ^^^^^^^\n  = help: known keys: port, workers, log\n"
    );
    assert!(
        source
            .render(&found[1])
            .contains("3 | \tlgo = info\n  | \t^^^\n")
    );

    let plain = |text: &str, span: Range<usize>| {
        Source::new("f", text).render(&Diagnostic {
            message: "m".into(),
            span,
            help: None,
        })
    };
    // columns and carets count characters, not bytes
    assert!(plain("café = ou", 8..10).ends_with("f:1:8\n  |\n1 | café = ou\n  |        ^^\n"));
    // an empty span at the very end, and a span on the line ending, still get one caret
    assert!(plain("a = ", 4..4).ends_with("1 | a = \n  |     ^\n"));
    assert!(plain("ab\r\ncd", 3..4).ends_with("f:1:3\n  |\n1 | ab\n  |   ^\n"));
    // an end before the start, even on an earlier line, gives one caret at the start
    assert!(plain("abc\ndef", Range { start: 5, end: 1 }).ends_with("2 | def\n  |  ^\n"));
    // a span over two lines is underlined on the first
    assert!(plain("abc\ndef", 1..6).ends_with("1 | abc\n  |  ^^\n"));
    // an escape sequence in a file is shown, not obeyed
    assert!(!plain("x = \u{1b}[2J", 0..1).contains('\u{1b}'));

    // the file name and the help text are cleaned too, as well as the message and the source line
    let dirty = "\u{1b}";
    for (name, message, help) in [(dirty, "m", "h"), ("f", dirty, "h"), ("f", "m", dirty)] {
        let diagnostic = Diagnostic {
            message: message.into(),
            span: 0..1,
            help: Some(help.into()),
        };
        let out = Source::new(name, "x").render(&diagnostic);
        assert!(!out.contains(dirty) && out.contains('\u{fffd}'), "{out:?}");
    }
    // direction marks, direction controls and Unicode line separators
    let hidden = [
        '\u{061c}', '\u{200e}', '\u{200f}', '\u{2028}', '\u{2029}', '\u{202a}', '\u{202b}',
        '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
    ];
    for c in hidden {
        assert_eq!(printable(&c.to_string()), "\u{fffd}", "{c:?}");
    }

    // an unusual key must not break the checker
    assert_eq!(check_keys("über = 1\n  = 2\n🦀=3", &known).len(), 3);

    // random text and random spans: no panic, and the same positions as the slow way
    let pieces = [
        "a", " ", "\t", "\n", "\r\n", "\r", "é", "🦀", "e\u{301}", "\u{1b}", "\u{202e}", "\0",
    ];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20_000 {
        let text: String = (0..rng.below(40))
            .map(|_| pieces[rng.below(pieces.len())])
            .collect();
        let source = Source::new("f", &text);
        for offset in 0..=text.len() + 3 {
            assert_eq!(
                source.line_col(offset),
                line_col_by_walking(&text, offset),
                "{text:?} at {offset}"
            );
        }
        let any = |rng: &mut Rng| {
            if rng.below(20) == 0 {
                usize::MAX
            } else {
                rng.below(text.len() + 4)
            }
        };
        let span = any(&mut rng)..any(&mut rng);
        let out = source.render(&Diagnostic {
            message: text.clone(),
            span: span.clone(),
            help: None,
        });
        assert_eq!(out.lines().count(), 5, "{text:?} {span:?}: {out:?}");
        assert!(
            out.chars()
                .all(|c| c == '\n' || c == '\t' || !c.is_control()),
            "{out:?}"
        );
        assert!(!out.contains('\u{202e}'));

        // the line shown and the carets agree with a separate reading of the text
        let (line, _) = line_col_by_walking(&text, span.start);
        let shown = text.split('\n').nth(line - 1).unwrap();
        let shown = shown
            .strip_suffix('\r')
            .filter(|_| text.split('\n').count() > line)
            .unwrap_or(shown);
        let rows: Vec<&str> = out.split('\n').collect();
        assert_eq!(
            rows[3],
            format!("{line} | {}", printable(shown)),
            "{text:?} {span:?}"
        );
        let under = rows[4].split_once("| ").unwrap().1;
        assert!(under.ends_with('^') && under.chars().count() <= shown.chars().count() + 1);
    }

    // many errors in a large file: each lookup is a binary search
    let big = "wrong = 1\n".repeat(100_000);
    let source = Source::new("big.conf", &big);
    let found = check_keys(&big, &known);
    assert_eq!(found.len(), 100_000);
    assert_eq!(source.line_col(found[99_999].span.start), (100_000, 1));
    println!("all checks passed");
}
