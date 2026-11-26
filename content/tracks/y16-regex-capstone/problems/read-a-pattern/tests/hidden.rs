use anneal_prelude::Rng;
use solution::*;

#[allow(dead_code)]
fn lit(c: char) -> Ast {
    Ast::Class(Class::literal(c))
}
#[allow(dead_code)]
fn cat(items: Vec<Ast>) -> Ast {
    Ast::Cat(items)
}
#[allow(dead_code)]
fn alt(branches: Vec<Ast>) -> Ast {
    Ast::Alt(branches)
}
#[allow(dead_code)]
fn star(a: Ast) -> Ast {
    Ast::Star(Box::new(a))
}
#[allow(dead_code)]
fn plus(a: Ast) -> Ast {
    Ast::Plus(Box::new(a))
}
#[allow(dead_code)]
fn opt(a: Ast) -> Ast {
    Ast::Opt(Box::new(a))
}

#[test]
fn stacked_quantifiers_nest_outwards() {
    check!("a+?", parse("a+?"), Ok(opt(plus(lit('a')))));
    check!("a?*+", parse("a?*+"), Ok(plus(star(opt(lit('a'))))));
    check!("^*", parse("^*"), Ok(star(Ast::Start)));
    check!("()?", parse("()?"), Ok(opt(Ast::Empty)));
}

#[test]
fn positions_count_characters_and_the_first_problem_wins() {
    check!("éé)", parse("éé)"), Err(ParseError::UnmatchedClose(2)));
    check!("é(é(é", parse("é(é(é"), Err(ParseError::UnclosedGroup(3)));
    check!("(a)(b", parse("(a)(b"), Err(ParseError::UnclosedGroup(3)));
    check!("(*", parse("(*"), Err(ParseError::NothingToRepeat(1)));
    check!(")(", parse(")("), Err(ParseError::UnmatchedClose(0)));
    check!("a)*", parse("a)*"), Err(ParseError::UnmatchedClose(1)));
    check!("(+)", parse("(+)"), Err(ParseError::NothingToRepeat(1)));
    check!("||?", parse("||?"), Err(ParseError::NothingToRepeat(2)));
}

#[test]
fn the_group_limit_is_exact() {
    let ok = "(".repeat(MAX_OPEN_GROUPS) + "a" + &")".repeat(MAX_OPEN_GROUPS);
    check!("100 groups inside each other", parse(&ok), Ok(lit('a')));
    let deep = "(".repeat(MAX_OPEN_GROUPS + 1) + "a" + &")".repeat(MAX_OPEN_GROUPS + 1);
    check!("101 groups inside each other", parse(&deep), Err(ParseError::TooDeep(MAX_OPEN_GROUPS)));
    check!("500 groups side by side", parse(&"(a)".repeat(500)).is_ok(), true);
}

#[test]
fn hostile_patterns_do_not_crash() {
    check!("100 000 opening brackets", parse(&"(".repeat(100_000)), Err(ParseError::TooDeep(MAX_OPEN_GROUPS)));
    let many = "a".to_string() + &"*".repeat(200_000);
    check!("200 000 stars", parse(&many), Err(ParseError::TooDeep(1 + MAX_STACKED)));
    check!("the stack limit counts afresh for each item", parse("a***b+++(c???)***").is_ok(), true);
    check!("a very long alternation", parse(&"a|".repeat(100_000)).is_ok(), true);
    check!("a very long sequence", parse(&"ab".repeat(100_000)).is_ok(), true);
}

/// Writes a tree as pattern text. `level` is the weakest form allowed without brackets.
#[allow(dead_code)]
fn show(ast: &Ast, level: u8) -> String {
    let (own, text) = match ast {
        Ast::Empty => (if level == 0 { 3 } else { 0 }, String::new()),
        Ast::Class(c) => (3, c.ranges[0].0.to_string()),
        Ast::Any => (3, ".".to_string()),
        Ast::Start => (3, "^".to_string()),
        Ast::End => (3, "$".to_string()),
        Ast::Alt(b) => (0, b.iter().map(|x| show_branch(x)).collect::<Vec<_>>().join("|")),
        Ast::Cat(items) => (1, items.iter().map(|x| show(x, 2)).collect()),
        // Brackets round a quantified inner item, so that the stack limit is never met by accident.
        Ast::Star(a) => (2, show(a, 3) + "*"),
        Ast::Plus(a) => (2, show(a, 3) + "+"),
        Ast::Opt(a) => (2, show(a, 3) + "?"),
    };
    if own < level {
        format!("({text})")
    } else {
        text
    }
}

#[allow(dead_code)]
fn show_branch(ast: &Ast) -> String {
    match ast {
        Ast::Empty => String::new(),
        _ => show(ast, 1),
    }
}

#[allow(dead_code)]
fn random(rng: &mut Rng, depth: u32) -> Ast {
    match rng.below(if depth == 0 { 5 } else { 10 }) {
        0 | 1 => lit(*rng.pick(&['a', 'b', 'é', ' ', '-', ']', '{'])),
        2 => rng.pick(&[Ast::Any, Ast::Start, Ast::End]).clone(),
        3 => Ast::Empty,
        4 => lit('c'),
        5 => {
            let n = rng.int(2, 3);
            cat((0..n).map(|_| random(rng, depth - 1)).collect())
        }
        6 => {
            let n = rng.int(2, 3);
            alt((0..n).map(|_| random(rng, depth - 1)).collect())
        }
        7 => star(random(rng, depth - 1)),
        8 => plus(random(rng, depth - 1)),
        _ => opt(random(rng, depth - 1)),
    }
}

#[test]
fn random_trees_written_as_text_read_back() {
    let mut rng = Rng::new(1601);
    for _ in 0..3000 {
        let tree = random(&mut rng, 4);
        let text = show(&tree, 0);
        check!(format!("parse({text:?})"), parse(&text), Ok(tree));
    }
}

#[test]
fn quantifiers_belong_to_one_item_across_branches() {
    check!("a|b*|c", parse("a|b*|c"), Ok(alt(vec![lit('a'), star(lit('b')), lit('c')])));
    check!("(|)", parse("(|)"), Ok(alt(vec![Ast::Empty, Ast::Empty])));
}

#[test]
fn an_unclosed_group_after_closed_ones() {
    check!("((a)(b)", parse("((a)(b)"), Err(ParseError::UnclosedGroup(0)));
    check!("(a|(b|c", parse("(a|(b|c"), Err(ParseError::UnclosedGroup(3)));
}

#[test]
fn random_noise_never_panics_and_errors_point_inside() {
    let mut rng = Rng::new(1611);
    for _ in 0..5000 {
        let n = rng.below(12);
        let text = rng.string(n, "ab()|*+?.^$é");
        match parse(&text) {
            Ok(_) => {}
            Err(ParseError::UnmatchedClose(p) | ParseError::UnclosedGroup(p) | ParseError::NothingToRepeat(p) | ParseError::TooDeep(p)) => {
                check!(format!("{text:?}: position {p} is inside the pattern"), p < n, true);
            }
            Err(other) => check!(format!("{text:?}"), format!("{other:?}"), "an error of this step".to_string()),
        }
    }
}
