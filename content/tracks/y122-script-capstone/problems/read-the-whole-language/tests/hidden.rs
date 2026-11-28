use solution::*;

#[allow(dead_code)]
fn tree(src: &str) -> Result<String, Vec<String>> {
    parse(src).map(|p| dump(&p))
}

#[allow(dead_code)]
fn ok(s: &str) -> Result<String, Vec<String>> {
    Ok(s.to_string())
}

#[allow(dead_code)]
fn bad(list: &[&str]) -> Result<String, Vec<String>> {
    Err(list.iter().map(|s| s.to_string()).collect())
}

/// Deep input is parsed on a thread with plenty of room.
#[allow(dead_code)]
fn on_a_roomy_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let thread = std::thread::Builder::new().stack_size(64 << 20);
    thread
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("the parser panicked")
}

#[test]
fn each_message_where_it_belongs() {
    let cases = [
        (
            "print(1)",
            "[line 1:9] Error at end: Expect ';' after expression.",
        ),
        (
            "var a = 1",
            "[line 1:10] Error at end: Expect ';' after variable declaration.",
        ),
        (
            "(1;",
            "[line 1:3] Error at ';': Expect ')' after expression.",
        ),
        (
            "f(1;",
            "[line 1:4] Error at ';': Expect ')' after arguments.",
        ),
        (
            "a.1;",
            "[line 1:3] Error at '1': Expect property name after '.'.",
        ),
        (
            "if x) y;",
            "[line 1:4] Error at 'x': Expect '(' after 'if'.",
        ),
        (
            "if (x y;",
            "[line 1:7] Error at 'y': Expect ')' after if condition.",
        ),
        (
            "while x) y;",
            "[line 1:7] Error at 'x': Expect '(' after 'while'.",
        ),
        (
            "while (x y;",
            "[line 1:10] Error at 'y': Expect ')' after condition.",
        ),
        (
            "for x;;) y;",
            "[line 1:5] Error at 'x': Expect '(' after 'for'.",
        ),
        (
            "for (x) y;",
            "[line 1:7] Error at ')': Expect ';' after loop initializer.",
        ),
        (
            "for (; x) y;",
            "[line 1:9] Error at ')': Expect ';' after loop condition.",
        ),
        (
            "for (;; x y;",
            "[line 1:11] Error at 'y': Expect ')' after for clauses.",
        ),
        ("{ x;", "[line 1:5] Error at end: Expect '}' after block."),
        (
            "break",
            "[line 1:6] Error at end: Expect ';' after 'break'.",
        ),
        (
            "continue",
            "[line 1:9] Error at end: Expect ';' after 'continue'.",
        ),
        (
            "return 1",
            "[line 1:9] Error at end: Expect ';' after return value.",
        ),
        (
            "fun f x) {}",
            "[line 1:7] Error at 'x': Expect '(' after function name.",
        ),
        (
            "fun f(1) {}",
            "[line 1:7] Error at '1': Expect parameter name.",
        ),
        (
            "fun f(a b) {}",
            "[line 1:9] Error at 'b': Expect ')' after parameters.",
        ),
        (
            "fun f() x;",
            "[line 1:9] Error at 'x': Expect '{' before function body.",
        ),
        ("class 1 {}", "[line 1:7] Error at '1': Expect class name."),
        (
            "class A < {}",
            "[line 1:11] Error at '{': Expect superclass name.",
        ),
        (
            "class A x",
            "[line 1:9] Error at 'x': Expect '{' before class body.",
        ),
        (
            "class A { 1 }",
            "[line 1:11] Error at '1': Expect method name.",
        ),
        (
            "class A { m x }",
            "[line 1:13] Error at 'x': Expect '(' after method name.",
        ),
        (
            "super x;",
            "[line 1:7] Error at 'x': Expect '.' after 'super'.",
        ),
        (
            "super.1;",
            "[line 1:7] Error at '1': Expect superclass method name.",
        ),
        (
            "if (a) var b;",
            "[line 1:8] Error at 'var': Expect expression.",
        ),
    ];
    for (src, first) in cases {
        let got = parse(src).map(|_| ()).map_err(|e| e[0].clone());
        check!(src, got, Err(first.to_string()));
    }
}

