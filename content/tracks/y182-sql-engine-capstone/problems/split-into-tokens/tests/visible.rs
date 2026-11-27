use solution::Token as T;
use solution::*;

#[allow(dead_code)]
fn word(w: &str) -> T {
    T::Word(w.to_string())
}

#[allow(dead_code)]
fn tokens(sql: &str) -> Result<Vec<T>, SyntaxError> {
    Ok(lex(sql)?.into_iter().map(|t| t.0).collect())
}

#[allow(dead_code)]
fn bad(problem: Problem, start: usize, end: usize) -> Result<Vec<T>, SyntaxError> {
    Err(SyntaxError { problem, start, end })
}

#[test]
fn words_numbers_and_symbols_with_their_places() {
    let want = vec![(word("select"), 0, 6), (word("a_1"), 7, 10), (T::Symbol("<="), 10, 12), (T::Number(42), 12, 14), (T::Symbol(";"), 14, 15)];
    check!("SELECT A_1<=42;", lex("SELECT A_1<=42;"), Ok(want));
}

#[test]
fn a_text_with_quotes_inside() {
    check!("'It''s Ok'", lex("'It''s Ok'"), Ok(vec![(T::Text("It's Ok".into()), 0, 10)]));
    check!("an empty text", tokens("''"), Ok(vec![T::Text(String::new())]));
}

#[test]
fn the_longer_symbol_wins() {
    let want = ["<>", "<>", ">=", "<", "="].map(T::Symbol).to_vec();
    check!("<> != >= < =", tokens("<>!=>=< ="), Ok(want));
}

#[test]
fn a_character_that_starts_nothing() {
    check!("#", tokens("a # b"), bad(Problem::BadCharacter, 2, 3));
    check!("a wide one", tokens("é"), bad(Problem::BadCharacter, 0, 2));
}

#[test]
fn text_and_numbers_that_go_wrong() {
    check!("open", tokens("x 'oops"), bad(Problem::UnclosedText, 2, 7));
    check!("huge", tokens("9223372036854775808"), bad(Problem::NumberTooLarge, 0, 19));
}
