//! Library under test for the testing-tools recipe: small on purpose, but with the shapes that
//! each technique likes:
//!   * `expr`     parser + pretty-printer pair (round trips, idempotence, recursive data, snapshots, fuzzing)
//!   * `duration` `parse_duration` / `format_duration` codec (regex strategies, canonical forms)
//!   * `lru`      bounded LRU map (model-based testing against a naive oracle)
//!   * `counter`  atomic id generator / bounded counter (loom), built on the `sync` cfg facade
pub mod counter;
pub mod duration;
pub mod expr;
pub mod lru;
pub mod sync;

/// The entry point a fuzzer would call: arbitrary bytes in, must never panic or loop forever.
/// Properties checked inside: whatever parses must print and re-parse to the same tree.
pub fn fuzz_target(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(e) = expr::parse_expr(text) {
        let printed = expr::format_expr(&e);
        assert_eq!(
            expr::parse_expr(&printed),
            Ok(e),
            "round trip broke for {text:?}"
        );
    }
    let _ = duration::parse_duration(text);
}
