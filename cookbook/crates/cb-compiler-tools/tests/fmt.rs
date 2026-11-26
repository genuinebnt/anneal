//! Formatter properties: idempotence, semantic preservation, comment preservation, robustness.

mod common;
use common::*;
use pebble_tools::fmt::{FormatError, ast_fingerprint, format};

fn comment_texts(src: &str) -> Vec<String> {
    pebble_tools::analysis::syntax::parse(src)
        .comments
        .iter()
        .map(|c| c.text.trim_end().to_string())
        .collect()
}

fn check_all_properties(label: &str, src: &str) -> Result<(), String> {
    let once = match format(src) {
        Ok(s) => s,
        Err(e) => return Err(format!("{label}: format failed: {e}")),
    };
    let twice = format(&once).map_err(|e| format!("{label}: second format failed: {e}\n{once}"))?;
    if once != twice {
        return Err(format!(
            "{label}: not idempotent\n--- once ---\n{once}\n--- twice ---\n{twice}"
        ));
    }
    if ast_fingerprint(src) != ast_fingerprint(&once) {
        return Err(format!("{label}: AST changed\n{once}"));
    }
    if comment_texts(src) != comment_texts(&once) {
        return Err(format!(
            "{label}: comments changed\n{:?}\n{:?}",
            comment_texts(src),
            comment_texts(&once)
        ));
    }
    if once.lines().any(|l| l.len() != l.trim_end().len()) {
        return Err(format!("{label}: trailing whitespace"));
    }
    Ok(())
}

#[test]
fn corpus_is_idempotent_semantic_and_keeps_comments() {
    let mut failures = Vec::new();
    let mut formatted = 0;
    for (path, src) in corpus() {
        if pebble_tools::analysis::syntax::parse(&src)
            .diagnostics
            .iter()
            .any(|d| d.is_error())
        {
            assert!(
                matches!(format(&src), Err(FormatError::Syntax(_))),
                "{} should be rejected",
                path.display()
            );
            continue;
        }
        formatted += 1;
        if let Err(e) = check_all_properties(&path.display().to_string(), &src) {
            failures.push(e);
        }
    }
    assert!(formatted > 60, "only {formatted} files formatted");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n====\n")
    );
}

#[test]
fn generated_programs_hold_all_properties() {
    let mut failures = Vec::new();
    let mut ok = 0;
    for seed in 0..500u64 {
        let src = Gen::program(seed);
        if pebble_tools::analysis::syntax::parse(&src)
            .diagnostics
            .iter()
            .any(|d| d.is_error())
        {
            continue; // the generator is allowed to be slightly off-grammar
        }
        ok += 1;
        if let Err(e) = check_all_properties(&format!("seed {seed}"), &src) {
            failures.push(format!("{e}\n--- source ---\n{src}"));
        }
    }
    assert!(
        ok >= 300,
        "generator produced only {ok} valid programs of 500"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n====\n"));
}

#[test]
fn formatting_does_not_change_program_output() {
    big_stack(|| {
        let mut count = 0;
        for path in peb_files("tests/run") {
            let src = std::fs::read_to_string(&path).unwrap();
            let run = |text: &str| -> Option<String> {
                let an = pebble_tools::analysis::analyze(text);
                if an.has_errors() {
                    return None;
                }
                let mut out = Vec::new();
                let res = pebble_tools::analysis::interp::run(&an.hir, &mut out);
                Some(format!(
                    "{}{}",
                    String::from_utf8_lossy(&out),
                    res.err().map(|e| e.to_string()).unwrap_or_default()
                ))
            };
            let Some(before) = run(&src) else { continue };
            let formatted = format(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let after = run(&formatted)
                .unwrap_or_else(|| panic!("{} no longer checks after formatting", path.display()));
            assert_eq!(
                before,
                after,
                "{} behaves differently after formatting",
                path.display()
            );
            count += 1;
        }
        assert!(count >= 40, "only {count} programs compared");
    });
}

#[test]
fn syntax_errors_are_a_clean_error() {
    for src in [
        "fn main( {",
        "fn main() { let x = ; }",
        "struct { }",
        "fn f() { \"abc }",
        "@@@",
    ] {
        assert!(
            matches!(format(src), Err(FormatError::Syntax(_))),
            "{src:?}"
        );
    }
}

#[test]
fn comments_attach_leading_trailing_dangling() {
    let src = "// lead\nfn main() { // open\n    let x = 1; // trailing\n\n\n    // before y\n    let y = 2;\n    /* dangling */\n}\nfn e() {\n    // only a comment\n}\n";
    let out = format(src).unwrap();
    let want = "// lead\nfn main() {\n    // open\n    let x = 1; // trailing\n\n    // before y\n    let y = 2;\n    /* dangling */\n}\n\nfn e() {\n    // only a comment\n}\n";
    assert_eq!(out, want);
}

#[test]
fn long_calls_break_with_trailing_commas() {
    let src = "fn main() { let total = compute(first_argument_value, second_argument_value, third_argument_value, fourth_argument_value); }";
    let out = format(src).unwrap();
    assert!(
        out.contains("compute(\n        first_argument_value,"),
        "{out}"
    );
    assert!(out.contains("fourth_argument_value,\n    );"), "{out}");
    assert!(out.lines().all(|l| l.chars().count() <= 100));
}

#[test]
fn fuzz_mutated_inputs_never_panic() {
    let corpus = corpus();
    let mut rng = Rng::new(7);
    let junk = [
        "{", "}", "(", ")", "/*", "*/", "//", "\"", "fn ", ";", "=>", "é", "😀", "\n",
    ];
    for _ in 0..5000 {
        let mut s = rng.pick(&corpus).1.clone();
        for _ in 0..1 + rng.below(4) {
            let mut at = rng.below(s.len() + 1);
            while !s.is_char_boundary(at) {
                at -= 1;
            }
            match rng.below(3) {
                0 => s.insert_str(at, rng.pick(&junk)),
                1 => {
                    let mut end = (at + rng.below(8)).min(s.len());
                    while !s.is_char_boundary(end) {
                        end -= 1;
                    }
                    s.replace_range(at..end, "");
                }
                _ => s.truncate(at),
            }
        }
        let r = std::panic::catch_unwind(|| format(&s));
        match r {
            Err(_) => panic!("formatter panicked on {s:?}"),
            Ok(Ok(out)) => assert_eq!(
                format(&out).ok().as_deref(),
                Some(out.as_str()),
                "not idempotent on mutated input {s:?}"
            ),
            Ok(Err(FormatError::Internal(m))) => panic!("internal formatter error {m} on {s:?}"),
            Ok(Err(_)) => {}
        }
    }
}
