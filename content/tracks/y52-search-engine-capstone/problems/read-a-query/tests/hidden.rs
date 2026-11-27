use solution::*;

fn w(word: &str) -> Expr {
    Expr::Word(word.into())
}

fn not(e: Expr) -> Expr {
    Expr::Not(Box::new(e))
}

#[test]
fn keywords_are_capitals_outside_quotes() {
    check!("a or not", parse_query("a or not"), Ok(Expr::And(vec![w("a"), w("or"), w("not")])));
    check!("\"AND\"", parse_query("\"AND\""), Ok(w("and")));
    check!("ANDY", parse_query("ANDY"), Ok(w("andy")));
    check!("(AND)", parse_query("(AND)"), Err(QueryError::Unexpected(1)));
}

#[test]
fn more_errors() {
    use QueryError::*;
    for (text, want) in [
        ("   ", UnexpectedEnd),
        ("(", UnexpectedEnd),
        ("(a", UnexpectedEnd),
        ("(a OR", UnexpectedEnd),
        ("a OR", UnexpectedEnd),
        ("NOT", UnexpectedEnd),
        ("a NOT", UnexpectedEnd),
        ("()", Unexpected(1)),
        (")", Unexpected(0)),
        ("a AND AND b", Unexpected(6)),
        ("a OR OR b", Unexpected(5)),
        ("a AND OR b", Unexpected(6)),
        ("NOT OR a", Unexpected(4)),
        ("(a OR) b", Unexpected(5)),
        ("a (b)) c", Unexpected(5)),
        ("é) x", Unexpected(1)),
        ("\"", UnterminatedQuote(0)),
        ("\"a\" \"", UnterminatedQuote(4)),
        ("é\"b", UnterminatedQuote(1)),
        (") \"", UnterminatedQuote(2)),
    ] {
        check!(text, parse_query(text), Err(want));
    }
}

#[test]
fn words_end_at_brackets_and_quotes() {
    check!("a(b)c", parse_query("a(b)c"), Ok(Expr::And(vec![w("a"), w("b"), w("c")])));
    check!("x\"y\"z", parse_query("x\"y\"z"), Ok(Expr::And(vec![w("x"), w("y"), w("z")])));
    check!("tab and no-break space", parse_query("a\t\u{a0}b"), Ok(Expr::And(vec![w("a"), w("b")])));
    check!("NOT(a)", parse_query("NOT(a)"), Ok(not(w("a"))));
}

#[test]
fn leaves() {
    check!("--", parse_query("--"), Ok(Expr::Nothing));
    check!("a --", parse_query("a --"), Ok(Expr::And(vec![w("a"), Expr::Nothing])));
    check!("*", parse_query("*"), Ok(Expr::Pattern("*".into())));
    check!("ab*c-d keeps its punctuation", parse_query("Ab*C-D"), Ok(Expr::Pattern("ab*c-d".into())));
    check!("a quoted star", parse_query("\"ab*cd\""), Ok(Expr::Phrase(vec!["ab".into(), "cd".into()])));
    check!("a capital sigma folds the same anywhere", parse_query("*ΟΔΟΣ*"), Ok(Expr::Pattern("*οδοσ*".into())));
}

#[test]
fn nesting_has_a_limit() {
    let brackets = |n: usize| format!("{}a{}", "(".repeat(n), ")".repeat(n));
    check!("50 brackets", parse_query(&brackets(MAX_NESTING)), Ok(w("a")));
    check!("51 brackets", parse_query(&brackets(MAX_NESTING + 1)), Err(QueryError::TooDeep(50)));
    let nots = |n: usize| format!("{}a", "NOT ".repeat(n));
    check!("50 NOTs", parse_query(&nots(MAX_NESTING)).is_ok(), true);
    check!("51 NOTs", parse_query(&nots(MAX_NESTING + 1)), Err(QueryError::TooDeep(200)));
    let mixed = format!("{}a{}", "NOT (".repeat(25), ")".repeat(25));
    check!("25 of each", parse_query(&mixed).is_ok(), true);
    check!("one more", parse_query(&format!("({mixed})")), Err(QueryError::TooDeep(1 + 24 * 5 + 4)));
}

