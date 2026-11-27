//! A runner for BusTub's `sqllogictest` files (`tools/sqllogictest/` in BusTub). A `.slt` file is a list of records:
//!
//! ```text
//! statement ok            <- the SQL below must run without an error ("statement error": it must fail)
//! create table t(a int);
//!
//! query rowsort           <- the SQL below must produce exactly these rows (rowsort: in any order)
//! select * from t;
//! ----
//! 1
//! ```
//!
//! `+ensure:index_scan`, `+ensure:topn`, ... after `statement`/`query` additionally check the optimizer's plan. Given code.

#![allow(dead_code)]

use std::path::Path;

use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::execution::check_options::{CheckOption, CheckOptions};

#[derive(Debug)]
pub enum Record {
    Halt,
    Sleep(u64),
    Statement { line: usize, is_error: bool, sql: String, extra_options: Vec<String> },
    Query { line: usize, rowsort: bool, sql: String, expected: String, extra_options: Vec<String> },
}

fn extra_options(tokens: &[&str]) -> Vec<String> {
    tokens.iter().filter(|t| t.starts_with('+')).map(|t| t[1..].to_string()).collect()
}

/// Parses a script. A script that says `no test` at the top is "not tested this semester": no records.
pub fn parse(script: &str) -> Vec<Record> {
    let script = script.replace("\r\n", "\n");
    let lines: Vec<&str> = script.split('\n').collect();
    let mut records = vec![];
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }
        let tokens: Vec<&str> = line.split(' ').filter(|t| !t.is_empty()).collect();
        if tokens.is_empty() {
            i += 1;
            continue;
        }
        if tokens[0] == "no" && tokens.get(1) == Some(&"test") {
            return records;
        }
        let loc = i + 1;
        match tokens[0] {
            "halt" => records.push(Record::Halt),
            "sleep" => records.push(Record::Sleep(tokens[1].parse().expect("sleep takes seconds"))),
            "statement" => {
                let is_error = tokens[1] == "error";
                i += 1;
                let mut sql = String::new();
                while i < lines.len() && !lines[i].is_empty() {
                    sql.push_str(lines[i]);
                    sql.push('\n');
                    i += 1;
                }
                records.push(Record::Statement {
                    line: loc,
                    is_error,
                    sql: sql.trim_end().to_string(),
                    extra_options: extra_options(&tokens[2..]),
                });
            }
            "query" => {
                let rowsort = tokens.get(1) == Some(&"rowsort");
                i += 1;
                let mut sql = String::new();
                let mut has_result = false;
                while i < lines.len() && !lines[i].is_empty() {
                    if lines[i] == "----" {
                        has_result = true;
                        break;
                    }
                    sql.push_str(lines[i]);
                    sql.push('\n');
                    i += 1;
                }
                assert!(has_result, "line {loc}: query without ----");
                i += 1;
                let mut expected = String::new();
                while i < lines.len() && !lines[i].is_empty() {
                    expected.push_str(lines[i].trim_end());
                    expected.push('\n');
                    i += 1;
                }
                records.push(Record::Query {
                    line: loc,
                    rowsort,
                    sql: sql.trim_end().to_string(),
                    expected,
                    extra_options: extra_options(&tokens[1..]),
                });
            }
            _ => {}
        }
        i += 1;
    }
    records
}

fn split_lines(text: &str) -> Vec<String> {
    text.split('\n').map(|l| l.trim_end().to_string()).filter(|l| !l.is_empty()).collect()
}

fn results_equal(produced: &str, expected: &str, rowsort: bool) -> bool {
    let (mut a, mut b) = (split_lines(produced), split_lines(expected));
    if rowsort {
        a.sort();
        b.sort();
    }
    a == b
}

/// Is `needle` somewhere after the first `keyword` in `haystack`?
fn contains_after(keyword: &str, haystack: &str, needle: &str) -> bool {
    haystack.find(keyword).is_some_and(|at| haystack[at..].contains(needle))
}

