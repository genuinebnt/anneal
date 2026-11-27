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
fn every_single_character_token_and_keyword() {
    use Kind::*;
    let src = "(){},.-+; / * % break class continue else false for fun if nil or return super this true var while";
    let want = vec![
        LParen, RParen, LBrace, RBrace, Comma, Dot, Minus, Plus, Semi, Slash, Star, Percent, Break,
        Class, Continue, Else, False, For, Fun, If, Nil, Or, Return, Super, This, True, Var, While,
        Eof,
    ];
    check!(src, kinds(src), want);
}

#[test]
fn a_dot_belongs_to_a_number_only_between_digits() {
    use Kind::*;
    check!("1.", kinds("1."), vec![Num(1.0), Dot, Eof]);
    check!(".5", kinds(".5"), vec![Dot, Num(5.0), Eof]);
    check!("1.2.3", kinds("1.2.3"), vec![Num(1.2), Dot, Num(3.0), Eof]);
    check!("7abc", kinds("7abc"), vec![Num(7.0), id("abc"), Eof]);
}

#[test]
fn an_empty_source_is_only_the_end() {
    let (tokens, diags) = scan("");
    check!("empty", (tokens.len(), diags.len()), (1, 0));
    check!(
        "empty",
        tokens[0].span,
        Span {
            start: 0,
            end: 0,
            line: 1
        }
    );
}

#[test]
fn spans_are_byte_offsets_and_lines_count_everywhere() {
    let src = "\"é\n\" /* ü\n\n */ x // y\n z";
    let (tokens, diags) = scan(src);
    check!(src, diags.len(), 0);
    let got: Vec<(&str, u32)> = tokens
        .iter()
        .map(|t| (&src[t.span.start..t.span.end], t.span.line))
        .collect();
    check!(src, got, vec![("\"é\n\"", 1), ("x", 4), ("z", 5), ("", 5)]);
}

#[test]
fn columns_count_characters_not_bytes() {
    check!(
        "\"éé\" $",
        errors("\"éé\" $"),
        vec!["[line 1:6] Error: Unexpected character '$'.".to_string()]
    );
    check!("line_col", line_col("aé\néé b", 9), (2, 4));
    check!("line_col at the end", line_col("ab\n", 3), (2, 1));
}

#[test]
fn a_non_ascii_stray_character_is_taken_whole() {
    let src = "1 € 2 λ";
    check!(
        src,
        errors(src),
        vec![
            "[line 1:3] Error: Unexpected character '€'.".to_string(),
            "[line 1:7] Error: Unexpected character 'λ'.".to_string()
        ]
    );
    check!(
        src,
        kinds(src),
        vec![Kind::Num(1.0), Kind::Num(2.0), Kind::Eof]
    );
}

#[test]
fn unterminated_text_is_reported_where_it_starts() {
    check!(
        "print(\"abc);\n1",
        errors("print(\"abc);\n1"),
        vec!["[line 1:7] Error: Unterminated string.".to_string()]
    );
    check!(
        "x\n /* /* */",
        errors("x\n /* /* */"),
        vec!["[line 2:2] Error: Unterminated block comment.".to_string()]
    );
    check!(
        "\"a\\",
        errors("\"a\\"),
        vec!["[line 1:1] Error: Unterminated string.".to_string()]
    );
    check!(
        "/*/",
        errors("/*/"),
        vec!["[line 1:1] Error: Unterminated block comment.".to_string()]
    );
}

#[test]
fn a_bad_escape_is_reported_and_the_string_goes_on() {
    let src = r#""a\qb" 1"#;
    check!(
        src,
        errors(src),
        vec!["[line 1:3] Error: Invalid escape sequence.".to_string()]
    );
    check!(
        src,
        kinds(src),
        vec![Kind::Str("ab".into()), Kind::Num(1.0), Kind::Eof]
    );
}

#[test]
fn a_bad_escape_before_any_character_can_be_drawn() {
    for (src, carets) in [
        ("print(\"a\\é\");", "^^"),
        ("\"\\€€\"", "^^"),
        ("\"\\\u{1F600}\"", "^^"),
    ] {
        let (_, diags) = scan(src);
        check!(src, diags.len(), 1);
        let drawn = diags[0].render(src);
        check!(src, drawn.rsplit(' ').next(), Some(carets));
    }
}

