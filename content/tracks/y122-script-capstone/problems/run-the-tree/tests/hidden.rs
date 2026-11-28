use solution::*;

/// Output and runtime error of a program that has no mistakes before it runs.
#[allow(dead_code)]
fn go(src: &str) -> (String, Option<String>) {
    go_fuel(src, 100_000_000)
}

#[allow(dead_code)]
fn go_fuel(src: &str, fuel: u64) -> (String, Option<String>) {
    let o = run(src, fuel);
    assert!(o.compile_errors.is_empty(), "{:?}", o.compile_errors);
    (o.out, o.error)
}

#[allow(dead_code)]
fn prints(out: &str) -> (String, Option<String>) {
    (out.to_string(), None)
}

#[allow(dead_code)]
fn fails(out: &str, error: &str) -> (String, Option<String>) {
    (out.to_string(), Some(error.to_string()))
}

/// The start of the runtime error, up to the length of `want`.
#[allow(dead_code)]
fn error_starts(src: &str, want: &str) -> String {
    let error = go_fuel(src, 30_000_000)
        .1
        .unwrap_or_else(|| "no error".into());
    error.chars().take(want.chars().count()).collect()
}

#[test]
fn programs() {
    // strings
    let src = r##"var s = "";
for (var i = 1; i <= 5; i = i + 1) { s = s + str(i * i); if (i < 5) s = s + ","; }
print(s); print(len(s));
var esc = "tab\there \"quoted\" \\ done";
print(esc); print(len(esc));"##;
    check!(
        src,
        go(src),
        prints(
            r##"1,4,9,16,25
11
tab	here "quoted" \ done
24
"##
        )
    );
    // nested loops with locals
    let src = r##"for (var i = 0; i < 3; i = i + 1) {
  var a = i * 10;
  var j = 0;
  while (true) {
    var b = a + j; j = j + 1;
    { var c = b; if (j == 2) continue; if (j > 3) break; var d = c; print(d); }
  }
  var e = a + 1; print(e);
}"##;
    check!(
        src,
        go(src),
        prints(
            r##"0
2
1
10
12
11
20
22
21
"##
        )
    );
    // natives
    let src = r##"print(type(nil)); print(type(true)); print(type(1)); print(type("s")); print(type(print)); print(type(list()));
print(num("12") + 1); print(num(" 2.5 ")); print(num("x")); print(num("inf")); print(num(true)); print(len("héllo")); print(str(1.5) + "!");
assert(1 < 2, "ok"); assert(0); assert(""); print("assert ok");
var l = list(); push(l, 1); push(l, "a"); push(l, list()); print(l); print(len(l)); print(pop(l)); print(pop(l)); print(get(l, 0)); set(l, 0, nil); print(l); pop(l); print(pop(l)); print(print);"##;
    check!(
        src,
        go(src),
        prints(
            r##"nil
bool
number
string
native
list
13
2.5
nil
nil
nil
5
1.5!
assert ok
[1, a, []]
3
[]
a
1
[nil]
nil
<native print>
"##
        )
    );
    // values
    let src = r##"print(1 == 1.0); print("a" == "a"); print(nil == nil); print(nil == false); print(1 == "1"); print(list() == list()); var l = list(); print(l == l); print(len != len); print(len == str);
print(0.1 + 0.2); print(10 / 4); print(7 % 3); print(-7 % 3); print(-0.5 * 4); print(100000000 * 100000000);
if (0) print("zero counts as true"); if ("") print("so does the empty string"); print(!nil); print(!0);
var o = 1; print((o = 10) - (o = 3)); print(o);"##;
    check!(
        src,
        go(src),
        prints(
            r##"true
true
true
false
false
false
true
false
false
0.30000000000000004
2.5
1
-1
-2
10000000000000000
zero counts as true
so does the empty string
true
false
7
3
"##
        )
    );
    // short circuit
    let src = r##"var calls = 0;
print(false and (calls = 1)); print(true or (calls = 2)); print(nil or "default"); print(1 and 2); print(nil and nothing); print(1 or nothing); print(calls);
print(true and (calls = 3)); print(false or nil); print(calls);"##;
    check!(
        src,
        go(src),
        prints(
            r##"false
true
default
2
nil
1
0
3
nil
3
"##
        )
    );
    // a list inside itself
    let src = r##"var l = list(); push(l, 1); push(l, l); print(l); set(l, 0, l); set(l, 1, 2); print(len(str(l)) > 0); set(l, 0, 0); print(l);"##;
    check!(
        src,
        go(src),
        prints(
            r##"[1, [1, [1, [1, [1, [1, [1, [1, [1, [...]]]]]]]]]]
true
[0, 2]
"##
        )
    );
}

