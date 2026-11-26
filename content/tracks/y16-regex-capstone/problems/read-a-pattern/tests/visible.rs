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
fn one_item_of_each_kind() {
    check!("a", parse("a"), Ok(lit('a')));
    check!(".", parse("."), Ok(Ast::Any));
    check!("^ and $", parse("^$"), Ok(cat(vec![Ast::Start, Ast::End])));
    check!("the empty pattern", parse(""), Ok(Ast::Empty));
}

#[test]
fn strengths() {
    check!("ab|c", parse("ab|c"), Ok(alt(vec![cat(vec![lit('a'), lit('b')]), lit('c')])));
    check!("ab*", parse("ab*"), Ok(cat(vec![lit('a'), star(lit('b'))])));
    check!("(ab)*", parse("(ab)*"), Ok(star(cat(vec![lit('a'), lit('b')]))));
    check!("a(b|c)+d?", parse("a(b|c)+d?"), Ok(cat(vec![lit('a'), plus(alt(vec![lit('b'), lit('c')])), opt(lit('d'))])));
}

#[test]
fn nothing_is_a_pattern_too() {
    check!("a|", parse("a|"), Ok(alt(vec![lit('a'), Ast::Empty])));
    check!("()", parse("()"), Ok(Ast::Empty));
    check!("()a", parse("()a"), Ok(cat(vec![Ast::Empty, lit('a')])));
}

#[test]
fn groups_stay_one_item() {
    check!("(ab)c", parse("(ab)c"), Ok(cat(vec![cat(vec![lit('a'), lit('b')]), lit('c')])));
    check!("(a|b)|c", parse("(a|b)|c"), Ok(alt(vec![alt(vec![lit('a'), lit('b')]), lit('c')])));
    check!("((a))", parse("((a))"), Ok(lit('a')));
}

#[test]
fn errors_name_a_position() {
    check!("(ab", parse("(ab"), Err(ParseError::UnclosedGroup(0)));
    check!("ab)", parse("ab)"), Err(ParseError::UnmatchedClose(2)));
    check!("a|*", parse("a|*"), Err(ParseError::NothingToRepeat(2)));
}