#[test]
fn all_operators_group_to_the_left() {
    let src =
        "a - b - c; a / b % c * d; a < b <= c > d >= e; a == b != c; a or b or c; a and b and c;";
    check!(
        src,
        tree(src),
        ok("(expr (- (- a b) c))\n(expr (* (% (/ a b) c) d))\n(expr (>= (> (<= (< a b) c) d) e))\n(expr (!= (== a b) c))\n(expr (or (or a b) c))\n(expr (and (and a b) c))")
    );
}

#[test]
fn literals_brackets_and_unary_chains() {
    let src = "print(nil, true, false, 2.50, \"a\\nb\", !-!x, -(1 + 2), -a.b(), this.a);";
    check!(
        src,
        tree(src),
        ok("(expr (call print nil true false 2.5 \"a\\nb\" (! (- (! x))) (- (+ 1 2)) (- (call (. a b))) (. this a)))")
    );
}

#[test]
fn else_belongs_to_the_nearest_if() {
    let src = "if (a) if (b) c; else d;";
    check!(src, tree(src), ok("(if a (if b (expr c) (expr d)))"));
}

#[test]
fn every_shape_of_a_for_loop() {
    check!(
        "for (i = 0; i;) {}",
        tree("for (i = 0; i;) {}"),
        ok("(block (expr (= i 0)) (while i (block)))")
    );
    check!(
        "for (; ; i) x;",
        tree("for (; ; i) x;"),
        ok("(while true (expr x) (step i))")
    );
    check!(
        "for (var i;;) x;",
        tree("for (var i;;) x;"),
        ok("(block (var i) (while true (expr x)))")
    );
}

#[test]
fn a_bad_target_is_reported_without_skipping_anything() {
    let src = "1 + 2 = 3 print; (a) = 1; f() = 2;";
    check!(
        src,
        tree(src),
        bad(&[
            "[line 1:7] Error at '=': Invalid assignment target.",
            "[line 1:11] Error at 'print': Expect ';' after expression.",
            "[line 1:31] Error at '=': Invalid assignment target.",
        ])
    );
}