#[test]
fn every_runtime_error() {
    // undefined variable
    let src = r##"print(1);
print(x);"##;
    check!(
        src,
        go(src),
        fails(
            r##"1
"##,
            r##"Undefined variable 'x'.
[line 2] in script"##
        )
    );
    // assign to undefined
    let src = r##"var y;
x = 1;"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Undefined variable 'x'.
[line 2] in script"##
        )
    );
    // the left operand comes first
    let src = r##"print(gone_a
 - gone_b);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Undefined variable 'gone_a'.
[line 1] in script"##
        )
    );
    // the value comes first
    let src = r##"x = nothing;"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Undefined variable 'nothing'.
[line 1] in script"##
        )
    );
    // add a string
    let src = r##"print(1
+
"a");"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Operands must be two numbers or two strings.
[line 2] in script"##
        )
    );
    // negate a string
    let src = r##"print(-"x");"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Operand must be a number.
[line 1] in script"##
        )
    );
    // compare
    let src = r##"print(1 < "a");"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Operands must be numbers.
[line 1] in script"##
        )
    );
    // multiply
    let src = r##""a" * 2;"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Operands must be numbers.
[line 1] in script"##
        )
    );
    // division by zero
    let src = r##"var z = 0;
print(10 / z);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Division by zero.
[line 2] in script"##
        )
    );
    // remainder by zero
    let src = r##"print(1 % 0);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Division by zero.
[line 1] in script"##
        )
    );
    // call a number
    let src = r##"var x = 3;
x(
);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Can only call functions and classes.
[line 3] in script"##
        )
    );
    // native arity
    let src = r##"len();"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Expected 1 arguments but got 0.
[line 1] in script"##
        )
    );
    // native arity range
    let src = r##"assert();"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Expected 1 to 2 arguments but got 0.
[line 1] in script"##
        )
    );
    // assert
    let src = r##"assert(1 == 2, "math");"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Assertion failed: math
[line 1] in script"##
        )
    );
    // assert without text
    let src = r##"assert(false);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Assertion failed.
[line 1] in script"##
        )
    );
    // index
    let src = r##"var l = list();
print(get(l, 0));"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Index out of range.
[line 2] in script"##
        )
    );
    // half an index
    let src = r##"var l = list(); push(l, 1); get(l, 0.5);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Index out of range.
[line 1] in script"##
        )
    );
    // negative index
    let src = r##"var l = list(); push(l, 1); set(l, -1, 0);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Index out of range.
[line 1] in script"##
        )
    );
    // text index
    let src = r##"var l = list(); push(l, 1); get(l, "0");"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Index out of range.
[line 1] in script"##
        )
    );
    // len of a number
    let src = r##"len(1);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"len() expects a string or a list.
[line 1] in script"##
        )
    );
    // push to a number
    let src = r##"push(1, 2);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"push() expects a list as its first argument.
[line 1] in script"##
        )
    );
}

