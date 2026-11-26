use solution::*;

#[allow(dead_code)]
fn ok(s: &str) -> Result<String, ParseError> {
    Ok(s.to_string())
}

#[test]
fn precedence() {
    check!("1 + 2 * 3 - 4", parse("1 + 2 * 3 - 4"), ok("(- (+ 1 (* 2 3)) 4)"));
    check!("a || b && c == d + 1", parse("a || b && c == d + 1"), ok("(|| a (&& b (== c (+ d 1))))"));
}

#[test]
fn associativity() {
    check!("a - b - c", parse("a - b - c"), ok("(- (- a b) c)"));
    check!("2 ^ 3 ^ 2", parse("2 ^ 3 ^ 2"), ok("(^ 2 (^ 3 2))"));
    check!("a = b = 1", parse("a = b = 1"), ok("(= a (= b 1))"));
}

#[test]
fn prefix_and_power() {
    check!("-2^2", parse("-2^2"), ok("(- (^ 2 2))"));
    check!("2^-1", parse("2^-1"), ok("(^ 2 (- 1))"));
    check!("-a * !b", parse("-a * !b"), ok("(* (- a) (! b))"));
}

#[test]
fn calls_and_indexing() {
    check!("a + f(b, 2) * -c", parse("a + f(b, 2) * -c"), ok("(+ a (* (call f b 2) (- c)))"));
    check!("f()[2](x)", parse("f()[2](x)"), ok("(call (index (call f) 2) x)"));
}

#[test]
fn errors_point_at_the_token() {
    check!("1 + * 2", parse("1 + * 2"), Err(ParseError::UnexpectedToken(4)));
    check!("(1 + 2", parse("(1 + 2"), Err(ParseError::UnexpectedEnd(6)));
    check!("a < b < c", parse("a < b < c"), Err(ParseError::ChainedComparison(6)));
    check!("a + 1 = 2", parse("a + 1 = 2"), Err(ParseError::BadAssignTarget(6)));
}
