//! The line limit against the real exercises: formatting must never decide whether a fix passes.

use std::path::Path;
use std::process::{Command, Stdio};

/// rustfmt as the editor runs it (`POST /api/format`).
fn editor_format(source: &str) -> String {
    use std::io::Write;
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2021", "--emit", "stdout", "--quiet"])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("rustfmt is installed");
    child.stdin.take().expect("piped").write_all(source.as_bytes()).expect("write");
    let out = child.wait_with_output().expect("rustfmt ran");
    assert!(out.status.success(), "rustfmt rejected:\n{source}");
    String::from_utf8(out.stdout).expect("utf-8")
}

#[test]
fn rust_starters_are_formatted_and_solutions_fit_the_limit_either_way() {
    let tracks = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/tracks");
    let (mut unformatted, mut over, mut limited) = (Vec::new(), Vec::new(), 0);
    for track in std::fs::read_dir(&tracks).expect("tracks") {
        let Ok(problems) = std::fs::read_dir(track.expect("entry").path().join("problems")) else {
            continue;
        };
        for problem in problems {
            let dir = problem.expect("entry").path();
            let Ok(starter) = std::fs::read_to_string(dir.join("starter.rs")) else {
                continue;
            };
            // a starter the editor would rewrite makes the learner's first save change lines they never touched
            if editor_format(&starter) != starter {
                unformatted.push(dir.display().to_string());
            }
            let meta = std::fs::read_to_string(dir.join("problem.toml")).expect("problem.toml");
            let Some(max) = meta.lines().find_map(|l| l.strip_prefix("max_changed_lines = ")) else {
                continue;
            };
            let max: u32 = max.trim().parse().expect("a number");
            let solution = std::fs::read_to_string(dir.join("solution.rs")).expect("solution.rs");
            limited += 1;
            let rules = anneal_content::Rules {
                forbid_methods: Vec::new(),
                forbid_types: Vec::new(),
                forbid_unsafe: false,
                max_changed_lines: Some(max),
            };
            for code in [solution.clone(), editor_format(&solution)] {
                for v in anneal_rules::check(&code, &starter, &rules) {
                    over.push(format!("{}: {}", dir.display(), v.message));
                }
            }
        }
    }
    assert!(limited > 20, "found only {limited} line-limited problems");
    assert!(unformatted.is_empty(), "run rustfmt on these starters: {unformatted:#?}");
    assert!(over.is_empty(), "solutions over their limit: {over:#?}");
}
