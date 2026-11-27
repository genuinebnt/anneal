use solution::*;

#[allow(dead_code)]
fn resolved(src: &str) -> Result<String, Vec<String>> {
    front_end(src).map(|(p, _)| dump(&p))
}

#[allow(dead_code)]
fn ok(s: &str) -> Result<String, Vec<String>> {
    Ok(s.to_string())
}

#[allow(dead_code)]
fn errors(src: &str) -> Vec<String> {
    front_end(src).err().unwrap_or_default()
}

#[test]
fn distances_are_counted_in_scopes() {
    let src = "var g = 1; { var a = g; { var b = a; b = a + g; } }";
    check!(
        src,
        resolved(src),
        ok("(var g 1)\n(block (var a@0 g) (block (var b@0 a@1) (expr (= b@0 (+ a@1 g)))))")
    );
}

#[test]
fn a_name_means_what_was_declared_above_it() {
    let src = "{ fun show() { print(a); } var a = 1; show(); print(a); }";
    check!(
        src,
        resolved(src),
        ok("(block (fun show@0 () (expr (call print a))) (var a@0 1) (expr (call show@0)) (expr (call print a@0)))")
    );
}

#[test]
fn parameters_and_the_body_share_a_scope() {
    let src = "fun f(x) { var y = x; return fun_of(f, y); } { fun g() { g(); } }";
    check!(
        src,
        resolved(src),
        ok("(fun f (x@0) (var y@0 x@0) (return (call fun_of f y@0)))\n(block (fun g@0 () (expr (call g@1))))")
    );
}

#[test]
fn rules_about_where_a_word_may_stand() {
    check!(
        "return 1;",
        errors("return 1;"),
        vec!["[line 1:1] Error at 'return': Can't return from top-level code.".to_string()]
    );
    check!(
        "{ var a = a; }",
        errors("{ var a = a; }"),
        vec![
            "[line 1:11] Error at 'a': Can't read local variable in its own initializer."
                .to_string()
        ]
    );
    check!(
        "break;",
        errors("break;"),
        vec!["[line 1:1] Error at 'break': Can't use 'break' outside of a loop.".to_string()]
    );
}

#[test]
fn an_unused_local_is_a_warning() {
    let src = "{ var unused = 1; var used = 2; print(used); }";
    check!(
        src,
        front_end(src).map(|(_, w)| w),
        Ok(vec![
            "[line 1:7] Warning at 'unused': Local variable is never used.".to_string()
        ])
    );
}
