use solution::*;

use std::error::Error;
use std::io;

#[allow(dead_code)]
fn row_message(line: &str) -> Result<Item, String> {
    parse_row(line).map_err(|e| e.to_string())
}

#[test]
fn empty_file() {
    check!("an empty file", import("e", |_| Ok(String::new())).map_err(|e| e.to_string()), Ok(Vec::new()));
}

#[test]
fn read_is_given_the_path() {
    let mut seen = String::new();
    let _ = import("dir/x.csv", |p| {
        seen = p.to_string();
        Ok(String::new())
    });
    check!("path passed to read", seen, "dir/x.csv".to_string());
}

#[test]
fn first_bad_line_wins() {
    let err = import("a", |_| Ok("a,1\nb,0\nc,x\n".to_string())).unwrap_err();
    check!("two bad lines", chain(&err), vec!["line 2 is invalid".to_string(), "quantity must be at least 1".to_string()]);
}

#[test]
fn crlf_and_blank_lines_count() {
    let err = import("a", |_| Ok("a,1\r\n\r\n\r\nbroken\r\n".to_string())).unwrap_err();
    check!("CRLF file, bad 4th line", chain(&err), vec!["line 4 is invalid".to_string(), "missing quantity".to_string()]);
}

#[test]
fn second_comma_is_part_of_the_quantity() {
    check!(r#"parse_row("a,1,2")"#, row_message("a,1,2"), Err("quantity \"1,2\" is not a number".to_string()));
}

#[test]
fn sku_is_checked_first() {
    check!(r#"parse_row(",")"#, row_message(","), Err("missing sku".to_string()));
    check!(r#"parse_row("a,  ")"#, row_message("a,  "), Err("missing quantity".to_string()));
}

#[test]
fn u32_bounds() {
    check!("u32::MAX", row_message("a,4294967295").map(|i| i.quantity), Ok(u32::MAX));
    check!("one more", row_message("a,4294967296"), Err("quantity \"4294967296\" is not a number".to_string()));
    check!("negative", row_message("a,-1"), Err("quantity \"-1\" is not a number".to_string()));
}

#[test]
fn bad_quantity_source_is_the_parse_error() {
    let err = parse_row("a,x").unwrap_err();
    let is_parse = err.source().map(|s| s.downcast_ref::<std::num::ParseIntError>().is_some());
    check!("source() of BadQuantity is a ParseIntError", is_parse, Some(true));
}

#[test]
fn leaf_errors_have_no_source() {
    check!("ZeroQuantity", parse_row("a,0").unwrap_err().source().is_none(), true);
    check!("MissingField", parse_row("a").unwrap_err().source().is_none(), true);
}

#[test]
fn bad_row_source_is_the_row_error() {
    let err = import("a", |_| Ok("a,0".to_string())).unwrap_err();
    let is_row = err.source().and_then(|s| s.downcast_ref::<RowError>()).map(|r| matches!(r, RowError::ZeroQuantity));
    check!("source() of BadRow", is_row, Some(true));
}

#[test]
fn every_retryable_kind() {
    use io::ErrorKind::*;
    for (kind, want) in [(Interrupted, true), (TimedOut, true), (WouldBlock, true), (NotFound, false), (UnexpectedEof, false), (Other, false)] {
        let err = import("a", |_| Err(io::Error::from(kind))).unwrap_err();
        check!(format!("{kind:?}"), is_retryable(&err), want);
    }
}

#[test]
fn a_bad_row_is_never_retryable() {
    let err = import("a", |_| Ok("a,0".to_string())).unwrap_err();
    check!("a bad row", is_retryable(&err), false);
}

#[test]
fn chain_works_on_any_error() {
    #[derive(Debug)]
    struct Outer(io::Error);
    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("outer")
        }
    }
    impl Error for Outer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }
    check!("a two-level error from the test", chain(&Outer(io::Error::other("inner"))), vec!["outer".to_string(), "inner".to_string()]);
}

#[test]
fn random_files_report_the_first_bad_line() {
    let mut rng = anneal_prelude::Rng::new(8101);
    for _ in 0..300 {
        let len = rng.below(8);
        let mut lines = Vec::new();
        let mut want: Result<Vec<Item>, usize> = Ok(Vec::new());
        for n in 1..=len {
            let q = rng.int(0, 5) as u32;
            let (line, item) = match rng.below(5) {
                0 => (String::new(), None),
                1 => ("oops".to_string(), Some(Err(()))),
                _ if q == 0 => ("s,0".to_string(), Some(Err(()))),
                _ => (format!(" s{n} , {q} "), Some(Ok(Item { sku: format!("s{n}"), quantity: q }))),
            };
            lines.push(line);
            match (item, &mut want) {
                (Some(Ok(i)), Ok(v)) => v.push(i),
                (Some(Err(())), Ok(_)) => want = Err(n),
                _ => {}
            }
        }
        let text = lines.join("\n");
        let got = match import("r", |_| Ok(text.clone())) {
            Ok(v) => Ok(v),
            Err(ImportError::BadRow { line, .. }) => Err(line),
            Err(ImportError::Read { .. }) => Err(0),
        };
        check!(format!("{text:?}"), got, want);
    }
}
