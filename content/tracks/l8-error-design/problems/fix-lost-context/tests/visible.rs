use solution::*;

use std::collections::HashMap;

#[allow(dead_code)]
fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect()
}

#[allow(dead_code)]
fn layers(err: &anyhow::Error) -> Vec<String> {
    err.chain().map(|c| c.to_string()).collect()
}

#[test]
fn good_config() {
    let m = map(&[("api.workers", "4"), ("api.queue", " 100 ")]);
    check!("workers=4, queue=100", load(&m, "api").ok(), Some(Limits { workers: 4, queue: 100 }));
}

#[test]
fn missing_key_chain() {
    let err = load(&map(&[]), "api").unwrap_err();
    check!("empty map", layers(&err), vec!["loading api".to_string(), "missing key `api.workers`".to_string()]);
}

#[test]
fn bad_number_chain() {
    let err = load(&map(&[("api.workers", "four")]), "api").unwrap_err();
    check!(
        "api.workers = four",
        layers(&err),
        vec!["loading api".to_string(), "reading `api.workers`".to_string(), "invalid digit found in string".to_string()]
    );
}

#[test]
fn exit_codes() {
    let missing = load(&map(&[]), "api").unwrap_err();
    let bad = load(&map(&[("api.workers", "x")]), "api").unwrap_err();
    let zero = load(&map(&[("api.workers", "0")]), "api").unwrap_err();
    check!("missing, bad number, zero", (exit_code(&missing), exit_code(&bad), exit_code(&zero)), (2, 3, 1));
}

#[test]
fn extra_context_changes_nothing() {
    let err = load(&map(&[]), "api").unwrap_err().context("starting up").context("boot");
    check!("two more layers of context", exit_code(&err), 2);
}

#[test]
fn report_joins_the_chain() {
    let err = load(&map(&[("db.workers", "0")]), "db").unwrap_err();
    check!("report", report(&err), "loading db: workers must be at least 1".to_string());
}
