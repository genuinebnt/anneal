use solution::*;

#[allow(dead_code)]
fn ok(s: &str) -> Result<String, ParseError> {
    Ok(s.to_string())
}

#[test]
fn atoms() {
    check!("42", parse("42"), ok("42"));
    check!("  _a1\n", parse("  _a1\n"), ok("_a1"));
    check!("((x))", parse("((x))"), ok("x"));
}

#[test]
fn empty_and_leftovers() {
    check!("empty", parse(""), Err(ParseError::UnexpectedEnd(0)));
    check!("spaces", parse("  "), Err(ParseError::UnexpectedEnd(2)));
    check!("1 2", parse("1 2"), Err(ParseError::UnexpectedToken(2)));
    check!("12ab", parse("12ab"), Err(ParseError::UnexpectedToken(2)));
    check!("a)", parse("a)"), Err(ParseError::UnexpectedToken(1)));
}

#[test]
fn unexpected_characters_come_first() {
    check!("1 + $", parse("1 + $"), Err(ParseError::UnexpectedChar(4)));
    check!("a & b", parse("a & b"), Err(ParseError::UnexpectedChar(2)));
    check!(") é", parse(") é"), Err(ParseError::UnexpectedChar(2)));
}

#[test]
fn argument_lists() {
    check!("f(a = 1, g(2, 3))", parse("f(a = 1, g(2, 3))"), ok("(call f (= a 1) (call g 2 3))"));
    check!("f(1,)", parse("f(1,)"), Err(ParseError::UnexpectedToken(4)));
    check!("f(,1)", parse("f(,1)"), Err(ParseError::UnexpectedToken(2)));
    check!("f(1 2)", parse("f(1 2)"), Err(ParseError::UnexpectedToken(4)));
    check!("a[]", parse("a[]"), Err(ParseError::UnexpectedToken(2)));
    check!("a[1)", parse("a[1)"), Err(ParseError::UnexpectedToken(3)));
}

#[test]
fn postfix_binds_tighter_than_everything() {
    check!("-f(x)^a[0]", parse("-f(x)^a[0]"), ok("(- (^ (call f x) (index a 0)))"));
    check!("!!a", parse("!!a"), ok("(! (! a))"));
}

#[test]
fn comparisons_do_not_chain() {
    check!("(a < b) < c", parse("(a < b) < c"), ok("(< (< a b) c)"));
    check!("a < b && c < d", parse("a < b && c < d"), ok("(&& (< a b) (< c d))"));
    check!("a == b + 1 != c", parse("a == b + 1 != c"), Err(ParseError::ChainedComparison(11)));
    check!("x = a < b || c > d", parse("x = a < b || c > d"), ok("(= x (|| (< a b) (> c d)))"));
    check!("a <= b >= c", parse("a <= b >= c"), Err(ParseError::ChainedComparison(7)));
}

#[test]
fn assignment_targets() {
    check!("a[i + 1] = 2", parse("a[i + 1] = 2"), ok("(= (index a (+ i 1)) 2)"));
    check!("(a) = 1", parse("(a) = 1"), ok("(= a 1)"));
    check!("5 = x", parse("5 = x"), Err(ParseError::BadAssignTarget(2)));
    check!("f() = 1", parse("f() = 1"), Err(ParseError::BadAssignTarget(4)));
    check!("-a = 1", parse("-a = 1"), Err(ParseError::BadAssignTarget(3)));
    check!("a || b = 1", parse("a || b = 1"), Err(ParseError::BadAssignTarget(7)));
}

#[test]
fn two_hundred_levels_parse() {
    let source = format!("{}x{}", "(".repeat(200), ")".repeat(200));
    check!("200 pairs of parentheses", parse(&source), ok("x"));
}

#[test]
fn hostile_nesting_is_refused() {
    for opener in ["(", "-", "f(", "a=", "a^"] {
        let source = opener.repeat(100_000) + "x";
        let got = parse(&source);
        check!(format!("{opener:?} repeated 100 000 times gives TooDeep"), matches!(got, Err(ParseError::TooDeep(_))), true);
    }
}

#[test]
fn long_flat_input_is_not_deep() {
    let source = vec!["1"; 3_000].join("+");
    check!("3000 additions are accepted", parse(&source).map(|s| s.matches('+').count()), Ok(2999));
}

enum Tree {
    Atom(String),
    Binary(&'static str, Box<Tree>, Box<Tree>),
    Prefix(&'static str, Box<Tree>),
    Index(Box<Tree>, Box<Tree>),
}

/// How tightly a node holds together, as in the table.
fn level(t: &Tree) -> u8 {
    match t {
        Tree::Atom(_) | Tree::Index(..) => 9,
        Tree::Binary(op, ..) => match *op {
            "||" => 2,
            "&&" => 3,
            "==" | "<" | ">=" => 4,
            "+" | "-" => 5,
            "*" | "%" => 6,
            _ => 8,
        },
        Tree::Prefix(..) => 7,
    }
}

fn sexpr(t: &Tree) -> String {
    match t {
        Tree::Atom(a) => a.clone(),
        Tree::Binary(op, l, r) => format!("({op} {} {})", sexpr(l), sexpr(r)),
        Tree::Prefix(op, x) => format!("({op} {})", sexpr(x)),
        Tree::Index(b, i) => format!("(index {} {})", sexpr(b), sexpr(i)),
    }
}

/// Source text with only the parentheses the grammar needs.
fn source(t: &Tree) -> String {
    let wrap = |child: &Tree, needed: bool| if needed { format!("({})", source(child)) } else { source(child) };
    match t {
        Tree::Atom(a) => a.clone(),
        Tree::Index(b, i) => format!("{}[{}]", wrap(b, level(b) < 9), source(i)),
        Tree::Prefix(op, x) => format!("{op}{}", wrap(x, level(x) < 7)),
        Tree::Binary(op, l, r) => {
            let mine = level(t);
            let (left, right) = match mine {
                // Power: right associative, and a prefix operator is fine on the right only.
                8 => (level(l) < 9, level(r) < 7),
                // Comparisons do not chain.
                4 => (level(l) <= 4, level(r) <= 4),
                _ => (level(l) < mine, level(r) <= mine),
            };
            format!("{} {op} {}", wrap(l, left), wrap(r, right))
        }
    }
}

fn tree(rng: &mut anneal_prelude::Rng, depth: usize) -> Tree {
    if depth == 0 || rng.below(4) == 0 {
        return Tree::Atom((*rng.pick(&["a", "b", "7", "x_1"])).to_string());
    }
    match rng.below(10) {
        0 => Tree::Prefix(if rng.bool() { "-" } else { "!" }, Box::new(tree(rng, depth - 1))),
        1 => Tree::Index(Box::new(tree(rng, depth - 1)), Box::new(tree(rng, depth - 1))),
        _ => {
            let op = *rng.pick(&["||", "&&", "==", "<", ">=", "+", "-", "*", "%", "^"]);
            let left = tree(rng, depth - 1);
            let right = tree(rng, depth - 1);
            Tree::Binary(op, Box::new(left), Box::new(right))
        }
    }
}

#[test]
fn random_trees_survive_printing_and_parsing() {
    let mut rng = anneal_prelude::Rng::new(8123);
    for _ in 0..1500 {
        let t = tree(&mut rng, 5);
        let text = source(&t);
        check!(text.clone(), parse(&text), Ok(sexpr(&t)));
    }
}
