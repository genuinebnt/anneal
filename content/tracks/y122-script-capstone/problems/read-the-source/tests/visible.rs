use solution::*;

#[allow(dead_code)]
fn kinds(src: &str) -> Vec<Kind> {
    scan(src).0.into_iter().map(|t| t.kind).collect()
}

#[allow(dead_code)]
fn errors(src: &str) -> Vec<String> {
    scan(src).1.iter().map(|d| d.short(src)).collect()
}

#[allow(dead_code)]
fn id(s: &str) -> Kind {
    Kind::Ident(s.into())
}

#[test]
fn a_small_statement() {
    use Kind::*;
    check!(
        "var x = 1.5;",
        kinds("var x = 1.5;"),
        vec![Var, id("x"), Eq, Num(1.5), Semi, Eof]
    );
}

#[test]
fn two_character_operators_and_keywords() {
    use Kind::*;
    let src = "!= == <= >= ! = < > and android _or";
    check!(
        src,
        kinds(src),
        vec![
            BangEq,
            EqEq,
            Le,
            Ge,
            Bang,
            Eq,
            Lt,
            Gt,
            And,
            id("android"),
            id("_or"),
            Eof
        ]
    );
}

#[test]
fn a_string_with_escapes() {
    let src = r#""a\n\t\"b\" \\ c""#;
    check!(
        src,
        kinds(src),
        vec![Kind::Str("a\n\t\"b\" \\ c".into()), Kind::Eof]
    );
}

#[test]
fn comments_nest() {
    let src = "1 /* a /* nested */ still a comment */ 2 // to the end\n3";
    check!(
        src,
        kinds(src),
        vec![Kind::Num(1.0), Kind::Num(2.0), Kind::Num(3.0), Kind::Eof]
    );
}

#[test]
fn a_stray_character_is_reported_and_skipped() {
    let src = "var $x;\n  @";
    check!(
        src,
        errors(src),
        vec![
            "[line 1:5] Error: Unexpected character '$'.".to_string(),
            "[line 2:3] Error: Unexpected character '@'.".to_string()
        ]
    );
    check!(
        src,
        kinds(src),
        vec![Kind::Var, id("x"), Kind::Semi, Kind::Eof]
    );
}

#[test]
fn a_diagnostic_with_carets() {
    let src = "var a = 1;\nprint(abc);\n";
    let span = Span {
        start: 17,
        end: 20,
        line: 2,
    };
    let diag = Diag::error(span, At::Token, "Something is wrong.");
    check!(
        "span of `abc`",
        diag.render(src),
        "[line 2:7] Error at 'abc': Something is wrong.\n   2 | print(abc);\n     |       ^^^"
            .to_string()
    );
}