#[test]
fn skipping_stops_before_a_word_that_starts_a_statement() {
    let src = "x y z var 1; x x if ) ; a b while 2 ( for ? ; return return ;";
    let got = parse(src).map(|_| ()).map_err(|e| {
        e.iter()
            .map(|m| m.split("Error").next().unwrap().to_string())
            .collect::<Vec<_>>()
    });
    let want: Vec<String> = [
        "[line 1:3] ",
        "[line 1:11] ",
        "[line 1:16] ",
        "[line 1:21] ",
        "[line 1:27] ",
        "[line 1:35] ",
        "[line 1:43] ",
        "[line 1:45] ",
        "[line 1:54] ",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    check!(src, got, Err(want));
}

#[test]
fn a_lone_closer_does_not_loop() {
    let got = parse(") } ) }").map(|_| ()).map_err(|e| e[0].clone());
    check!(
        ") } ) }",
        got,
        Err("[line 1:1] Error at ')': Expect expression.".to_string())
    );
}

#[test]
fn scanner_mistakes_are_merged_in_by_position() {
    let src = "var = 1; $\nvar x = ;";
    check!(
        src,
        tree(src),
        bad(&[
            "[line 1:5] Error at '=': Expect variable name.",
            "[line 1:10] Error: Unexpected character '$'.",
            "[line 2:9] Error at ';': Expect expression.",
        ])
    );
}

#[test]
fn two_hundred_and_fifty_five_is_the_most() {
    let names: Vec<String> = (0..255).map(|i| format!("p{i}")).collect();
    let fine = format!("fun f({}) {{}} f({});", names.join(", "), names.join(", "));
    check!("255 parameters and arguments", parse(&fine).is_ok(), true);
    let params = format!("fun f({}, q) {{}}", names.join(","));
    let got = parse(&params).map(|_| ()).map_err(|e| {
        e.len() == 1 && e[0].ends_with("Error at 'q': Can't have more than 255 parameters.")
    });
    check!("256 parameters", got, Err(true));
    let args = format!("f({}, q);", names.join(","));
    let got = parse(&args).map(|_| ()).map_err(|e| {
        e.len() == 1 && e[0].ends_with("Error at 'q': Can't have more than 255 arguments.")
    });
    check!("256 arguments", got, Err(true));
}

#[test]
fn only_the_first_hundred_mistakes_are_reported() {
    let src = "var = 1;\n".repeat(3000);
    let got = parse(&src)
        .map(|_| ())
        .map_err(|e| (e.len(), e[99].clone()));
    check!(
        "3000 mistakes",
        got,
        Err((
            100,
            "[line 100:5] Error at '=': Expect variable name.".to_string()
        ))
    );
}

#[allow(dead_code)]
fn forms(n: usize) -> Vec<(&'static str, String)> {
    vec![
        (
            "brackets",
            format!("x = {}1{};", "(".repeat(n), ")".repeat(n)),
        ),
        ("a long chain", format!("x = 1{};", " + 1".repeat(n))),
        ("a chain of or", format!("x = a{};", " or a".repeat(n))),
        ("prefix operators", format!("x = {}y;", "!".repeat(n))),
        ("assignments", format!("{}1;", "a = ".repeat(n))),
        ("calls", format!("f{};", "()".repeat(n))),
        ("properties", format!("a{};", ".b".repeat(n))),
        ("arguments", format!("{}{};", "f(".repeat(n), ")".repeat(n))),
        ("blocks", format!("{}{}", "{".repeat(n), "}".repeat(n))),
        (
            "functions",
            format!("{}{}", "fun f() {".repeat(n), "}".repeat(n)),
        ),
        ("ifs", format!("{}x;", "if (a) ".repeat(n))),
        ("else ifs", format!("{}x;", "if (a) b; else ".repeat(n))),
        ("loops", format!("{}x;", "while (a) ".repeat(n))),
        ("for loops", format!("{}x;", "for (;;) ".repeat(n))),
    ]
}

#[test]
fn forty_brackets_each_with_an_operator_around_a_call() {
    let src = format!(
        "fun f(n) {{ if (n == 0) return 0; return {}f(n - 1){}; }}",
        "(1 + ".repeat(40),
        ")".repeat(40)
    );
    let got = on_a_roomy_thread(move || parse(&src).map(|p| p.len()));
    check!("the shape in the statement", got, Ok(1));
}

#[test]
fn forty_levels_of_anything_are_accepted() {
    for (what, src) in forms(40) {
        let got = on_a_roomy_thread(move || parse(&src).map(|p| dump(&p).is_empty()));
        check!(format!("40 levels of {what}"), got, Ok(false));
    }
}

#[test]
fn a_hundred_thousand_levels_are_refused() {
    for (what, src) in forms(100_000) {
        let got = on_a_roomy_thread(move || {
            parse(&src)
                .map(|_| ())
                .map_err(|e| e[0].ends_with(": Too deeply nested."))
        });
        check!(format!("100 000 levels of {what}"), got, Err(true));
    }
}

#[test]
fn width_is_not_depth() {
    let src = format!(
        "{{ {} }}",
        "if (a) { b = (c) + -d; } f(1, 2);\n".repeat(600)
    );
    check!(
        "600 statements side by side",
        parse(&src).map(|p| p.len()),
        Ok(1)
    );
    let groups = format!("x = f({});", vec!["((1))"; 200].join(", "));
    check!("200 bracketed arguments", parse(&groups).is_ok(), true);
}

#[test]
fn the_depth_count_survives_mistakes() {
    let src = format!("{}{}", "x = ((((((((( ;\n".repeat(60), "x = 1;");
    let got = parse(&src)
        .map(|_| ())
        .map_err(|e| (e.len(), e.iter().all(|m| m.ends_with("Expect expression."))));
    check!("60 broken lines", got, Err((60, true)));
}

// Random trees, written as source with only the brackets they need.

enum E {
    Atom(&'static str),
    Bin(&'static str, Box<E>, Box<E>),
    Un(&'static str, Box<E>),
    Call(Box<E>, Vec<E>),
    Get(Box<E>),
    Assign(Box<E>),
}

fn power(op: &str) -> u8 {
    match op {
        "or" => 1,
        "and" => 2,
        "==" | "!=" => 3,
        "<" | "<=" | ">" | ">=" => 4,
        "+" | "-" => 5,
        _ => 6,
    }
}

fn level(e: &E) -> u8 {
    match e {
        E::Assign(_) => 0,
        E::Bin(op, ..) => power(op),
        E::Un(..) => 7,
        _ => 9,
    }
}

fn source(e: &E, at_least: u8) -> String {
    let text = match e {
        E::Atom(a) => a.to_string(),
        E::Bin(op, l, r) => format!("{} {op} {}", source(l, power(op)), source(r, power(op) + 1)),
        E::Un(op, x) => format!("{op}{}", source(x, 7)),
        E::Call(c, args) => {
            let args: Vec<String> = args.iter().map(|a| source(a, 0)).collect();
            format!("{}({})", source(c, 9), args.join(", "))
        }
        E::Get(o) => format!("{}.p", source(o, 9)),
        E::Assign(v) => format!("v = {}", source(v, 0)),
    };
    if level(e) < at_least {
        format!("({text})")
    } else {
        text
    }
}

fn expected(e: &E) -> String {
    match e {
        E::Atom(a) => a.to_string(),
        E::Bin(op, l, r) => format!("({op} {} {})", expected(l), expected(r)),
        E::Un(op, x) => format!("({op} {})", expected(x)),
        E::Call(c, args) => {
            let args: String = args.iter().map(|a| format!(" {}", expected(a))).collect();
            format!("(call {}{args})", expected(c))
        }
        E::Get(o) => format!("(. {} p)", expected(o)),
        E::Assign(v) => format!("(= v {})", expected(v)),
    }
}

fn random(rng: &mut anneal_prelude::Rng, depth: usize) -> E {
    if depth == 0 || rng.below(4) == 0 {
        return E::Atom(rng.pick(&["a", "b", "7", "nil", "true"]));
    }
    let d = depth - 1;
    match rng.below(9) {
        0 => E::Un(if rng.bool() { "-" } else { "!" }, Box::new(random(rng, d))),
        1 => {
            let callee = Box::new(random(rng, d));
            let count = rng.below(3);
            E::Call(callee, (0..count).map(|_| random(rng, d)).collect())
        }
        2 => E::Get(Box::new(random(rng, d))),
        3 => E::Assign(Box::new(random(rng, d))),
        _ => {
            let op = *rng.pick(&[
                "or", "and", "==", "!=", "<", "<=", ">", ">=", "+", "-", "*", "/", "%",
            ]);
            let left = Box::new(random(rng, d));
            E::Bin(op, left, Box::new(random(rng, d)))
        }
    }
}

#[test]
fn random_expressions_come_back_with_the_same_shape() {
    let mut rng = anneal_prelude::Rng::new(5201);
    for _ in 0..1500 {
        let e = random(&mut rng, 5);
        let src = format!("{};", source(&e, 0));
        check!(
            src.clone(),
            tree(&src),
            ok(&format!("(expr {})", expected(&e)))
        );
    }
}

#[test]
fn any_pile_of_tokens_ends_with_a_tree_or_with_mistakes() {
    let words = [
        "(", ")", "{", "}", ",", ".", ";", "=", "+", "-", "!", "<", "a", "1", "var", "fun",
        "class", "if", "else", "for", "while", "return", "break", "continue", "this", "super",
        "or", "\"s\"",
    ];
    let mut rng = anneal_prelude::Rng::new(5202);
    for _ in 0..4000 {
        let src: Vec<&str> = (0..rng.below(16)).map(|_| *rng.pick(&words)).collect();
        let src = src.join(" ");
        if let Err(list) = parse(&src) {
            check!(
                src.clone(),
                (!list.is_empty(), list.len() <= 100),
                (true, true)
            );
        }
    }
}

#[test]
fn skipping_stops_before_return() {
    // The broken `var` is skipped up to `return`, which starts a new statement.
    let src = "fun f() { var = 1 return 2; } print(f);";
    let (program, diags) = parse_tokens(scan(src).0);
    let messages: Vec<String> = diags.iter().map(|d| d.short(src)).collect();
    check!(
        src,
        (dump(&program), messages),
        (
            "(fun f () (return 2))\n(expr (call print f))".to_string(),
            vec!["[line 1:15] Error at '=': Expect variable name.".to_string()]
        )
    );
}

#[test]
fn more_than_256_is_still_one_mistake() {
    let names: Vec<String> = (0..300).map(|i| format!("p{i}")).collect();
    let params = format!("fun f({}) {{}}", names.join(","));
    let got = parse(&params).map(|_| ()).map_err(|e| e.len());
    check!("300 parameters", got, Err(1));
    let args = format!("f({});", names.join(","));
    let got = parse(&args).map(|_| ()).map_err(|e| e.len());
    check!("300 arguments", got, Err(1));
}
