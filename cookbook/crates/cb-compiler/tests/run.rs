//! Run tests: every `tests/run/*.peb` must check cleanly of errors and print the
//! output recorded in the `.stdout` next to it (a runtime error is appended to the output).

mod common;
use common::*;
use pebble::sema::CheckOptions;

fn run_case(path: &std::path::Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    let an = pebble::analyze(&text, &CheckOptions::default());
    if an.has_errors() {
        let file = pebble::source::SourceFile::new(
            path.file_name().unwrap().to_string_lossy().to_string(),
            text,
        );
        return format!(
            "COMPILE ERRORS\n{}",
            pebble::diag::render_all(&an.diagnostics, &file, &Default::default())
        );
    }
    let mut out: Vec<u8> = Vec::new();
    let res = pebble::interp::run(&an.hir, &mut out);
    let mut s = String::from_utf8_lossy(&out).to_string();
    if let Err(e) = res {
        s.push_str(&format!("--- {e}\n"));
    }
    s
}

#[test]
fn run_golden() {
    let files = files_with_ext(&manifest_dir().join("tests/run"), "peb");
    assert!(
        files.len() >= 40,
        "need at least 40 run programs, found {}",
        files.len()
    );
    let failures = big_stack(move || {
        let mut failures = Vec::new();
        for f in files {
            let out = run_case(&f);
            if out.starts_with("COMPILE ERRORS") {
                failures.push(format!("{}: {out}", f.display()));
                continue;
            }
            expect_file(&f.with_extension("stdout"), &out, &mut failures);
        }
        failures
    });
    assert!(
        failures.is_empty(),
        "{} run mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn examples_check_and_run() {
    let files = files_with_ext(&manifest_dir().join("examples"), "peb");
    assert!(files.len() >= 12, "need at least 12 examples");
    big_stack(move || {
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let an = pebble::analyze(&text, &CheckOptions::default());
            assert!(!an.has_errors(), "{} has errors", f.display());
            let mut out = Vec::new();
            pebble::interp::run(&an.hir, &mut out)
                .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        }
    });
}
