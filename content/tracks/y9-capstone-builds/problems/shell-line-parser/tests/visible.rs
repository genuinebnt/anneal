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
fn redirects_are_collected() {
    let c = parse("sort < in > out", &env()).unwrap();
    check!("one command", c.len(), 1);
    check!("args", c[0].args.clone(), v(&["sort"]));
    check!("stdin", c[0].stdin.clone(), Some("in".to_string()));
    check!("stdout", c[0].stdout.clone(), Some(Redirect { path: "out".to_string(), append: false }));
}

#[test]
fn one_command() {
    check!(r#"plain words"#, argv("ls -l /tmp"), Ok(vec![v(&["ls", "-l", "/tmp"])]));
}

#[test]
fn a_pipeline() {
    check!(r#"three stages"#, argv("cat f | grep x | wc -l"), Ok(vec![v(&["cat", "f"]), v(&["grep", "x"]), v(&["wc", "-l"])]));
}

#[test]
fn pipe_needs_no_spaces() {
    check!(r#"a|b"#, argv("a|b"), Ok(vec![v(&["a"]), v(&["b"])]));
}

#[test]
fn quotes_group_words() {
    check!(r#"double and single"#, argv("echo \"a b\" 'c  d'"), Ok(vec![v(&["echo", "a b", "c  d"])]));
}

#[test]
fn variables_expand_in_double_quotes() {
    check!(r#"but not in single quotes"#, argv("echo \"$HOME/bin\" '$HOME'"), Ok(vec![v(&["echo", "/home/u/bin", "$HOME"])]));
}

#[test]
fn empty_line() {
    check!(r#"no commands"#, (argv(""), argv("   ")), (Ok(vec![]), Ok(vec![])));
}

#[test]
fn unterminated_quote() {
    check!(r#"an error, not a panic"#, argv("echo \"abc"), Err(ParseError::UnterminatedQuote));
}
