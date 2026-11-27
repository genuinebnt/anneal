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

#[test]
fn operators_bind_as_in_the_table() {
    let src = "1 + 2 * 3 - -4 == 5 or !a and b;";
    check!(
        src,
        tree(src),
        ok(r#"(expr (or (== (- (+ 1 (* 2 3)) (- 4)) 5) (and (! a) b)))"#)
    );
}

#[test]
fn assignment_calls_and_properties() {
    let src = "a = b.c = f(1, \"x\")(2).d;";
    check!(
        src,
        tree(src),
        ok(r#"(expr (= a (.= b c (. (call (call f 1 "x") 2) d))))"#)
    );
}

#[test]
fn statements() {
    let src = "var x; if (x) { print(x); } else x = 1;\nwhile (x) break;";
    check!(
        src,
        tree(src),
        ok("(var x)\n(if x (block (expr (call print x))) (expr (= x 1)))\n(while x (break))")
    );
}

#[test]
fn a_for_loop_is_rewritten() {
    let src = "for (var i = 0; i < 3; i = i + 1) continue;";
    check!(
        src,
        tree(src),
        ok("(block (var i 0) (while (< i 3) (continue) (step (= i (+ i 1)))))")
    );
    check!(
        "for (;;) {}",
        tree("for (;;) {}"),
        ok("(while true (block))")
    );
}

#[test]
fn functions_and_classes() {
    let src = "fun add(a, b) { return a + b; }\nclass B < A { init(x) { this.x = x; } m() { return super.m(); } }";
    check!(
        src,
        tree(src),
        ok("(fun add (a b) (return (+ a b)))\n(class B (< A) (init-method init (x) (expr (.= this x x))) (method m () (return (call (super super m)))))")
    );
}

#[test]
fn one_run_reports_every_mistake() {
    let src = "var = 1;\nprint(;\nfun (){}\nprint(2);";
    check!(
        src,
        tree(src),
        bad(&[
            "[line 1:5] Error at '=': Expect variable name.",
            "[line 2:7] Error at ';': Expect expression.",
            "[line 3:5] Error at '(': Expect function name.",
        ])
    );
}