/// The `+ensure:...` checks: look at the optimized plan. Returns an error message if one fails.
fn process_extra_options(
    sql: &str,
    db: &BusTubInstance,
    options: &[String],
    check_options: &mut CheckOptions,
    skip: &[&str],
) -> Result<(), String> {
    for opt in options {
        if let Some(what) = opt.strip_prefix("ensure:") {
            if skip.contains(&what) {
                continue;
            }
            let mut plan = String::new();
            db.execute_sql(&format!("explain (o) {sql}"), &mut SimpleStreamWriter::new(&mut plan, true, "\t"), None)
                .map_err(|e| e.to_string())?;
            let count = |s: &str| plan.matches(s).count();
            match what {
                "index_scan" if !plan.contains("IndexScan") => return Err("IndexScan not found".into()),
                "seq_scan" if plan.contains("IndexScan") || contains_after("OPTIMIZER", &plan, "Filter") => {
                    return Err("SeqScan on not indexed columns".into())
                }
                "hash_join" if count("HashJoin") != 1 && !plan.contains("Filter") => return Err("HashJoin not found".into()),
                "hash_join_no_filter" if count("HashJoin") != 1 || contains_after("OPTIMIZER", &plan, "Filter") => {
                    return Err("Push all filters into HashJoin".into())
                }
                "hash_join*2" if count("HashJoin") != 2 && !plan.contains("Filter") => {
                    return Err("HashJoin should appear exactly twice".into())
                }
                "hash_join*3" if count("HashJoin") != 3 && !plan.contains("Filter") => {
                    return Err("HashJoin should appear exactly thrice".into())
                }
                "topn" => {
                    if !plan.contains("TopN") {
                        return Err("TopN not found".into());
                    }
                    check_options.check_options_set.insert(CheckOption::EnableTopnCheck);
                }
                "topn*2" => {
                    if count("TopN") != 2 {
                        return Err("TopN should appear exactly twice".into());
                    }
                    check_options.check_options_set.insert(CheckOption::EnableTopnCheck);
                }
                "index_join" if !plan.contains("NestedIndexJoin") => return Err("NestedIndexJoin not found".into()),
                "nlj_init_check" => {
                    if !plan.contains("NestedLoopJoin") {
                        return Err("NestedLoopJoin not found".into());
                    }
                    check_options.check_options_set.insert(CheckOption::EnableNljCheck);
                }
                w if w.starts_with("column-pruned") => {
                    let args: Vec<&str> = what.split(':').collect();
                    assert_eq!(args.len(), 3, "unsupported extra option: {opt}");
                    let (expected_proj, expected_agg): (usize, usize) = (args[1].parse().unwrap(), args[2].parse().unwrap());
                    for line in plan.lines() {
                        let line = line.trim_start();
                        if line.starts_with("Agg") {
                            let cols: Vec<&str> = line.split("],").collect();
                            if cols.len() != 3 {
                                return Err("Agg plan wrong formatting!".into());
                            }
                            if cols[..2].iter().any(|c| c.matches("\",").count() + 1 > expected_agg) {
                                return Err("Agg wrong column pruning count!".into());
                            }
                            break;
                        }
                        if line.starts_with("Projection") && line.matches("\",").count() + 1 > expected_proj {
                            return Err("Projection wrong column pruning count!".into());
                        }
                    }
                }
                "index_scan" | "seq_scan" | "hash_join" | "hash_join_no_filter" | "hash_join*2" | "hash_join*3" | "index_join" => {}
                _ => panic!("unsupported extra option: {opt}"),
            }
        } else if opt.starts_with("timing") || opt.starts_with("explain") {
            // timing and plan printing are for people, not for the pass/fail result
        } else {
            panic!("unsupported extra option: {opt}");
        }
    }
    Ok(())
}

/// A database for a test file: in memory, with the mock and test tables.
pub fn new_instance(bpm_size: usize) -> BusTubInstance {
    let db = BusTubInstance::new(bpm_size);
    db.generate_mock_table();
    db.generate_test_table();
    db
}

/// Runs one script against `db`. `Err(message)` names the first record that failed.
pub fn run_script(db: &BusTubInstance, name: &str, script: &str) -> Result<(), String> {
    run_script_skipping(db, name, script, &[])
}

/// Like `run_script`, but the `+ensure:<kind>` checks named in `skip` are not made (BusTub's own comments say "you could disable this ensure if you
/// haven't implemented it yet"; the optimizer rule behind `hash_join` is written in a later module).
pub fn run_script_skipping(db: &BusTubInstance, name: &str, script: &str, skip: &[&str]) -> Result<(), String> {
    for record in parse(script) {
        match record {
            Record::Halt => return Ok(()),
            Record::Sleep(s) => std::thread::sleep(std::time::Duration::from_secs(s)),
            Record::Statement { line, is_error, sql, extra_options } => {
                let mut check_options = CheckOptions::default();
                process_extra_options(&sql, db, &extra_options, &mut check_options, skip)
                    .map_err(|e| format!("{name}:{line}: {e}\n{sql}"))?;
                let mut out = String::new();
                let result = db.execute_sql(&sql, &mut SimpleStreamWriter::new(&mut out, true, "\t"), Some(&check_options));
                match (result, is_error) {
                    (Ok(_), true) => return Err(format!("{name}:{line}: statement should error\n{sql}")),
                    (Err(e), false) => return Err(format!("{name}:{line}: unexpected error: {e}\n{sql}")),
                    _ => {}
                }
            }
            Record::Query { line, rowsort, sql, expected, extra_options } => {
                let mut check_options = CheckOptions::default();
                process_extra_options(&sql, db, &extra_options, &mut check_options, skip)
                    .map_err(|e| format!("{name}:{line}: {e}\n{sql}"))?;
                let mut out = String::new();
                db.execute_sql(&sql, &mut SimpleStreamWriter::new(&mut out, true, " "), Some(&check_options))
                    .map_err(|e| format!("{name}:{line}: unexpected error: {e}\n{sql}"))?;
                if !results_equal(&out, &expected, rowsort) {
                    let mut got = split_lines(&out);
                    let mut want = split_lines(&expected);
                    if rowsort {
                        got.sort();
                        want.sort();
                    }
                    return Err(format!(
                        "{name}:{line}: wrong result (rowsort={rowsort})\n{sql}\n--- got ---\n{}\n--- expected ---\n{}",
                        got.iter().take(30).cloned().collect::<Vec<_>>().join("\n"),
                        want.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Runs the file `tests/sql/<file>` against a fresh database with a buffer pool of `bpm_size` frames. Panics with the failure.
pub fn run_slt(file: &str, bpm_size: usize) {
    run_slt_skipping(file, bpm_size, &[]);
}

/// `run_slt` without the `+ensure:` checks named in `skip`.
pub fn run_slt_skipping(file: &str, bpm_size: usize, skip: &[&str]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sql").join(file);
    let script = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let db = new_instance(bpm_size);
    if let Err(msg) = run_script_skipping(&db, file, &script, skip) {
        panic!("{msg}");
    }
}