#[test]
fn hostile_programs_end_in_an_error() {
    let src = r##"var l = list(); for (var i = 0; i < 7; i = i + 1) push(l, l); print(l);"##;
    check!(
        src,
        error_starts(src, r##"String too long."##),
        r##"String too long."##.to_string()
    );
    let src = r##"var l = list(); for (var i = 0; i < 7; i = i + 1) push(l, l); var s = str(l);"##;
    check!(
        src,
        error_starts(src, r##"String too long."##),
        r##"String too long."##.to_string()
    );
    let src = r##"var s = "x"; for (var i = 0; i < 40; i = i + 1) s = s + s;"##;
    check!(
        src,
        error_starts(src, r##"String too long."##),
        r##"String too long."##.to_string()
    );
    let src = r##"var l = list(); while (true) push(l, 1);"##;
    check!(
        src,
        error_starts(src, r##"List too long."##),
        r##"List too long."##.to_string()
    );
    let src = r##"var s = "x"; for (var i = 0; i < 19; i = i + 1) s = s + s; for (var i = 0; i < 12; i = i + 1) print(s);"##;
    check!(
        src,
        error_starts(src, r##"Output limit exceeded."##),
        r##"Output limit exceeded."##.to_string()
    );
    let src = r##"var l = list(); while (true) { var m = list(); push(m, l); l = m; }"##;
    check!(
        src,
        error_starts(src, r##"Out of memory: too many live objects."##),
        r##"Out of memory: too many live objects."##.to_string()
    );
}

#[test]
fn the_limits_are_exact() {
    let src = r##"var s = "x"; for (var i = 0; i < 20; i = i + 1) s = s + s; print(len(s));"##;
    check!(src, go(src), prints("1048576\n"));
    let one_more = format!("{src} s = s + \"x\";");
    check!(
        one_more.clone(),
        go(&one_more).1.map(|e| e.starts_with("String too long.")),
        Some(true)
    );
    let src =
        r##"var l = list(); for (var i = 0; i < 100000; i = i + 1) push(l, i); print(len(l));"##;
    check!(src, go(src), prints("100000\n"));
    let one_more = format!("{src} push(l, 0);");
    check!(
        one_more.clone(),
        go(&one_more).1.map(|e| e.starts_with("List too long.")),
        Some(true)
    );
    // 512 KiB and a line end, seven times, is just over 3.5 MiB; the eighth crosses 4 MiB by its eight line ends.
    let src = r##"var s = "x"; for (var i = 0; i < 19; i = i + 1) s = s + s; for (var i = 0; i < 7; i = i + 1) print(s);"##;
    check!(src, go(src).1, None::<String>);
    let more = format!("{src} print(s);");
    check!(
        more.clone(),
        go(&more).1.map(|e| e.starts_with("Output limit exceeded.")),
        Some(true)
    );
}

#[test]
fn what_is_freed_no_longer_counts() {
    let src = r##"for (var i = 0; i < 150000; i = i + 1) { var t = list(); push(t, list()); } print("done");"##;
    check!(src, go(src), prints("done\n"));
    let src = r##"var l = list(); for (var i = 0; i < 99000; i = i + 1) push(l, list()); print(len(l));"##;
    check!(src, go(src), prints("99000\n"));
}

#[test]
fn an_earlier_run_leaves_nothing_behind() {
    // Two lists that hold each other are never freed. A later run must not pay for them.
    let leak = r##"for (var i = 0; i < 40000; i = i + 1) { var a = list(); var b = list(); push(a, b); push(b, a); }"##;
    for round in 0..3 {
        check!(format!("run {round}"), go(leak).1, None::<String>);
    }
}

#[test]
fn a_loop_that_never_ends_runs_out_of_fuel() {
    for src in [
        "while (true) {}",
        "var n = 0; while (true) { n = n + 1; }",
        "for (;;) continue;",
    ] {
        let error = go_fuel(src, 10_000).1.unwrap_or_default();
        check!(
            src,
            error.starts_with("Fuel exhausted: step limit (10000) exceeded.\n[line 1] in script"),
            true
        );
    }
    let src = "var n = 0; while (n < 100) n = n + 1; print(n);";
    check!(src, go_fuel(src, 100_000), prints("100\n"));
}

#[test]
fn mistakes_before_running_mean_nothing_runs() {
    let o = run("print(1); break;", 1000);
    check!(
        "break",
        (o.out, o.error, o.compile_errors.len()),
        (String::new(), None, 1)
    );
    let o = run("{ var unused = 1; print(2); }", 1000);
    check!(
        "a warning",
        (o.out, o.warnings.len()),
        ("2\n".to_string(), 1)
    );
}

#[test]
fn deep_trees_run() {
    let src = format!(
        "var x = {}1{}; print(x);",
        "(1 + ".repeat(40),
        ")".repeat(40)
    );
    let src2 = src.clone();
    let thread = std::thread::Builder::new().stack_size(64 << 20);
    let got = thread
        .spawn(move || go(&src2))
        .expect("spawn")
        .join()
        .expect("no panic");
    check!(src, got, prints("41\n"));
}

// Random integer arithmetic, checked against Rust's own.
#[allow(dead_code)]
fn sum(rng: &mut anneal_prelude::Rng, depth: u32) -> (String, i64) {
    if depth == 0 || rng.below(3) == 0 {
        let n = rng.int(0, 9);
        return (n.to_string(), n);
    }
    let (a, x) = sum(rng, depth - 1);
    let (b, y) = sum(rng, depth - 1);
    match rng.below(4) {
        0 => (format!("({a} + {b})"), x + y),
        1 => (format!("({a} - {b})"), x - y),
        2 => (format!("({a} * {b})"), x * y),
        _ => (
            format!("(({x} < {y}) and {a} or {b})"),
            if x < y { x } else { y },
        ),
    }
}

#[test]
fn random_arithmetic() {
    let mut rng = anneal_prelude::Rng::new(5401);
    for _ in 0..600 {
        let (text, value) = sum(&mut rng, 4);
        let src = format!("print({text});");
        // A product with zero can be printed as -0.
        let (out, error) = go(&src);
        check!(
            src.clone(),
            (out.trim().parse::<f64>(), error),
            (Ok(value as f64), None)
        );
    }
}

#[test]
fn the_output_limit_is_exact() {
    // Seven lines of 2^19 bytes, then one of 2^19 - 8 bytes: with the line ends, exactly MAX_OUTPUT bytes.
    let src = r##"var s = "x"; for (var i = 0; i < 19; i = i + 1) s = s + s; for (var i = 0; i < 7; i = i + 1) print(s);
var a = "x"; var t = ""; for (var i = 0; i < 19; i = i + 1) { if (i >= 3) t = t + a; a = a + a; } print(t);"##;
    let (out, error) = go(src);
    check!("exactly MAX_OUTPUT bytes", (out.len(), error), (MAX_OUTPUT, None::<String>));
    let more = format!("{src} print(\"\");");
    check!(
        "one line end more",
        go(&more).1.map(|e| e.starts_with("Output limit exceeded.")),
        Some(true)
    );
}
