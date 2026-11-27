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
fn warnings(src: &str) -> Result<Vec<String>, Vec<String>> {
    front_end(src).map(|(_, w)| w)
}

#[test]
fn each_rule_has_its_message_and_place() {
    let cases = [
        (
            "{ var a = a; }",
            "[line 1:11] Error at 'a': Can't read local variable in its own initializer.",
        ),
        (
            "{ var a = 1; { var a = a + 1; print(a); } print(a); }",
            "[line 1:24] Error at 'a': Can't read local variable in its own initializer.",
        ),
        (
            "{ var a = (a = 1); print(a); }",
            "[line 1:12] Error at 'a': Can't assign to local variable in its own initializer.",
        ),
        (
            "return;",
            "[line 1:1] Error at 'return': Can't return from top-level code.",
        ),
        (
            "print(this);",
            "[line 1:7] Error at 'this': Can't use 'this' outside of a class.",
        ),
        (
            "fun f() { this.x = 1; }",
            "[line 1:11] Error at 'this': Can't use 'this' outside of a class.",
        ),
        (
            "{ var a = 1; var a = 2; print(a); }",
            "[line 1:18] Error at 'a': Already a variable with this name in this scope.",
        ),
        (
            "fun f(a, a) {}",
            "[line 1:10] Error at 'a': Already a variable with this name in this scope.",
        ),
        (
            "fun f(a) { var a = 1; print(a); }",
            "[line 1:16] Error at 'a': Already a variable with this name in this scope.",
        ),
        (
            "{ fun f() {} fun f() {} f(); }",
            "[line 1:18] Error at 'f': Already a variable with this name in this scope.",
        ),
        (
            "{ class A {} var A = 1; print(A); }",
            "[line 1:18] Error at 'A': Already a variable with this name in this scope.",
        ),
        (
            "continue;",
            "[line 1:1] Error at 'continue': Can't use 'continue' outside of a loop.",
        ),
        (
            "fun f() { break; }\nwhile (true) { f(); break; }",
            "[line 1:11] Error at 'break': Can't use 'break' outside of a loop.",
        ),
        (
            "while (true) { fun f() { continue; } }",
            "[line 1:26] Error at 'continue': Can't use 'continue' outside of a loop.",
        ),
        (
            "for (;;) {} break;",
            "[line 1:13] Error at 'break': Can't use 'break' outside of a loop.",
        ),
        (
            "class A < A {}",
            "[line 1:11] Error at 'A': A class can't inherit from itself.",
        ),
        (
            "super.x();",
            "[line 1:1] Error at 'super': Can't use 'super' outside of a class.",
        ),
        (
            "class A { m() { return super.m(); } }",
            "[line 1:24] Error at 'super': Can't use 'super' in a class with no superclass.",
        ),
        (
            "class A < B {} class C { m() { super.m(); } }",
            "[line 1:32] Error at 'super': Can't use 'super' in a class with no superclass.",
        ),
        (
            "class A { init() { return 1; } }",
            "[line 1:20] Error at 'return': Can't return a value from an initializer.",
        ),
        (
            "class A { m() { fun init() { return 1; } return this; } n() { break; } }",
            "[line 1:63] Error at 'break': Can't use 'break' outside of a loop.",
        ),
        (
            "fun f() { class A { m() {} } return 1; } return 2;",
            "[line 1:42] Error at 'return': Can't return from top-level code.",
        ),
        (
            "class A { m() {} } print(this);",
            "[line 1:26] Error at 'this': Can't use 'this' outside of a class.",
        ),
    ];
    for (src, want) in cases {
        let got = front_end(src).map(|_| ()).map_err(|e| e.clone());
        check!(src, got, Err(vec![want.to_string()]));
    }
}

