use anneal_prelude::Rng;
use solution::Token as T;
use solution::*;

fn tokens(sql: &str) -> Result<Vec<T>, SyntaxError> {
    Ok(lex(sql)?.into_iter().map(|t| t.0).collect())
}

fn mistake(sql: &str) -> Option<(Problem, usize, usize)> {
    lex(sql).err().map(|e| (e.problem, e.start, e.end))
}

#[test]
fn nothing_but_space() {
    check!("empty", lex(""), Ok(vec![]));
    check!("space", lex(" \t\r\n\u{c}"), Ok(vec![]));
}

#[test]
fn tokens_need_no_space() {
    check!("1a", tokens("1a"), Ok(vec![T::Number(1), T::Word("a".into())]));
    check!("-1", tokens("-1"), Ok(vec![T::Symbol("-"), T::Number(1)]));
    check!("a'b'c", tokens("a'b'c"), Ok(vec![T::Word("a".into()), T::Text("b".into()), T::Word("c".into())]));
}

#[test]
fn texts_keep_everything() {
    let sql = "'X ''y'' -- ( é\n'";
    check!(sql, lex(sql), Ok(vec![(T::Text("X 'y' -- ( é\n".into()), 0, sql.len())]));
    check!("one quote inside", tokens("''''"), Ok(vec![T::Text("'".into())]));
}

#[test]
fn the_largest_number() {
    check!("MAX", tokens("9223372036854775807"), Ok(vec![T::Number(i64::MAX)]));
    check!("leading zeros", tokens("007"), Ok(vec![T::Number(7)]));
    check!("far too large", mistake("a 99999999999999999999 b"), Some((Problem::NumberTooLarge, 2, 22)));
}

#[test]
fn every_way_to_leave_a_text_open() {
    check!("a lone quote", mistake("SELECT '"), Some((Problem::UnclosedText, 7, 8)));
    check!("two quotes then open", mistake("SELECT '''"), Some((Problem::UnclosedText, 7, 10)));
}

#[test]
fn bad_characters() {
    for (sql, start, end) in [("a ! b", 2, 3), ("é", 0, 2), ("x \u{3000}", 2, 5), ("\"a\"", 0, 1), ("a.b", 1, 2), ("1 % 2", 2, 3), ("!", 0, 1)] {
        check!(sql, mistake(sql), Some((Problem::BadCharacter, start, end)));
    }
}

#[test]
fn the_first_mistake_from_the_left() {
    check!("character first", mistake("? 'open"), Some((Problem::BadCharacter, 0, 1)));
    check!("number first", mistake("99999999999999999999 ?"), Some((Problem::NumberTooLarge, 0, 20)));
    check!("an open text takes the rest", mistake("'a ?"), Some((Problem::UnclosedText, 0, 4)));
}

#[test]
fn random_tokens_read_back() {
    let mut rng = Rng::new(18241);
    for _ in 0..800 {
        let mut sql = String::new();
        let mut want = Vec::new();
        for _ in 0..rng.below(12) {
            let (written, token) = match rng.below(4) {
                0 => {
                    let w = *rng.pick(&["select", "a", "_b9", "Count", "NULL"]);
                    (w.to_string(), T::Word(w.to_ascii_lowercase()))
                }
                1 => {
                    let n = rng.int(0, 1000);
                    (n.to_string(), T::Number(n))
                }
                2 => {
                    let len = rng.below(5);
                    let text = rng.string(len, "aB' é,<");
                    (format!("'{}'", text.replace('\'', "''")), T::Text(text))
                }
                _ => {
                    let s = *rng.pick(&["<=", ">=", "<>", "!=", "(", ")", ",", "*", "+", "-", "/", "=", "<", ">", ";"]);
                    (s.to_string(), T::Symbol(if s == "!=" { "<>" } else { s }))
                }
            };
            // only a bracket or a comma may touch the token before it without any risk of running into it
            let may_touch = matches!(token, T::Symbol("(" | ")" | ","));
            if !may_touch || rng.bool() {
                sql.push_str(rng.pick(&[" ", "  ", "\n", "\t"]));
            }
            let start = sql.len();
            sql.push_str(&written);
            want.push((token, start, sql.len()));
        }
        check!(format!("{sql:?}"), lex(&sql), Ok(want));
    }
}

#[test]
fn any_text_at_all() {
    let mut rng = Rng::new(18242);
    for _ in 0..5000 {
        let len = rng.below(12);
        let sql = rng.string(len, "a1 '<>=!()é#_\n");
        match lex(&sql) {
            Ok(all) => {
                let ordered = all.windows(2).all(|w| w[0].2 <= w[1].1);
                let sliceable = all.iter().all(|t| sql.get(t.1..t.2).is_some_and(|s| !s.is_empty()));
                check!(format!("{sql:?}: ranges in order"), ordered && sliceable, true);
            }
            Err(e) => check!(format!("{sql:?}: inside the text"), sql.get(e.start..e.end).is_some(), true),
        }
    }
}

#[test]
fn long_input() {
    let sql = format!("'{}'", "x''".repeat(200_000));
    check!("a text with 200 000 quotes", tokens(&sql).map(|t| t.len()), Ok(1));
    let sql = "(a1,".repeat(100_000);
    check!("300 000 tokens", lex(&sql).map(|t| t.len()), Ok(300_000));
}
