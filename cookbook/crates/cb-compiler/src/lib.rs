//! PEBBLE: a small statically typed language and its compiler front end.
//! See `docs/ARCHITECTURE.md` for the pipeline and the stable API contract.

// Block and local ids index several parallel tables at once throughout the back end;
// rewriting those loops as `iter().enumerate()` over one of the tables reads worse.
#![allow(clippy::needless_range_loop)]

pub mod bc;
pub mod diag;
pub mod driver;
pub mod interp;
pub mod mir;
pub mod opt;
pub mod sema;
pub mod source;
pub mod syntax;
pub mod vm;

use diag::Diagnostic;

/// Everything the front end knows about one file.
pub struct Analysis {
    pub ast: syntax::ast::Module,
    pub comments: Vec<syntax::Comment>,
    pub hir: sema::Program,
    pub index: sema::SemanticIndex,
    /// Lexer, parser and checker diagnostics, merged and sorted by position.
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }
}

/// Parse and check `src` in one call (what the CLI and editor integrations use).
pub fn analyze(src: &str, opts: &sema::CheckOptions) -> Analysis {
    let p = syntax::parse(src);
    let c = sema::check_with(&p.ast, opts);
    let mut diagnostics = p.diagnostics;
    diagnostics.extend(c.diagnostics);
    diag::sort_and_dedup(&mut diagnostics);
    Analysis {
        ast: p.ast,
        comments: p.comments,
        hir: c.hir,
        index: c.index,
        diagnostics,
    }
}