#[test]
fn what_is_allowed() {
    let fine = [
        "var a = a;",
        "var a = 1; var a = 2;",
        "x = 1; fun f(f) {} fun g() { var g = 2; return g; }",
        "while (true) { if (x) { break; } else { continue; } }",
        "for (;;) { { break; } }",
        "while (a) fun_call(); while (b) { fun f() { while (c) break; return; } continue; }",
        "class A { init() { return; } m() { return 1; } } class B < A { n() { fun h() { return super.m() + this.x; } return h; } }",
        "{ var a = 1; { var b = a; var a = b; print(a); } }",
        "{ class A { copy() { return A(); } } print(A); }",
        "fun f() { class Inner < f { m() { return this; } } return Inner; }",
    ];
    for src in fine {
        check!(src, front_end(src).map(|_| ()), Ok(()));
    }
}

#[test]
fn several_mistakes_in_one_run_come_in_order() {
    let src = "return 1;\n{ var a = 1; var a = 2; }\nbreak;\nprint(this);";
    let got = front_end(src).map(|_| ()).map_err(|e| {
        e.iter()
            .map(|m| m.split(']').next().unwrap().to_string())
            .collect::<Vec<_>>()
    });
    let want: Vec<String> = ["[line 1:1", "[line 2:18", "[line 3:1", "[line 4:7"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    check!(src, got, Err(want));
}

#[test]
fn errors_hide_warnings() {
    let src = "{ var unused = 1; } break;";
    check!(src, front_end(src).map(|_| ()).map_err(|e| e.len()), Err(1));
}

#[test]
fn syntax_mistakes_come_first_and_alone() {
    let src = "break; print( ;";
    check!(
        src,
        front_end(src).map(|_| ()).map_err(|e| e),
        Err(vec![
            "[line 1:15] Error at ';': Expect expression.".to_string()
        ])
    );
}

#[test]
fn only_variables_declared_with_var_can_be_unused() {
    let src =
        "var g = 1; fun f(p) { fun h() {} class K {} var w = 1; w = 2; w = w; var z; z = 3; }";
    check!(
        src,
        warnings(src),
        Ok(vec![
            "[line 1:74] Warning at 'z': Local variable is never used.".to_string()
        ])
    );
}

#[test]
fn warnings_come_in_source_order() {
    let names: Vec<String> = (0..40).map(|i| format!("var v{i:02} = 0;")).collect();
    let src = format!("{{\n{}\n}}", names.join("\n"));
    let got = warnings(&src).map(|w| {
        w.iter()
            .map(|m| m.split('\'').nth(1).unwrap().to_string())
            .collect::<Vec<_>>()
    });
    check!(
        "40 unused locals",
        got,
        Ok((0..40).map(|i| format!("v{i:02}")).collect::<Vec<_>>())
    );
}

#[test]
fn only_the_first_hundred_are_reported() {
    let src = "break;\n".repeat(500);
    check!(
        "500 breaks",
        front_end(&src)
            .map(|_| ())
            .map_err(|e| (e.len(), e[99].starts_with("[line 100:1]"))),
        Err((100, true))
    );
}

#[test]
fn this_and_super_live_in_their_own_scopes() {
    let src =
        "class B < A { m(x) { { this.m(x); super.m(x); } fun g() { return this; } return g; } }";
    check!(
        src,
        resolved(src),
        ok("(class B (< A) (method m (x@0) (block (expr (call (. this@2 m) x@1)) (expr (call (super super@3 m) x@1))) (fun g@0 () (return this@2)) (return g@0)))")
    );
    let src = "class A { m() { return this; } }";
    check!(
        src,
        resolved(src),
        ok("(class A (method m () (return this@1)))")
    );
}

#[test]
fn a_local_class_and_its_parent() {
    let src = "{ class A {} class B < A { m() { return A; } } print(B); }";
    check!(
        src,
        resolved(src),
        ok("(block (class A@0) (class B@0 (< A@0) (method m () (return A@3))) (expr (call print B@0)))")
    );
}

#[test]
fn the_step_of_a_loop_and_the_loop_variable() {
    let src = "for (var i = 0; i < 3; i = i + 1) { var j = i; print(j); }";
    check!(
        src,
        resolved(src),
        ok("(block (var i@0 0) (while (< i@0 3) (block (var j@0 i@1) (expr (call print j@0))) (step (= i@0 (+ i@0 1)))))")
    );
}

// Random nests of blocks, functions, declarations and reads. The text of the expected tree is built alongside.

#[allow(dead_code)]
struct Gen {
    rng: anneal_prelude::Rng,
    scopes: Vec<Vec<&'static str>>,
}

#[allow(dead_code)]
const NAMES: [&str; 5] = ["a", "b", "c", "d", "e"];

#[allow(dead_code)]
impl Gen {
    fn look(&self, name: &str) -> String {
        match self.scopes.iter().rev().position(|s| s.contains(&name)) {
            Some(d) => format!("{name}@{d}"),
            None => name.to_string(),
        }
    }

    /// One statement as (source, expected).
    fn statement(&mut self, depth: usize) -> (String, String) {
        let name = NAMES[self.rng.below(5)];
        let other = NAMES[self.rng.below(5)];
        let kind = self.rng.below(if depth == 0 { 3 } else { 5 });
        let taken = self.scopes.last().is_some_and(|s| s.contains(&name));
        let inside = !self.scopes.is_empty();
        match kind {
            0 if !taken => {
                // The new name may not appear in its own initializer.
                if inside && other == name {
                    self.declare(name);
                    let here = self.look(name);
                    return (format!("var {name};"), format!("(var {here})"));
                }
                let value = self.look(other);
                self.declare(name);
                let declared = if inside {
                    format!("{name}@0")
                } else {
                    name.to_string()
                };
                (
                    format!("var {name} = {other};"),
                    format!("(var {declared} {value})"),
                )
            }
            3 => {
                self.scopes.push(Vec::new());
                let (src, want) = self.body(depth - 1);
                self.scopes.pop();
                (
                    format!("{{ {src} }}"),
                    if want.is_empty() {
                        "(block)".into()
                    } else {
                        format!("(block {want})")
                    },
                )
            }
            4 if !taken => {
                self.declare(name);
                let declared = if inside {
                    format!("{name}@0")
                } else {
                    name.to_string()
                };
                self.scopes.push(vec![other]);
                let (src, want) = self.body(depth - 1);
                self.scopes.pop();
                let tail = if want.is_empty() {
                    String::new()
                } else {
                    format!(" {want}")
                };
                (
                    format!("fun {name}({other}) {{ {src} }}"),
                    format!("(fun {declared} ({other}@0){tail})"),
                )
            }
            1 => {
                let value = self.look(other);
                let target = self.look(name);
                (
                    format!("{name} = {other};"),
                    format!("(expr (= {target} {value}))"),
                )
            }
            _ => (format!("{other};"), format!("(expr {})", self.look(other))),
        }
    }

    fn declare(&mut self, name: &'static str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(name);
        }
    }

    fn body(&mut self, depth: usize) -> (String, String) {
        let count = self.rng.below(6);
        let (mut src, mut want) = (Vec::new(), Vec::new());
        // A parameter is already declared, so a body must not declare it again: `statement` checks that.
        for _ in 0..count {
            let (s, w) = self.statement(depth);
            src.push(s);
            want.push(w);
        }
        (src.join(" "), want.join(" "))
    }
}

#[test]
fn random_nests_of_scopes() {
    for seed in 0..1500 {
        let mut g = Gen {
            rng: anneal_prelude::Rng::new(5300 + seed),
            scopes: Vec::new(),
        };
        let count = 1 + g.rng.below(5);
        let (src, want): (Vec<String>, Vec<String>) = (0..count).map(|_| g.statement(4)).unzip();
        let src = src.join(" ");
        check!(src.clone(), resolved(&src), ok(&want.join("\n")));
    }
}
