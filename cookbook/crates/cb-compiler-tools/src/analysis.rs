//! The ONLY module that talks to the compiler crate (`pebble`).
//!
//! Every other file in this crate (and its tests) reaches the compiler through the
//! re-exports below, so when the compiler's API moves (new module system, project
//! analysis, cross-file indices) this is the one file to update.
//!
//! * [`analyze`] analyses one source text (today's contract: one file is one program).
//! * [`parse`] and [`lex`] expose the recoverable front end.
//! * [`run`] interprets a checked program; tests use it to compare behaviour.
//!
//! A later pass wires project-wide analysis here (`analyze_project(entry, loader)`
//! and a cross-file semantic index): add a `ProjectAnalysis` next to [`analyze`]
//! and let `lsp::workspace` call it; nothing else needs to change.

pub use pebble::{Analysis, diag, interp, sema, source, syntax};

use pebble::sema::CheckOptions;
use pebble::syntax::{ParseResult, Token};

/// Analyse one source text with the default checker options.
pub fn analyze(src: &str) -> Analysis {
    pebble::analyze(src, &CheckOptions::default())
}

/// Parse only (no checking).
pub fn parse(src: &str) -> ParseResult {
    pebble::syntax::parse(src)
}

/// Lex only: tokens, comments and lexer diagnostics.
pub fn lex(src: &str) -> (Vec<Token>, Vec<syntax::Comment>, Vec<diag::Diagnostic>) {
    pebble::syntax::lex(src)
}

/// Run a checked program, returning everything it printed and the runtime error, if any.
pub fn run(an: &Analysis) -> (String, Option<String>) {
    let mut out = Vec::new();
    let res = pebble::interp::run(&an.hir, &mut out);
    (
        String::from_utf8_lossy(&out).into_owned(),
        res.err().map(|e| e.to_string()),
    )
}
