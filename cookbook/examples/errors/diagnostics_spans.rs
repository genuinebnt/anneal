//! RECIPE 62 (errors): compiler-style diagnostics: byte spans, line/column, and a caret under the problem.
//! LEVEL: intermediate
//! Use in: any DSL/config parser (see zero_copy_lexer, pratt_parser), validators that should point at the exact location.   Run: cargo run --release --example diagnostics_spans
//! COMMON USES: compilers, linters, config validators, query parsers: errors that point at the exact bytes.
#![allow(dead_code)]
//!
//! Store errors as a MESSAGE + a byte SPAN (`start..end`) into the source, not a pre-formatted string. A separate
//! renderer turns span -> (line, column) and prints the source line with `^^^` underneath. Keeping spans in your AST/tokens
//! from day one is what makes good error messages possible later.
//! Study real implementations: https://github.com/biomejs/biome/tree/main/crates/biome_diagnostics and
//! https://github.com/zkat/miette  and  https://github.com/brendanzab/codespan
use std::ops::Range;

#[derive(Debug)]
struct Diagnostic {
    message: String,
    span: Range<usize>,
    help: Option<String>,
}

fn render(src: &str, file: &str, d: &Diagnostic) -> String {
    let line_start = src[..d.span.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[d.span.start..]
        .find('\n')
        .map_or(src.len(), |i| d.span.start + i);
    let line_no = src[..d.span.start].matches('\n').count() + 1;
    let col = src[line_start..d.span.start].chars().count() + 1; // columns count chars, not bytes
    let width = src[d.span.start..d.span.end.min(line_end)]
        .chars()
        .count()
        .max(1);
    let gutter = line_no.to_string();
    let pad = " ".repeat(gutter.len());
    let mut out = format!(
        "error: {}\n{pad}--> {file}:{line_no}:{col}\n{pad} |\n",
        d.message
    );
    out += &format!("{gutter} | {}\n", &src[line_start..line_end]);
    out += &format!("{pad} | {}{}\n", " ".repeat(col - 1), "^".repeat(width));
    if let Some(h) = &d.help {
        out += &format!("{pad} = help: {h}\n");
    }
    out
}

fn find_unknown(src: &str, known: &[&str]) -> Vec<Diagnostic> {
    let mut diags = vec![];
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if word.is_empty() {
                continue;
            }
            let start = offset + word.as_ptr() as usize - line.as_ptr() as usize;
            if !known.contains(&word) && !word.chars().all(|c| c.is_ascii_digit()) {
                diags.push(Diagnostic {
                    message: format!("unknown task `{word}`"),
                    span: start..start + word.len(),
                    help: known
                        .iter()
                        .find(|k| k.starts_with(&word[..1]))
                        .map(|k| format!("did you mean `{k}`?")),
                });
            }
        }
        offset += line.len();
    }
    diags
}

fn main() {
    let src = "upload -> validate\nvalidate -> transcod\n  thumbnail -> pubish\n";
    let known = ["upload", "validate", "transcode", "thumbnail", "publish"];
    let diags = find_unknown(src, &known);
    for d in &diags {
        println!("{}", render(src, "video.flow", d));
    }
    assert_eq!(diags.len(), 2);
    assert_eq!(&src[diags[0].span.clone()], "transcod");
}
