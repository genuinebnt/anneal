//! Rendering diagnostics as rustc-like text (optionally with ANSI colour) or JSON.
//!
//! Layout per diagnostic:
//! ```text
//! error[E0301]: mismatched types
//!  --> demo.peb:3:16
//!   |
//! 3 |     let x: int = true;
//!   |            ---   ^^^^ expected `int`, found `bool`
//!   |            |
//!   |            expected due to this type
//! ```
//! Same-line labels are drawn on one underline row, the rightmost label's text
//! trailing it and the others hanging below on `|` connectors. One multi-line
//! label per diagnostic gets the `/ ... |___^` bracket; further multi-line labels
//! are flattened to their first line.

use super::{Applicability, Diagnostic, Severity};
use crate::source::{SourceFile, Span};
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptions {
    /// Emit ANSI escape sequences (off by default so output is diffable).
    pub color: bool,
}

const TAB_WIDTH: usize = 4;

/// Terminal cell width of a char: tabs expand, combining marks take none, wide
/// East-Asian and emoji ranges take two. An approximation of `wcwidth` without a table crate.
fn char_width(c: char) -> usize {
    let u = c as u32;
    match u {
        0x09 => TAB_WIDTH,
        0x300..=0x36F | 0x200B..=0x200F | 0xFE00..=0xFE0F => 0,
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

fn display_col(line: &str, byte_col: usize) -> usize {
    let mut w = 0;
    for (i, c) in line.char_indices() {
        if i >= byte_col {
            break;
        }
        w += char_width(c);
    }
    w
}

fn expand_tabs(line: &str) -> String {
    line.replace('\t', &" ".repeat(TAB_WIDTH))
}

struct Painter {
    color: bool,
}

impl Painter {
    fn paint(&self, s: &str, code: &str) -> String {
        if self.color && !s.is_empty() {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn sev(&self, s: Severity) -> &'static str {
        match s {
            Severity::Error => "1;31",
            Severity::Warning => "1;33",
            Severity::Note => "1;32",
            Severity::Help => "1;36",
        }
    }
    fn gutter(&self, s: &str) -> String {
        self.paint(s, "1;34")
    }
}

/// One cell of an annotation row; `style`: 0 plain, 1 primary label, 2 secondary label.
#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    style: u8,
}

#[derive(Default)]
struct Row(Vec<Cell>);

impl Row {
    fn put(&mut self, col: usize, ch: char, style: u8) {
        while self.0.len() <= col {
            self.0.push(Cell { ch: ' ', style: 0 });
        }
        self.0[col] = Cell { ch, style };
    }
    fn put_str(&mut self, col: usize, s: &str, style: u8) {
        for (i, ch) in s.chars().enumerate() {
            self.put(col + i, ch, style);
        }
    }
    fn paint(&self, p: &Painter, sev: Severity) -> String {
        let mut out = String::new();
        let mut run = String::new();
        let mut run_style = 0u8;
        let flush = |out: &mut String, run: &mut String, style: u8| {
            let code = match style {
                1 => p.sev(sev),
                2 => "1;34",
                _ => "",
            };
            if code.is_empty() {
                out.push_str(run)
            } else {
                out.push_str(&p.paint(run, code))
            }
            run.clear();
        };
        for c in &self.0 {
            if c.style != run_style {
                flush(&mut out, &mut run, run_style);
                run_style = c.style;
            }
            run.push(c.ch);
        }
        flush(&mut out, &mut run, run_style);
        out.trim_end().to_string()
    }
}

/// A label resolved to lines and display columns.
struct Ann {
    sline: usize,
    scol: usize,
    eline: usize,
    ecol: usize,
    text: String,
    primary: bool,
}

impl Ann {
    fn style(&self) -> u8 {
        if self.primary { 1 } else { 2 }
    }
    fn multiline(&self) -> bool {
        self.sline != self.eline
    }
}

fn resolve(file: &SourceFile, span: Span, text: &str, primary: bool) -> Ann {
    let len = file.text.len();
    let start = (span.start as usize).min(len);
    let mut end = (span.end as usize).clamp(start, len);
    // A span that swallows the trailing newline should stop at the end of its last line.
    while end > start && matches!(file.text.as_bytes()[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    let sline = file.index.line_of(start);
    let eline = file.index.line_of(end);
    let sl = file.line_text(sline);
    let el = file.line_text(eline);
    let scol = display_col(sl, start - file.index.line_start(sline));
    let ecol = display_col(el, end - file.index.line_start(eline));
    Ann {
        sline,
        scol,
        eline,
        ecol,
        text: text.to_string(),
        primary,
    }
}

/// Draw the underline rows for labels that live on one line.
fn annotation_rows(anns: &[&Ann], p: &Painter, sev: Severity) -> Vec<String> {
    if anns.is_empty() {
        return vec![];
    }
    let mut order: Vec<&Ann> = anns.to_vec();
    order.sort_by_key(|a| (a.scol, a.ecol));
    let mut line = Row::default();
    // Secondary first so a primary caret wins where they overlap.
    for pass in [2u8, 1u8] {
        for a in order.iter().filter(|a| a.style() == pass) {
            let ch = if a.primary { '^' } else { '-' };
            for c in a.scol..a.ecol.max(a.scol + 1) {
                line.put(c, ch, pass);
            }
        }
    }
    let last = *order.last().unwrap_or(&order[0]);
    if !last.text.is_empty() {
        line.put_str(last.ecol.max(last.scol + 1) + 1, &last.text, last.style());
    }
    let mut rows = vec![line.paint(p, sev)];
    let hanging: Vec<&Ann> = order[..order.len() - 1]
        .iter()
        .copied()
        .filter(|a| !a.text.is_empty())
        .collect();
    if !hanging.is_empty() {
        let mut conn = Row::default();
        for a in &hanging {
            conn.put(a.scol, '|', a.style());
        }
        rows.push(conn.paint(p, sev));
        for i in (0..hanging.len()).rev() {
            let mut r = Row::default();
            for a in &hanging[..i] {
                r.put(a.scol, '|', a.style());
            }
            r.put_str(hanging[i].scol, &hanging[i].text, hanging[i].style());
            rows.push(r.paint(p, sev));
        }
    }
    rows
}

pub fn render(d: &Diagnostic, file: &SourceFile, opts: &RenderOptions) -> String {
    let p = Painter { color: opts.color };
    let mut out = String::new();
    let head = if d.code.is_empty() {
        d.severity.as_str().to_string()
    } else {
        format!("{}[{}]", d.severity.as_str(), d.code)
    };
    let _ = writeln!(
        out,
        "{}: {}",
        p.paint(&head, p.sev(d.severity)),
        p.paint(&d.message, "1")
    );

    let mut anns: Vec<Ann> = d
        .labels
        .iter()
        .map(|l| resolve(file, l.span, &l.text, l.primary))
        .collect();
    // Only the first multi-line label keeps its bracket; flatten the rest.
    let ml_index = anns.iter().position(|a| a.multiline());
    for (i, a) in anns.iter_mut().enumerate() {
        if a.multiline() && Some(i) != ml_index {
            a.eline = a.sline;
            a.ecol = display_col(file.line_text(a.sline), usize::MAX);
        }
    }
    let ml = ml_index.map(|i| {
        (
            anns[i].sline,
            anns[i].eline,
            anns[i].scol,
            anns[i].ecol,
            anns[i].text.clone(),
            anns[i].primary,
        )
    });

    let mut width = 1;
    if !anns.is_empty() {
        let mut shown: BTreeSet<usize> = BTreeSet::new();
        for a in &anns {
            shown.insert(a.sline);
            shown.insert(a.eline);
        }
        if let Some((s, e, ..)) = &ml {
            if e - s <= 5 {
                shown.extend(*s..=*e);
            } else {
                shown.extend([*s + 1, *e - 1]);
            }
        }
        // Show a single skipped line instead of an ellipsis.
        let snapshot: Vec<usize> = shown.iter().copied().collect();
        for w in snapshot.windows(2) {
            if w[1] - w[0] == 2 {
                shown.insert(w[0] + 1);
            }
        }
        width = (shown.iter().next_back().copied().unwrap_or(0) + 1)
            .to_string()
            .len();
        let pl = d.primary_span().unwrap_or_default();
        let lc = file.line_col_chars((pl.start as usize).min(file.text.len()));
        let sp = " ".repeat(width);
        let _ = writeln!(
            out,
            "{}{} {}:{}:{}",
            sp,
            p.gutter("-->"),
            file.name,
            lc.line + 1,
            lc.col + 1
        );
        let _ = writeln!(out, "{} {}", sp, p.gutter("|"));

        let prefix = |line: usize| -> String {
            match &ml {
                Some((s, e, _, _, _, prim)) => {
                    if line > *s && line <= *e {
                        p.paint("|", if *prim { p.sev(d.severity) } else { "1;34" }) + " "
                    } else {
                        "  ".to_string()
                    }
                }
                None => String::new(),
            }
        };
        let mut prev: Option<usize> = None;
        for &line in &shown {
            if let Some(pl) = prev
                && line - pl > 1
            {
                let _ = writeln!(out, "{}", p.gutter("..."));
            }
            prev = Some(line);
            let src = expand_tabs(file.line_text(line));
            let _ = writeln!(
                out,
                "{} {}{}",
                p.gutter(&format!("{:>width$} |", line + 1)),
                prefix(line),
                src
            );
            let singles: Vec<&Ann> = anns
                .iter()
                .filter(|a| !a.multiline() && a.sline == line)
                .collect();
            let gut = format!("{} {} ", " ".repeat(width), p.gutter("|"));
            for row in annotation_rows(&singles, &p, d.severity) {
                let _ = writeln!(out, "{}{}{}", gut, prefix(line), row);
            }
            if let Some((s, e, scol, ecol, text, prim)) = &ml {
                let style = if *prim { 1 } else { 2 };
                if line == *s {
                    let mut r = Row::default();
                    r.put(0, ' ', 0);
                    for c in 1..=(*scol + 1) {
                        r.put(c, '_', style);
                    }
                    r.put(*scol + 2, if *prim { '^' } else { '-' }, style);
                    let _ = writeln!(out, "{}{}", gut, r.paint(&p, d.severity));
                }
                if line == *e {
                    let mut r = Row::default();
                    r.put(0, '|', style);
                    let ec = (*ecol).max(1);
                    for c in 1..=ec {
                        r.put(c, '_', style);
                    }
                    r.put(ec + 1, if *prim { '^' } else { '-' }, style);
                    if !text.is_empty() {
                        r.put_str(ec + 3, text, style);
                    }
                    let _ = writeln!(out, "{}{}", gut, r.paint(&p, d.severity));
                }
            }
        }
    }
    let sp = " ".repeat(width);
    let has_tail = !d.notes.is_empty() || !d.help.is_empty() || !d.suggestions.is_empty();
    if has_tail && !d.labels.is_empty() {
        let _ = writeln!(out, "{} {}", sp, p.gutter("|"));
    }
    let indent = format!("{sp}   ");
    for (kind, items) in [("note", &d.notes), ("help", &d.help)] {
        for text in items {
            let text = text.replace('\n', &format!("\n{indent}"));
            let _ = writeln!(
                out,
                "{} {} {}: {}",
                sp,
                p.gutter("="),
                p.paint(kind, "1"),
                text
            );
        }
    }
    for s in &d.suggestions {
        let _ = writeln!(
            out,
            "{} {} {}: {}",
            sp,
            p.gutter("="),
            p.paint("help", "1"),
            s.message
        );
    }
    out
}

/// Render all diagnostics (in the given order), separated by blank lines, followed by a summary line.
pub fn render_all(diags: &[Diagnostic], file: &SourceFile, opts: &RenderOptions) -> String {
    let p = Painter { color: opts.color };
    let mut out = String::new();
    for d in diags {
        out.push_str(&render(d, file, opts));
        out.push('\n');
    }
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let plural = |n: usize, w: &str| {
        if n == 1 {
            format!("{n} {w}")
        } else {
            format!("{n} {w}s")
        }
    };
    if errors > 0 {
        let mut s = format!("aborting due to {}", plural(errors, "previous error"));
        if warnings > 0 {
            s.push_str(&format!("; {} emitted", plural(warnings, "warning")));
        }
        let _ = writeln!(
            out,
            "{}: {}",
            p.paint("error", p.sev(Severity::Error)),
            p.paint(&s, "1")
        );
    } else if warnings > 0 {
        let _ = writeln!(
            out,
            "{}: {} emitted",
            p.paint("warning", p.sev(Severity::Warning)),
            plural(warnings, "warning")
        );
    }
    out
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn json_span(file: &SourceFile, span: Span) -> String {
    let s = file.line_col_chars(span.start as usize);
    let e = file.line_col_chars(span.end as usize);
    let s16 = file.line_col_utf16(span.start as usize);
    let e16 = file.line_col_utf16(span.end as usize);
    format!(
        "{{\"start\":{},\"end\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{},\"utf16_line\":{},\"utf16_column\":{},\"utf16_end_line\":{},\"utf16_end_column\":{}}}",
        span.start,
        span.end,
        s.line + 1,
        s.col + 1,
        e.line + 1,
        e.col + 1,
        s16.line,
        s16.col,
        e16.line,
        e16.col
    )
}

/// One diagnostic as a single-line JSON object (line/column are 1-based chars; the
/// `utf16_*` fields are 0-based LSP positions).
pub fn to_json(d: &Diagnostic, file: &SourceFile) -> String {
    let labels: Vec<String> = d
        .labels
        .iter()
        .map(|l| {
            format!(
                "{{\"span\":{},\"text\":{},\"primary\":{}}}",
                json_span(file, l.span),
                json_str(&l.text),
                l.primary
            )
        })
        .collect();
    let strs = |v: &[String]| v.iter().map(|s| json_str(s)).collect::<Vec<_>>().join(",");
    let sugg: Vec<String> = d
        .suggestions
        .iter()
        .map(|s| {
            let edits: Vec<String> = s
                .edits
                .iter()
                .map(|e| {
                    format!(
                        "{{\"span\":{},\"replacement\":{}}}",
                        json_span(file, e.span),
                        json_str(&e.replacement)
                    )
                })
                .collect();
            let app = match s.applicability {
                Applicability::MachineApplicable => "machine-applicable",
                Applicability::MaybeIncorrect => "maybe-incorrect",
            };
            format!(
                "{{\"message\":{},\"applicability\":\"{}\",\"edits\":[{}]}}",
                json_str(&s.message),
                app,
                edits.join(",")
            )
        })
        .collect();
    format!(
        "{{\"severity\":\"{}\",\"code\":{},\"message\":{},\"file\":{},\"labels\":[{}],\"notes\":[{}],\"help\":[{}],\"suggestions\":[{}]}}",
        d.severity.as_str(),
        json_str(d.code),
        json_str(&d.message),
        json_str(&file.name),
        labels.join(","),
        strs(&d.notes),
        strs(&d.help),
        sugg.join(",")
    )
}

/// All diagnostics as a JSON array.
pub fn to_json_array(diags: &[Diagnostic], file: &SourceFile) -> String {
    format!(
        "[{}]",
        diags
            .iter()
            .map(|d| to_json(d, file))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Apply every machine-applicable edit (skipping overlapping ones) and return the new text.
pub fn apply_suggestions(text: &str, diags: &[Diagnostic]) -> String {
    let mut edits: Vec<(usize, usize, &str)> = diags
        .iter()
        .flat_map(|d| d.suggestions.iter())
        .filter(|s| s.applicability == Applicability::MachineApplicable)
        .flat_map(|s| s.edits.iter())
        .map(|e| {
            (
                e.span.start as usize,
                e.span.end as usize,
                e.replacement.as_str(),
            )
        })
        .collect();
    edits.sort_by_key(|e| (e.0, e.1));
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for (s, e, r) in edits {
        if s < pos || e > text.len() || !text.is_char_boundary(s) || !text.is_char_boundary(e) {
            continue;
        }
        out.push_str(&text[pos..s]);
        out.push_str(r);
        pos = e;
    }
    out.push_str(&text[pos..]);
    out
}
