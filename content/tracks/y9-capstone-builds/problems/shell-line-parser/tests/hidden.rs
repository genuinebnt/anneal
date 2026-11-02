use solution::*;

use std::collections::HashMap;

fn env() -> HashMap<String, String> {
    HashMap::from([
        ("HOME".to_string(), "/home/u".to_string()),
        ("NAME".to_string(), "world".to_string()),
        ("EMPTY".to_string(), String::new()),
        ("A".to_string(), "1".to_string()),
    ])
}

fn argv(line: &str) -> Result<Vec<Vec<String>>, ParseError> {
    parse(line, &env()).map(|cs| cs.into_iter().map(|c| c.args).collect())
}

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn redirects_in_every_position() {
    let c = parse("a>f b", &env()).unwrap();
    check!("unspaced redirect, then another arg", (c[0].args.clone(), c[0].stdout.clone()), (v(&["a", "b"]), Some(Redirect { path: "f".to_string(), append: false })));
    let c = parse("a>>f", &env()).unwrap();
    check!("append", c[0].stdout.clone(), Some(Redirect { path: "f".to_string(), append: true }));
    let c = parse("a<f", &env()).unwrap();
    check!("stdin unspaced", c[0].stdin.clone(), Some("f".to_string()));
    let c = parse("a > x > y", &env()).unwrap();
    check!("the last one wins", c[0].stdout.clone(), Some(Redirect { path: "y".to_string(), append: false }));
    let c = parse("a > \"my file\"", &env()).unwrap();
    check!("quoted target", c[0].stdout.clone(), Some(Redirect { path: "my file".to_string(), append: false }));
    let c = parse("> f a", &env()).unwrap();
    check!("redirect first", (c[0].args.clone(), c[0].stdout.clone()), (v(&["a"]), Some(Redirect { path: "f".to_string(), append: false })));
}

#[test]
fn each_stage_has_its_own_redirects() {
    let c = parse("a < in | b | c >> out", &env()).unwrap();
    check!("three stages", c.len(), 3);
    check!("first reads", (c[0].stdin.clone(), c[0].stdout.clone()), (Some("in".to_string()), None));
    check!("middle has none", (c[1].stdin.clone(), c[1].stdout.clone()), (None, None));
    check!("last appends", (c[2].stdin.clone(), c[2].stdout.clone()), (None, Some(Redirect { path: "out".to_string(), append: true })));
}

#[test]
fn single_quotes_are_literal() {
    check!(r#"no expansion, no escapes"#, argv("echo '$HOME \"x\" \\n'"), Ok(vec![v(&["echo", "$HOME \"x\" \\n"])]));
}

#[test]
fn braces_and_names() {
    check!(r#"${NAME}s and longer names"#, argv("echo \"${NAME}s $NAME_x $A$A\""), Ok(vec![v(&["echo", "worlds  11"])]));
}

#[test]
fn lone_dollar() {
    check!(r#"stays a dollar"#, argv("echo \"$ x\" a$"), Ok(vec![v(&["echo", "$ x", "a$"])]));
}

#[test]
fn empty_expansions() {
    check!(r#"unquoted vanishes, quoted stays"#, (argv("echo $EMPTY x"), argv("echo \"$EMPTY\" x"), argv("echo $NOPE x")), (Ok(vec![v(&["echo", "x"])]), Ok(vec![v(&["echo", "", "x"])]), Ok(vec![v(&["echo", "x"])])));
}

#[test]
fn backslash_escapes() {
    check!(r#"outside and inside quotes"#, argv("echo a\\ b \\| \\> \"q\\\"r\" \"s\\nt\""), Ok(vec![v(&["echo", "a b", "|", ">", "q\"r", "s\\nt"])]));
}

#[test]
fn operators_inside_quotes_are_text() {
    check!(r#"quoted pipe and redirect"#, argv("echo \"a|b\" 'c>d'"), Ok(vec![v(&["echo", "a|b", "c>d"])]));
}

#[test]
fn adjacent_pieces_make_one_word() {
    check!(r#"mixed quoting"#, argv("echo a\"b c\"'d'e"), Ok(vec![v(&["echo", "ab cde"])]));
}

#[test]
fn empty_arguments_are_arguments() {
    check!(r#"two empty strings"#, argv("echo \"\" ''"), Ok(vec![v(&["echo", "", ""])]));
}

#[test]
fn comments() {
    check!(r#"only at the start of a word"#, (argv("echo hi # there"), argv("echo a#b"), argv("# only"), argv("echo \"#x\"")), (Ok(vec![v(&["echo", "hi"])]), Ok(vec![v(&["echo", "a#b"])]), Ok(vec![]), Ok(vec![v(&["echo", "#x"])])));
}

#[test]
fn empty_command_errors() {
    check!(r#"bad pipelines"#, (argv("| a"), argv("a |"), argv("a | | b"), argv("> f")), (Err(ParseError::EmptyCommand), Err(ParseError::EmptyCommand), Err(ParseError::EmptyCommand), Err(ParseError::EmptyCommand)));
}

#[test]
fn redirect_target_errors() {
    check!(r#"nothing to open"#, (argv("a >"), argv("a > | b"), argv("a > > f"), argv("a < > f"), argv("a <")), (Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget)));
}

#[test]
fn quote_and_brace_errors() {
    check!(r#"unterminated things"#, (argv("echo 'abc"), argv("echo \"abc"), argv("echo ${HOME"), argv("echo \"${HOME\"")), (Err(ParseError::UnterminatedQuote), Err(ParseError::UnterminatedQuote), Err(ParseError::BadVariable), Err(ParseError::BadVariable)));
}
