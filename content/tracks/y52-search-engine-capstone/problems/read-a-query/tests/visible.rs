use solution::*;

#[allow(dead_code)]
fn w(word: &str) -> Expr {
    Expr::Word(word.into())
}

#[allow(dead_code)]
fn not(e: Expr) -> Expr {
    Expr::Not(Box::new(e))
}

#[test]
fn words_side_by_side_and_with_and() {
    check!("rust async", parse_query("rust async"), Ok(Expr::And(vec![w("rust"), w("async")])));
    check!("Rust AND ASYNC", parse_query("Rust AND ASYNC"), Ok(Expr::And(vec![w("rust"), w("async")])));
    check!("a b c", parse_query("a b c"), Ok(Expr::And(vec![w("a"), w("b"), w("c")])));
    check!("one word", parse_query("  rust "), Ok(w("rust")));
}

#[test]
fn or_binds_loosest_and_not_tightest() {
    check!(
        "a b OR NOT c d",
        parse_query("a b OR NOT c d"),
        Ok(Expr::Or(vec![
            Expr::And(vec![w("a"), w("b")]),
            Expr::And(vec![not(w("c")), w("d")]),
        ]))
    );
}

#[test]
fn brackets() {
    check!(
        "a (b OR c)",
        parse_query("a (b OR c)"),
        Ok(Expr::And(vec![w("a"), Expr::Or(vec![w("b"), w("c")])]))
    );
    check!(
        "(a b) c is not flattened",
        parse_query("(a b) c"),
        Ok(Expr::And(vec![Expr::And(vec![w("a"), w("b")]), w("c")]))
    );
    check!("((a))", parse_query("((a))"), Ok(w("a")));
}

#[test]
fn quotes_patterns_and_words_that_split() {
    let phrase = |ws: &[&str]| Expr::Phrase(ws.iter().map(|s| s.to_string()).collect());
    check!("a quote", parse_query("\"Memory (OR) ordering\""), Ok(phrase(&["memory", "or", "ordering"])));
    check!("don't", parse_query("don't"), Ok(phrase(&["don", "t"])));
    check!("*Cast*", parse_query("*Cast*"), Ok(Expr::Pattern("*cast*".into())));
    check!("nothing to find", parse_query("\"\""), Ok(Expr::Nothing));
}

#[test]
fn errors() {
    check!("empty", parse_query(""), Err(QueryError::UnexpectedEnd));
    check!("a AND", parse_query("a AND"), Err(QueryError::UnexpectedEnd));
    check!("OR a", parse_query("OR a"), Err(QueryError::Unexpected(0)));
    check!("a) b", parse_query("a) b"), Err(QueryError::Unexpected(1)));
    check!("an open quote", parse_query("rust \""), Err(QueryError::UnterminatedQuote(5)));
}
