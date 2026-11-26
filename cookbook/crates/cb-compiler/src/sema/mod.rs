//! Semantic analysis: name resolution, type checking with inference, pattern
//! analysis, warnings and the [`SemanticIndex`]. Entry point: [`check`].
//!
//! Data flow (see `docs/ARCHITECTURE.md`):
//! ```text
//! ast::Module --resolve--> item tables --lower--> signatures
//!      \--> per body: infer + check --> HirExpr --> zonk --> exhaustive --> Program
//! ```

pub mod check;
pub mod exhaustive;
pub mod hir;
pub mod index;
mod pattern;
pub mod resolve;
pub mod types;
pub mod unify;

use crate::diag::Diagnostic;
use crate::syntax::ast::Module;

pub use hir::{Builtin, DefInfo, DefKind, HirExpr, HirExprKind, HirFn, Program};
pub use index::SemanticIndex;
pub use types::{DefId, Ty};

/// Knobs for optional diagnostics.
#[derive(Clone, Copy, Debug, Default)]
pub struct CheckOptions {
    /// Report `let` bindings that shadow an earlier variable (W0505, off by default).
    pub warn_shadow: bool,
}

pub struct CheckResult {
    /// The typed program. Complete even when there are errors (erroneous parts are `Error` nodes).
    pub hir: Program,
    /// Errors and warnings, sorted by position.
    pub diagnostics: Vec<Diagnostic>,
    pub index: SemanticIndex,
}

/// Check a parsed module with default options.
pub fn check(module: &Module) -> CheckResult {
    check_with(module, &CheckOptions::default())
}

pub fn check_with(module: &Module, opts: &CheckOptions) -> CheckResult {
    check::run(module, opts)
}
