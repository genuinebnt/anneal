use solution::*;

use std::error::Error;
use std::io;

#[allow(dead_code)]
fn item(sku: &str, quantity: u32) -> Item {
    Item { sku: sku.to_string(), quantity }
}

#[test]
fn good_file() {
    let got = import("a.csv", |_| Ok("apple, 3\n\n pear ,10\n".to_string())).map_err(|e| format!("{e:?}"));
    check!(r#""apple, 3\n\n pear ,10\n""#, got, Ok(vec![item("apple", 3), item("pear", 10)]));
}

#[test]
fn bad_row_reports_the_line() {
    let err = import("a.csv", |_| Ok("apple,3\n\npear,many\n".to_string())).unwrap_err();
    check!("a bad third line: Display", err.to_string(), "line 3 is invalid".to_string());
    check!("a bad third line: variant", matches!(err, ImportError::BadRow { line: 3, source: RowError::BadQuantity { .. } }), true);
}

#[test]
fn chain_says_each_thing_once() {
    let err = import("a.csv", |_| Ok("pear,many".to_string())).unwrap_err();
    check!(
        "chain of \"pear,many\"",
        chain(&err),
        vec!["line 1 is invalid".to_string(), "quantity \"many\" is not a number".to_string(), "invalid digit found in string".to_string()]
    );
}

#[test]
fn read_failure_keeps_the_io_error() {
    let err = import("stock.csv", |_| Err(io::Error::new(io::ErrorKind::NotFound, "no such file"))).unwrap_err();
    check!("Display", err.to_string(), "cannot read stock.csv".to_string());
    let kind = err.source().and_then(|s| s.downcast_ref::<io::Error>()).map(|e| e.kind());
    check!("source() is the io::Error", kind, Some(io::ErrorKind::NotFound));
}

#[test]
fn zero_and_missing() {
    check!(r#"parse_row("apple,0")"#, parse_row("apple,0").map_err(|e| e.to_string()), Err("quantity must be at least 1".to_string()));
    check!(r#"parse_row("apple")"#, parse_row("apple").map_err(|e| e.to_string()), Err("missing quantity".to_string()));
    check!(r#"parse_row(" ,4")"#, parse_row(" ,4").map_err(|e| e.to_string()), Err("missing sku".to_string()));
}

#[test]
fn retry_decision() {
    let timeout = import("a", |_| Err(io::Error::from(io::ErrorKind::TimedOut))).unwrap_err();
    let denied = import("a", |_| Err(io::Error::from(io::ErrorKind::PermissionDenied))).unwrap_err();
    check!("TimedOut, PermissionDenied", (is_retryable(&timeout), is_retryable(&denied)), (true, false));
}
