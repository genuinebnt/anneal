//! Turns cargo's JSON messages and libtest's JSON events into [`Diagnostic`]s and [`TestOutcome`]s.

use serde_json::Value;

use crate::ansi::strip_ansi;
use crate::project::TEST_PREFIX;
use crate::result::{Check, Diagnostic, Level, Outcome, Span, Suite, TestOutcome};

/// Compiler messages from `--message-format=json-diagnostic-rendered-ansi` output (`rendered`
/// keeps rustc's terminal colours; `message` is plain). Keeps diagnostics
/// that point at user files and drops rustc's "aborting due to…" summaries.
pub(crate) fn diagnostics(stdout: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    for v in json_lines(stdout) {
        if v["reason"] != "compiler-message" {
            continue;
        }
        let Some(d) = diagnostic(&v["message"]) else {
            continue;
        };
        // --all-targets reports lib warnings once per target; keep one copy.
        if !out.iter().any(|o| o.rendered == d.rendered) {
            out.push(d);
        }
    }
    out
}

/// Whether cargo said the build failed.
pub(crate) fn build_failed(stdout: &str) -> bool {
    json_lines(stdout).any(|v| v["reason"] == "build-finished" && v["success"] == false)
}

fn diagnostic(m: &Value) -> Option<Diagnostic> {
    let level = match m["level"].as_str()? {
        "error" | "error: internal compiler error" => Level::Error,
        "warning" => Level::Warning,
        "note" => Level::Note,
        "help" => Level::Help,
        _ => return None,
    };
    let spans: Vec<Span> = m["spans"].as_array()?.iter().filter_map(user_span).collect();
    // A warning that points nowhere in these files is cargo's bookkeeping. An error is why the run failed, so it is kept
    // even without a place (a link error, say), or the learner is told "doesn't compile" and nothing else.
    let summary = m["message"].as_str().is_some_and(|t| t.starts_with("aborting due to"));
    if spans.is_empty() && (level != Level::Error || summary) {
        return None;
    }
    let notes = m["children"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter_map(|c| {
                    Some(format!(
                        "{}: {}",
                        c["level"].as_str()?,
                        c["message"].as_str()?
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Diagnostic {
        level,
        code: m["code"]["code"].as_str().map(str::to_owned),
        message: m["message"].as_str().unwrap_or_default().to_owned(),
        rendered: m["rendered"].as_str().unwrap_or_default().to_owned(),
        spans,
        notes,
    })
}

/// `s` in a user file: itself, or else where the macro that produced it was called. An error inside `check!` is
/// reported in the injected prelude, and the line that matters is the test's.
fn user_span(s: &Value) -> Option<Span> {
    if let Some(found) = span(s) {
        return Some(found);
    }
    let mut at = &s["expansion"]["span"];
    while at.is_object() {
        if let Some(mut found) = span(at) {
            found.primary = s["is_primary"].as_bool().unwrap_or(false);
            found.label = s["label"].as_str().map(str::to_owned);
            return Some(found);
        }
        at = &at["expansion"]["span"];
    }
    None
}

/// What to show when cargo failed the build without a compiler error: the end of what it printed.
pub(crate) fn build_failure(stderr: &str) -> Diagnostic {
    let plain = strip_ansi(stderr);
    let killed = ["SIGKILL", "signal: 9", "out of memory", "memory allocation of"].iter().any(|t| plain.contains(t));
    let message = if killed {
        "the compiler was stopped before it finished, usually because it ran out of memory. Your code may be fine: run it again"
    } else {
        "the build failed, but the compiler reported no error in your code. What cargo printed is below"
    };
    let lines: Vec<&str> = stderr.lines().filter(|l| !strip_ansi(l).trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(30)..].join("\n");
    Diagnostic {
        level: Level::Error,
        code: None,
        message: message.to_owned(),
        rendered: format!("error: {message}\n{tail}"),
        spans: Vec::new(),
        notes: Vec::new(),
    }
}

fn span(s: &Value) -> Option<Span> {
    let file = s["file_name"].as_str()?;
    let user_file = file == "src/lib.rs"
        || file == "src/bin/scratch.rs"
        || (file.starts_with("tests/") && !file.starts_with("tests/anneal/"));
    if !user_file {
        return None;
    }
    let num = |k: &str| s[k].as_u64().map(|n| n as u32);
    let (line_start, line_end) = (num("line_start")?, num("line_end")?);
    let (mut col_start, mut col_end) = (num("column_start")?, num("column_end")?);
    // Undo the prelude injected on line 1 of test files.
    if file.starts_with("tests/") {
        let shift = TEST_PREFIX.chars().count() as u32;
        if line_start == 1 {
            col_start = col_start.saturating_sub(shift).max(1);
        }
        if line_end == 1 {
            col_end = col_end.saturating_sub(shift).max(1);
        }
    }
    Some(Span {
        file: file.to_owned(),
        line_start,
        line_end,
        col_start,
        col_end,
        primary: s["is_primary"].as_bool().unwrap_or(false),
        label: s["label"].as_str().map(str::to_owned),
    })
}

/// Test outcomes from a `cargo test … -- --format json` run.
///
/// libtest events don't name their binary, so suites are matched to the
/// `Running tests/<name>.rs` lines cargo writes to stderr, in order.
pub(crate) fn tests(stdout: &str, stderr: &str) -> Vec<TestOutcome> {
    let suites: Vec<Suite> = strip_ansi(stderr)
        .lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix("Running ")?;
            if rest.starts_with("tests/hidden.rs") {
                Some(Suite::Hidden)
            } else if rest.starts_with("tests/visible.rs") {
                Some(Suite::Visible)
            } else {
                None
            }
        })
        .collect();

    let mut out: Vec<TestOutcome> = Vec::new();
    let mut suite_idx: Option<usize> = None;
    for v in json_lines(stdout) {
        match (v["type"].as_str(), v["event"].as_str()) {
            (Some("suite"), Some("started")) => suite_idx = Some(suite_idx.map_or(0, |i| i + 1)),
            (Some("test"), Some(event)) => {
                let suite = suite_idx
                    .and_then(|i| suites.get(i).copied())
                    .unwrap_or(Suite::Visible);
                let name = v["name"].as_str().unwrap_or_default().to_owned();
                let outcome = match event {
                    "started" => {
                        out.push(TestOutcome {
                            suite,
                            name,
                            outcome: Outcome::TimedOut,
                            duration_ms: None,
                            check: None,
                            panic: None,
                            stdout: String::new(),
                        });
                        continue;
                    }
                    "ok" => Outcome::Passed,
                    "failed" => Outcome::Failed,
                    "ignored" => Outcome::Ignored,
                    _ => continue,
                };
                let raw = v["stdout"].as_str().unwrap_or_default();
                let (check, panic, stdout) = split_stdout(raw);
                let result = TestOutcome {
                    suite,
                    name: name.clone(),
                    outcome,
                    duration_ms: v["exec_time"].as_f64().map(|s| s * 1000.0),
                    check,
                    panic,
                    stdout,
                };
                // Replace the placeholder from the "started" event, if any.
                match out
                    .iter_mut()
                    .rev()
                    .find(|t| t.suite == suite && t.name == name && t.outcome == Outcome::TimedOut)
                {
                    Some(slot) => *slot = result,
                    None => out.push(result),
                }
            }
            _ => {}
        }
    }
    out
}

/// Splits a failed test's captured output into the `check!` report, the panic
/// message, and whatever else the test printed.
fn split_stdout(raw: &str) -> (Option<Check>, Option<String>, String) {
    let mut check = None;
    let mut panic = None;
    let mut rest = Vec::new();
    let mut lines = raw.lines().peekable();
    while let Some(line) = lines.next() {
        // Matched on the plain text: a test that printed a colour and no newline puts its escape in front of our line.
        let plain = strip_ansi(line);
        if let Some(json) = plain.strip_prefix("ANNEAL ") {
            check = serde_json::from_str::<Check>(json).ok();
        } else if plain.starts_with("thread '") && plain.contains("panicked at") {
            let mut msg = Vec::new();
            while let Some(next) = lines.peek() {
                let next = strip_ansi(next);
                if next.starts_with("note: run with `RUST_BACKTRACE")
                    || next.starts_with("stack backtrace:")
                {
                    break;
                }
                msg.push(lines.next().unwrap_or_default());
            }
            panic = Some(msg.join("\n").trim().to_owned());
        } else if !plain.starts_with("note: run with `RUST_BACKTRACE") {
            rest.push(line);
        }
    }
    (check, panic, rest.join("\n").trim().to_owned())
}

fn json_lines(s: &str) -> impl Iterator<Item = Value> + '_ {
    s.lines()
        .filter(|l| l.starts_with('{'))
        .filter_map(|l| serde_json::from_str(l).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_suites_by_running_order() {
        let stderr = "     Running tests/hidden.rs (target/debug/deps/hidden-1)\n     Running tests/visible.rs (target/debug/deps/visible-2)\n";
        let stdout = r#"{ "type": "suite", "event": "started", "test_count": 1 }
{ "type": "test", "event": "started", "name": "dense" }
{ "type": "test", "name": "dense", "event": "failed", "exec_time": 0.002, "stdout": "ANNEAL {\"input\":\"n = 100\",\"expected\":\"None\",\"got\":\"Some(4294967295)\"}\n\nthread 'dense' (1) panicked at tests/hidden.rs:9:5:\ncheck failed: expected None, got Some(4294967295)\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n" }
{ "type": "suite", "event": "failed", "passed": 0, "failed": 1 }
{ "type": "suite", "event": "started", "test_count": 1 }
{ "type": "test", "event": "started", "name": "single" }
{ "type": "test", "name": "single", "event": "ok", "exec_time": 0.0001 }
"#;
        let t = tests(stdout, stderr);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].suite, t[0].outcome), (Suite::Hidden, Outcome::Failed));
        assert_eq!(
            t[0].check.as_ref().map(|c| c.got.as_str()),
            Some("Some(4294967295)")
        );
        assert_eq!(
            t[0].panic.as_deref(),
            Some("check failed: expected None, got Some(4294967295)")
        );
        assert_eq!(t[0].stdout, "");
        assert_eq!(
            (t[1].suite, t[1].outcome),
            (Suite::Visible, Outcome::Passed)
        );
    }

    #[test]
    fn coloured_cargo_output_parses_like_plain() {
        // Cargo's own lines as it prints them with CARGO_TERM_COLOR=always.
        let stderr = "\x1b[1m\x1b[92m     Running\x1b[0m tests/hidden.rs (target/debug/deps/hidden-1)\n";
        let stdout = "{ \"type\": \"suite\", \"event\": \"started\" }\n{ \"type\": \"test\", \"name\": \"a\", \"event\": \"failed\", \"stdout\": \"\\u001b[32mANNEAL {\\\"input\\\":\\\"i\\\",\\\"expected\\\":\\\"e\\\",\\\"got\\\":\\\"g\\\"}\\n\\u001b[0mthread 'a' (1) panicked at tests/hidden.rs:9:5:\\n\\u001b[31mred reason\\u001b[0m\\n\\u001b[33mprinted\\n\" }\n";
        let t = tests(stdout, stderr);
        assert_eq!(t[0].suite, Suite::Hidden);
        assert_eq!(t[0].check.as_ref().map(|c| c.got.as_str()), Some("g"));
        // The learner's colours are kept for the web app to draw.
        assert_eq!(t[0].panic.as_deref(), Some("\x1b[31mred reason\x1b[0m\n\x1b[33mprinted"));

        let d = build_failure("\x1b[1m\x1b[91merror\x1b[0m: could not compile\n\x1b[0m\n  (signal: \x1b[1m9\x1b[0m, SIG\x1b[0mKILL)\n");
        assert!(d.message.contains("ran out of memory"), "{}", d.message);
        assert!(d.rendered.contains("\x1b[91merror"), "cargo's colours are kept: {:?}", d.rendered);
    }

    #[test]
    fn unfinished_test_stays_timed_out() {
        let stderr = "     Running tests/visible.rs (x)\n";
        let stdout = "{ \"type\": \"suite\", \"event\": \"started\", \"test_count\": 1 }\n{ \"type\": \"test\", \"event\": \"started\", \"name\": \"spins\" }\n";
        let t = tests(stdout, stderr);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].outcome, Outcome::TimedOut);
    }

    fn message(level: &str, text: &str, spans: &str) -> String {
        format!(r#"{{"reason":"compiler-message","message":{{"level":"{level}","message":"{text}","rendered":"{level}: {text}","code":null,"children":[],"spans":{spans}}}}}"#)
    }

    #[test]
    fn an_error_without_a_place_is_still_reported() {
        let out = [message("error", "linking with `cc` failed", "[]"), message("error", "aborting due to 1 previous error", "[]"), message("warning", "1 warning emitted", "[]")].join("\n");
        let found = diagnostics(&out);
        assert_eq!(found.iter().map(|d| d.message.as_str()).collect::<Vec<_>>(), ["linking with `cc` failed"]);
    }

    #[test]
    fn an_error_inside_a_test_macro_points_at_the_test_that_used_it() {
        let spans = r#"[{"file_name":"tests/anneal/prelude.rs","line_start":3,"line_end":3,"column_start":1,"column_end":9,"is_primary":true,"label":"`Batch` cannot be compared","expansion":{"span":{"file_name":"tests/hidden.rs","line_start":7,"line_end":7,"column_start":5,"column_end":20,"is_primary":false,"label":null,"expansion":null}}}]"#;
        let found = diagnostics(&message("error", "binary operation `==` cannot be applied", spans));
        let at = &found[0].spans[0];
        assert_eq!((at.file.as_str(), at.line_start, at.primary, at.label.as_deref()), ("tests/hidden.rs", 7, true, Some("`Batch` cannot be compared")));
    }

    #[test]
    fn a_killed_compiler_says_so() {
        let d = build_failure("   Compiling solution\nerror: could not compile `solution`\n  process didn't exit successfully: `rustc` (signal: 9, SIGKILL: kill)\n");
        assert!(d.message.contains("ran out of memory"), "{}", d.message);
        assert!(d.rendered.contains("SIGKILL"));
    }
}