#[test]
fn several_mistakes_are_all_found() {
    let src = "# 1 \"\\z\" ? 2";
    check!(
        src,
        errors(src),
        vec![
            "[line 1:1] Error: Unexpected character '#'.".to_string(),
            "[line 1:6] Error: Invalid escape sequence.".to_string(),
            "[line 1:10] Error: Unexpected character '?'.".to_string()
        ]
    );
}

#[test]
fn drawing_handles_line_ends_and_the_end_of_input() {
    let src = "a = 1;\r\nb\r\n";
    let at_b = Diag::error(
        Span {
            start: 8,
            end: 9,
            line: 2,
        },
        At::Token,
        "No.",
    );
    check!(
        src,
        at_b.render(src),
        "[line 2:1] Error at 'b': No.\n   2 | b\n     | ^".to_string()
    );
    let end = Diag::error(
        Span {
            start: 5,
            end: 5,
            line: 1,
        },
        At::End,
        "Expect more.",
    );
    check!(
        "end of `a = 1`",
        end.render("a = 1"),
        "[line 1:6] Error at end: Expect more.\n   1 | a = 1\n     |      ^".to_string()
    );
    let open = Diag::error(
        Span {
            start: 2,
            end: 9,
            line: 1,
        },
        At::Nowhere,
        "Long.",
    );
    check!(
        "a span over two lines",
        open.render("x \"ab\ncd\" y"),
        "[line 1:3] Error: Long.\n   1 | x \"ab\n     |   ^^^".to_string()
    );
}

#[test]
fn a_warning_says_so() {
    let mut d = Diag::error(
        Span {
            start: 4,
            end: 5,
            line: 1,
        },
        At::Token,
        "Unused.",
    );
    d.warn = true;
    check!(
        "warning",
        d.short("var q;"),
        "[line 1:5] Warning at 'q': Unused.".to_string()
    );
}

#[test]
fn random_sources_give_back_their_pieces() {
    let pieces: &[&str] = &[
        "(",
        ")",
        "{",
        "}",
        ",",
        ".",
        "-",
        "+",
        ";",
        "*",
        "%",
        "!=",
        "==",
        "<=",
        ">=",
        "x",
        "_y2",
        "while",
        "whiles",
        "12",
        "3.25",
        "\"s\"",
        "\"é \\n\"",
        "super",
    ];
    let gaps: &[&str] = &[
        " ",
        "  ",
        "\n",
        "\t",
        "\r\n",
        " // c\n",
        " /* x\n /* y */ */ ",
    ];
    let mut rng = anneal_prelude::Rng::new(5101);
    for _ in 0..400 {
        let mut src = String::new();
        let mut want = Vec::new();
        for _ in 0..rng.below(30) {
            src.push_str(rng.pick(gaps));
            let piece = *rng.pick(pieces);
            let line = 1 + src.matches('\n').count() as u32;
            want.push((piece.to_string(), src.len(), line));
            src.push_str(piece);
        }
        let (tokens, diags) = scan(&src);
        check!(src.clone(), diags.len(), 0);
        check!(
            src.clone(),
            tokens.last().map(|t| t.kind.clone()),
            Some(Kind::Eof)
        );
        let got: Vec<(String, usize, u32)> = tokens[..tokens.len() - 1]
            .iter()
            .map(|t| {
                (
                    src[t.span.start..t.span.end].to_string(),
                    t.span.start,
                    t.span.line,
                )
            })
            .collect();
        check!(src.clone(), got, want);
    }
}

#[test]
fn any_text_at_all_is_survived() {
    let alphabet: &[char] = &[
        'a', '1', '.', '"', '\\', '/', '*', '\n', 'é', '€', ' ', '=', '!', '$', '\r',
    ];
    let mut rng = anneal_prelude::Rng::new(5102);
    for _ in 0..3000 {
        let src: String = (0..rng.below(14)).map(|_| *rng.pick(alphabet)).collect();
        let (tokens, diags) = scan(&src);
        check!(
            format!("{src:?}"),
            tokens.last().map(|t| t.span.end),
            Some(src.len())
        );
        for d in diags {
            check!(format!("{src:?}"), d.render(&src).lines().count(), 3);
        }
    }
}

#[test]
fn a_large_source_is_fine() {
    let src = "var abc = 12.5; /* é */\n".repeat(5000);
    check!("5000 lines", scan(&src).0.len(), 25001);
}
