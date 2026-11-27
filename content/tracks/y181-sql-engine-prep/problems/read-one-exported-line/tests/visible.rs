use solution::*;

#[allow(dead_code)]
fn values(line: &str) -> Result<Vec<Option<String>>, BadLine> {
    Ok(fields(line)?.into_iter().map(|f| f.value).collect())
}

#[allow(dead_code)]
fn some(all: &[&str]) -> Vec<Option<String>> {
    all.iter().map(|s| Some(s.to_string())).collect()
}

#[allow(dead_code)]
fn bad(problem: Problem, start: usize, end: usize) -> Result<Vec<Field>, BadLine> {
    Err(BadLine { problem, start, end })
}

#[test]
fn plain_fields() {
    check!("tea,3, boxes ", values("tea,3, boxes "), Ok(some(&["tea", "3", " boxes "])));
}

#[test]
fn a_comma_and_a_quote_inside_quotes() {
    check!("a quoted name", values(r#""Nuts, 1kg ""fresh""",4"#), Ok(some(&[r#"Nuts, 1kg "fresh""#, "4"])));
}

#[test]
fn missing_is_not_empty() {
    check!(r#"a,,"""#, values(r#"a,,"""#), Ok(vec![Some("a".into()), None, Some(String::new())]));
}

#[test]
fn ranges_include_the_quotes() {
    let want = vec![Field { value: Some("é".into()), start: 0, end: 4 }, Field { value: None, start: 5, end: 5 }];
    check!(r#""é","#, fields(r#""é","#), Ok(want));
}

#[test]
fn the_three_mistakes() {
    check!("never closed", fields(r#"a,"b"#), bad(Problem::Unclosed, 2, 4));
    check!("text after the quote", fields(r#""a"b,c"#), bad(Problem::AfterQuote, 3, 4));
    check!("a quote in a plain field", fields(r#"5" nail"#), bad(Problem::StrayQuote, 1, 2));
}
