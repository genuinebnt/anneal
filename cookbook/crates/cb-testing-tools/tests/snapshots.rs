//! insta snapshot tests: assert a value against a reviewed, committed text file instead of
//! hand-writing a big expected literal. Snapshots live in `tests/snapshots/*.snap`.
//!
//! Workflow: change code -> `cargo test` fails with a diff and writes `*.snap.new` ->
//! `cargo insta review` (or `cargo insta accept`) shows each diff and promotes it to `*.snap`
//! (cargo-insta is a separate tool: `cargo install cargo-insta`, not installed here) ->
//! commit the `.snap` files; the diff in code review IS the review of behaviour change.
//! Without cargo-insta: `INSTA_UPDATE=always cargo test` rewrites snapshots in place.
//! In CI set `INSTA_UPDATE=no` so a mismatch (or a missing snapshot) fails and never writes files
//! (insta also does this automatically when the `CI` env var is set). `INSTA_FORCE_PASS=1`
//! reports all diffs without failing.
use cb_testing_tools::counter::IdGen;
use cb_testing_tools::duration::{format_duration, parse_duration};
use cb_testing_tools::expr::{Expr, format_expr, parse_expr};
use insta::{assert_debug_snapshot, assert_json_snapshot, assert_snapshot};
use serde::Serialize;

#[test]
fn pretty_printer_output() {
    // assert_snapshot! stores the Display/String value; the file is named after the test fn.
    let src = "((a)) - (b - c) * -(d + 1) / 2";
    assert_snapshot!(format_expr(&parse_expr(src).unwrap()));
}

#[test]
fn parse_tree_debug() {
    // assert_debug_snapshot! stores `{:#?}`: ideal for trees and error enums.
    assert_debug_snapshot!(parse_expr("-a * (b + 2)"));
}

#[test]
fn error_messages() {
    // Named snapshots (first arg) let one test hold several snapshots; the `@` form below is inline.
    for (name, src) in [
        ("unclosed", "(1 + 2"),
        ("bad_char", "1 + $"),
        ("overflow", "99999999999999999999"),
        ("empty", ""),
    ] {
        assert_snapshot!(name, parse_expr(src).unwrap_err().to_string());
    }
}

#[test]
fn inline_snapshot() {
    // Inline snapshots live in the source file itself (`@r"..."`; leading indentation is
    // normalised). `cargo insta review` rewrites the literal in place when the output changes.
    assert_snapshot!(format_duration(parse_duration("90s5ms").unwrap()), @"1m30s5ms");
    assert_debug_snapshot!(parse_expr("1+x"), @r#"
    Ok(
        Bin(
            Add,
            Num(
                1,
            ),
            Var(
                "x",
            ),
        ),
    )
    "#);
}

/// A report with VOLATILE fields (ids from a generator, wall-clock time): they would make a
/// naive snapshot differ on every run, so they are redacted.
#[derive(Serialize)]
struct Report {
    run_id: usize,
    elapsed_ms: u128,
    expr: String,
    nodes: usize,
    log: Vec<String>,
}

fn node_count(e: &Expr) -> usize {
    match e {
        Expr::Num(_) | Expr::Var(_) => 1,
        Expr::Neg(x) => 1 + node_count(x),
        Expr::Bin(_, l, r) => 1 + node_count(l) + node_count(r),
    }
}

fn make_report() -> Report {
    let started = std::time::Instant::now();
    let e = parse_expr("a * (b + 1)").unwrap();
    let ids = IdGen::new();
    Report {
        run_id: ids.next_id() + 1000 + std::process::id() as usize % 7,
        elapsed_ms: started.elapsed().as_millis() + 12345,
        expr: format_expr(&e),
        nodes: node_count(&e),
        log: vec!["parsed in 3ms".into(), "printed in 12ms".into()],
    }
}

#[test]
fn redactions_hide_volatile_fields() {
    // Redactions replace values at a selector path BEFORE writing/comparing (needs insta's
    // "redactions" + "json" features). `[id]` and `[ms]` are what the .snap file contains.
    assert_json_snapshot!(make_report(), {
        ".run_id" => "[id]",
        ".elapsed_ms" => "[ms]",
        ".log" => insta::dynamic_redaction(|v, _| format!("{} entries", v.as_slice().map_or(0, <[_]>::len))),
    });
}

#[test]
fn settings_filters_and_suffix() {
    // with_settings! scopes snapshot configuration to the block:
    //   filters  regex rewrites applied to the text (needs the "filters" feature)
    //   snapshot_suffix  separate snapshot per variant, here named `..@filtered`
    //   description  shown in `cargo insta review`
    let text = "parsed in 3ms, printed in 12ms, cached 7 items";
    insta::with_settings!({
        filters => vec![(r"\d+ms", "[N]ms")],
        snapshot_suffix => "filtered",
        description => "timings vary per machine, so they are filtered",
    }, {
        assert_snapshot!("timings", text);
    });
}
