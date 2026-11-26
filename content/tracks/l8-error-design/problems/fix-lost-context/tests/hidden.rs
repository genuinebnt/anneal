use solution::*;

use std::collections::HashMap;
use std::fmt;
use std::num::ParseIntError;

#[allow(dead_code)]
fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect()
}

#[allow(dead_code)]
fn layers(err: &anyhow::Error) -> Vec<String> {
    err.chain().map(|c| c.to_string()).collect()
}

/// Somebody else's error type, with the real cause underneath.
#[derive(Debug)]
struct Wrapper(Box<dyn std::error::Error + Send + Sync + 'static>);

impl fmt::Display for Wrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("worker failed")
    }
}

impl std::error::Error for Wrapper {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[test]
fn missing_second_key() {
    let err = load(&map(&[("api.workers", "1")]), "api").unwrap_err();
    check!("queue absent", layers(&err), vec!["loading api".to_string(), "missing key `api.queue`".to_string()]);
}

#[test]
fn the_root_is_a_missing_key_value() {
    let err = load(&map(&[]), "billing").unwrap_err();
    let key = err.downcast_ref::<MissingKey>().map(|m| m.0.clone());
    check!("downcast_ref::<MissingKey>", key, Some("billing.workers".to_string()));
}

#[test]
fn the_root_is_the_parse_error() {
    let err = load(&map(&[("api.workers", "1"), ("api.queue", "")]), "api").unwrap_err();
    check!("empty queue value: downcast_ref::<ParseIntError>", err.downcast_ref::<ParseIntError>().is_some(), true);
    check!("last layer", layers(&err).pop(), Some("cannot parse integer from empty string".to_string()));
}

#[test]
fn a_value_that_says_missing_is_still_a_bad_number() {
    let err = load(&map(&[("api.workers", "missing")]), "api").unwrap_err();
    check!("api.workers = missing", exit_code(&err), 3);
}

#[test]
fn a_service_called_reading_missing() {
    let err = load(&map(&[("missing-reading.workers", "0")]), "missing-reading").unwrap_err();
    check!("zero workers for a service with a misleading name", exit_code(&err), 1);
}

#[test]
fn other_peoples_errors_around_the_cause() {
    let missing = anyhow::Error::new(Wrapper(Box::new(MissingKey("k".into())))).context("outer");
    check!("context(Wrapper(MissingKey))", exit_code(&missing), 2);
    let parse = anyhow::Error::new(Wrapper(Box::new("x".parse::<u8>().unwrap_err())));
    check!("Wrapper(ParseIntError)", exit_code(&parse), 3);
}

#[test]
fn unrelated_errors_are_one() {
    let plain = anyhow::anyhow!("missing key `looks.like.one`");
    check!("a plain message that only looks like a missing key", exit_code(&plain), 1);
}

#[test]
fn raw_and_number_work_alone() {
    let m = map(&[("a", " 7 ")]);
    check!("raw keeps the text as stored", raw(&m, "a").ok(), Some(" 7 "));
    check!("number trims", number(&m, "a").ok(), Some(7));
    let err = number(&m, "b").unwrap_err();
    check!("number on an absent key has one layer", layers(&err), vec!["missing key `b`".to_string()]);
}

#[test]
fn every_failure_has_the_right_depth() {
    let mut rng = anneal_prelude::Rng::new(8102);
    let values = ["", "0", "1", "9", "-1", "q", "missing", "4294967296"];
    for _ in 0..300 {
        let mut m = HashMap::new();
        let w = if rng.below(4) == 0 { None } else { Some(*rng.pick(&values)) };
        let q = if rng.below(4) == 0 { None } else { Some(*rng.pick(&values)) };
        if let Some(w) = w {
            m.insert("s.workers".to_string(), w.to_string());
        }
        if let Some(q) = q {
            m.insert("s.queue".to_string(), q.to_string());
        }
        let class = |v: Option<&str>| match v {
            None => Err((2, 2)),
            Some(t) => t.parse::<u32>().map_err(|_| (3, 3)),
        };
        let want = match class(w) {
            Err(e) => Err(e),
            Ok(0) => Err((1, 2)),
            Ok(_) => class(q).map(|_| ()),
        };
        let got = load(&m, "s").map(|_| ()).map_err(|e| (exit_code(&e), e.chain().count()));
        check!(format!("workers={w:?} queue={q:?} (exit code, chain length)"), got, want);
    }
}