#[test]
fn deep_input_does_not_crash() {
    check!("200000 brackets", parse_query(&"(".repeat(200_000)), Err(QueryError::TooDeep(50)));
    check!("200000 NOTs", parse_query(&"NOT ".repeat(200_000)), Err(QueryError::TooDeep(200)));
}

#[test]
fn levels_are_given_back() {
    for piece in ["(a) ", "NOT a ", "NOT (NOT a) "] {
        let text = piece.repeat(500);
        check!(format!("500 times {piece}"), parse_query(&text).is_ok(), true);
    }
}

#[test]
fn long_flat_queries() {
    for (glue, text) in [("space", "a ".repeat(200_000)), ("AND", format!("a{}", " AND a".repeat(100_000))), ("OR", format!("a{}", " OR a".repeat(100_000)))] {
        let tree = parse_query(&text);
        let items = match &tree {
            Ok(Expr::And(items) | Expr::Or(items)) => items.len(),
            _ => 0,
        };
        check!(format!("{glue}: one flat list"), items >= 100_000, true);
        // using and dropping the tree must not overflow either
        let copy = tree.clone();
        check!(format!("{glue}: equal to its copy"), copy == tree, true);
    }
}

#[test]
fn many_quotes() {
    let text = "\"a b\" ".repeat(100_000);
    check!("100000 quotes", parse_query(&text).is_ok(), true);
}

fn random(rng: &mut anneal_prelude::Rng, depth: usize) -> Expr {
    let pick = if depth == 0 { rng.below(4) } else { rng.below(7) };
    match pick {
        0 | 1 => w(*rng.pick(&["a", "b", "or", "x9"])),
        2 => Expr::Phrase(vec!["c".into(), "d".into()]),
        3 => Expr::Pattern(rng.pick(&["*e", "f*g", "**"]).to_string()),
        4 => not(random(rng, depth - 1)),
        _ => {
            let items = (0..2 + rng.below(3)).map(|_| random(rng, depth - 1)).collect();
            if pick == 5 {
                Expr::And(items)
            } else {
                Expr::Or(items)
            }
        }
    }
}

/// Writes a tree with only the brackets it needs. `needs`: 0 any, 1 not an Or, 2 a single item.
fn print(rng: &mut anneal_prelude::Rng, e: &Expr, needs: usize) -> String {
    let (text, strength) = match e {
        Expr::Nothing => ("\"\"".to_string(), 2),
        Expr::Word(x) => {
            let text = if x == "or" || rng.bool() { x.to_string() } else { x.to_uppercase() };
            (text, 2)
        }
        Expr::Phrase(ws) => (format!("\"{}\"", ws.join(" ")), 2),
        Expr::Pattern(p) => (p.clone(), 2),
        Expr::Not(inner) => (format!("NOT {}", print(rng, inner, 2)), 2),
        // a child of the same kind needs brackets, or it would be flattened by the writer
        Expr::And(items) => {
            let mut text = print(rng, &items[0], 2);
            for item in &items[1..] {
                let glue = *rng.pick(&[" ", " AND ", "  "]);
                text = format!("{text}{glue}{}", print(rng, item, 2));
            }
            (text, 1)
        }
        Expr::Or(items) => {
            let parts: Vec<String> = items.iter().map(|i| print(rng, i, 1)).collect();
            (parts.join(" OR "), 0)
        }
    };
    if strength < needs || (needs == 0 && rng.below(10) == 0) {
        format!("({text})")
    } else {
        text
    }
}

#[test]
fn matches_trees_written_out_as_text() {
    let mut rng = anneal_prelude::Rng::new(5204);
    for _ in 0..1_000 {
        let depth = rng.below(5);
        let tree = random(&mut rng, depth);
        let text = print(&mut rng, &tree, 0);
        check!(text.clone(), parse_query(&text), Ok(tree));
    }
}

#[test]
fn random_text_never_panics() {
    let mut rng = anneal_prelude::Rng::new(5205);
    let pieces = ["(", ")", "\"", " ", "a", "AND", "OR", "NOT", "*", "é", "-"];
    for _ in 0..20_000 {
        let len = rng.below(14);
        let text: String = (0..len).map(|_| *rng.pick(&pieces)).collect();
        let _ = parse_query(&text);
    }
}
