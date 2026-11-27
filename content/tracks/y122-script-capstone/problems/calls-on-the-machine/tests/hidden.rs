use solution::*;

/// Output and runtime error of a program that the machine supports.
#[allow(dead_code)]
fn go(src: &str) -> (String, Option<String>) {
    go_fuel(src, 100_000_000)
}

#[allow(dead_code)]
fn go_fuel(src: &str, fuel: u64) -> (String, Option<String>) {
    let o = run_vm(src, fuel).expect("this program is supported");
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

/// Compiles a program that has no mistakes.
#[allow(dead_code)]
fn compiled(src: &str) -> std::rc::Rc<Proto> {
    let (program, _) = front_end(src).expect("no mistakes");
    compile(&program).expect("supported")
}

#[test]
fn programs() {
    // recursion
    let src = r##"fun fib(n) {
  if (n < 2) return n;
  return fib(n - 1) + fib(n - 2);
}
for (var i = 0; i < 10; i = i + 1) print(fib(i));
print(fib(15));"##;
    check!(
        src,
        go(src),
        prints(
            r##"0
1
1
2
3
5
8
13
21
34
610
"##
        )
    );
    // iterative
    let src = r##"fun fib(n) {
  var a = 0; var b = 1;
  for (var i = 0; i < n; i = i + 1) { var t = a + b; a = b; b = t; }
  return a;
}
print(fib(30)); print(fib(1));"##;
    check!(
        src,
        go(src),
        prints(
            r##"832040
1
"##
        )
    );
    // mutual recursion
    let src = r##"fun isEven(n) { if (n == 0) return true; return isOdd(n - 1); }
fun isOdd(n) { if (n == 0) return false; return isEven(n - 1); }
print(isEven(10)); print(isOdd(7)); print(isEven(7));"##;
    check!(
        src,
        go(src),
        prints(
            r##"true
true
false
"##
        )
    );
    // returns
    let src = r##"fun f() {} print(f()); fun g() { return; } print(g());
fun find(limit) { for (var i = 0; ; i = i + 1) { while (true) { { if (i * i > limit) { return i; } } break; } } }
print(find(50)); print(f); print(type(f)); print(f == f); print(f == g);
fun first(a, b) { return a; } var count = 0; fun next() { count = count + 1; return count; } print(first(next(), next())); print(count);"##;
    check!(
        src,
        go(src),
        prints(
            r##"nil
nil
8
<fn f>
function
true
false
1
2
"##
        )
    );
    // sorting
    let src = r##"fun bubble(l) {
  var n = len(l);
  for (var i = 0; i < n; i = i + 1) {
    for (var j = 0; j + 1 < n - i; j = j + 1) {
      var a = get(l, j); var b = get(l, j + 1);
      if (a > b) { set(l, j, b); set(l, j + 1, a); }
    }
  }
  return l;
}
fun qsort(l) {
  if (len(l) < 2) return l;
  var p = get(l, 0); var lo = list(); var hi = list();
  for (var i = 1; i < len(l); i = i + 1) { var x = get(l, i); if (x < p) push(lo, x); else push(hi, x); }
  var r = qsort(lo); push(r, p);
  var h = qsort(hi);
  for (var i = 0; i < len(h); i = i + 1) push(r, get(h, i));
  return r;
}
var data = list();
push(data, 5); push(data, 2); push(data, 9); push(data, 1); push(data, 7); push(data, 3); push(data, 2);
print(str(qsort(data)));
print(str(bubble(data)));"##;
    check!(
        src,
        go(src),
        prints(
            r##"[1, 2, 2, 3, 5, 7, 9]
[1, 2, 2, 3, 5, 7, 9]
"##
        )
    );
    // json
    let src = r##"fun json(v) {
  var t = type(v);
  if (t == "list") {
    var s = "[";
    for (var i = 0; i < len(v); i = i + 1) { if (i > 0) s = s + ","; s = s + json(get(v, i)); }
    return s + "]";
  }
  if (t == "string") return "\"" + v + "\"";
  return str(v);
}
var inner = list(); push(inner, 2); push(inner, 3.5);
var doc = list(); push(doc, 1); push(doc, "a\nb"); push(doc, inner); push(doc, list()); push(doc, true); push(doc, nil);
print(json(doc));"##;
    check!(
        src,
        go(src),
        prints(
            r##"[1,"a
b",[2,3.5],[],true,nil]
"##
        )
    );
    // deep
    let src = r##"fun sum(n) { if (n == 0) return 0; return n + sum(n - 1); } print(sum(199));"##;
    check!(
        src,
        go(src),
        prints(
            r##"19900
"##
        )
    );
    // sieve
    let src = r##"var n = 30; var sieve = list();
for (var i = 0; i <= n; i = i + 1) push(sieve, true);
for (var i = 2; i * i <= n; i = i + 1) {
  if (get(sieve, i)) { for (var j = i * i; j <= n; j = j + i) set(sieve, j, false); }
}
var out = "";
for (var i = 2; i <= n; i = i + 1) { if (get(sieve, i)) out = out + str(i) + " "; }
print(out);"##;
    check!(
        src,
        go(src),
        prints(
            r##"2 3 5 7 11 13 17 19 23 29 
"##
        )
    );
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
    // loops
    let src = r##"var s = 0;
for (var i = 0; i < 10; i = i + 1) { if (i % 2 == 0) continue; if (i > 7) break; s = s + i; }
print(s);
var w = 0; while (true) { w = w + 1; if (w >= 5) break; } print(w);
var c = 0; var n = 0; while (n < 6) { n = n + 1; if (n == 3) continue; c = c + n; } print(c);"##;
    check!(
        src,
        go(src),
        prints(
            r##"16
5
18
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
    // scopes
    let src = r##"var a = "global a"; var b = "global b";
{ var a = "outer a"; { var a = "inner a"; print(a); print(b); b = "changed"; } print(a); { var c = a; a = c + "!"; } print(a); }
print(a); print(b); var a = "again"; print(a); var u; print(u);"##;
    check!(
        src,
        go(src),
        prints(
            r##"inner a
global b
outer a
outer a!
global a
changed
again
nil
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
fn errors_and_traces() {
    // too few arguments
    let src = r##"fun f(a, b) {}
f(1);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Expected 2 arguments but got 1.
[line 2] in script"##
        )
    );
    // too many arguments
    let src = r##"fun f() {}
f(1, 2);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Expected 0 arguments but got 2.
[line 2] in script"##
        )
    );
    // arguments are worked out first
    let src = r##"var n = 1; n(
missing);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Undefined variable 'missing'.
[line 2] in script"##
        )
    );
    // trace
    let src = r##"fun inner() {
  return missing;
}
fun outer() {
  return inner();
}
outer();"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Undefined variable 'missing'.
[line 2] in inner()
[line 5] in outer()
[line 7] in script"##
        )
    );
    // a native inside
    let src = r##"fun a(l) {
  return get(l,
 3);
}
fun b() { a(list()); }

b();"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Index out of range.
[line 3] in a()
[line 5] in b()
[line 7] in script"##
        )
    );
    // arity inside
    let src = r##"fun g(x) {}
fun f() {
  g();
}
f();"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Expected 1 arguments but got 0.
[line 3] in f()
[line 5] in script"##
        )
    );
    // a returned call leaves no frame
    let src = r##"fun ok() { return 1; }
fun bad() {
 ok(); ok();
 return -nil;
}
bad();"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Operand must be a number.
[line 4] in bad()
[line 6] in script"##
        )
    );
    // recursion keeps every frame
    let src = r##"fun f(n) {
  if (n == 0) return nil + 1;
  return f(n - 1);
}
f(2);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Operands must be two numbers or two strings.
[line 2] in f()
[line 3] in f()
[line 3] in f()
[line 5] in script"##
        )
    );
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
fn two_hundred_calls_may_be_active_and_no_more() {
    let src = "fun down(n) { if (n > 1) down(n - 1); }\ndown(200); print(\"fine\");";
    check!(src, go(src), prints("fine\n"));
    let src = "fun down(n) { if (n > 1) down(n - 1); }\ndown(201);";
    let mut want = String::from("Stack overflow.");
    for _ in 0..200 {
        want.push_str("\n[line 1] in down()");
    }
    want.push_str("\n[line 2] in script");
    check!(src, go(src), fails("", &want));
    let src = "fun f() { return f(); }\nf();";
    check!(src, go(src).1.map(|e| e.lines().count()), Some(202));
}

#[test]
fn the_deepest_tree_at_the_deepest_call() {
    let src = format!(
        "fun f(n) {{ if (n == 0) return 0; return {}f(n - 1){}; }} print(f(199));",
        "(1 + ".repeat(40),
        ")".repeat(40)
    );
    check!("40 brackets in a call 199 deep", go(&src), prints("7960\n"));
}

#[test]
fn capturing_is_not_handled_yet() {
    for src in [
        "{ var a = 1; fun f() { return a; } }",
        "fun f() { var a = 1; fun g() { a = 2; } }",
        "fun f(p) { fun g() { fun h() { return p; } } }",
        "{ fun again(n) { return again(n); } }",
        "class A {}",
        "var x; print(x.y);",
    ] {
        check!(src, run_vm(src, 1000).is_err(), true);
    }
}

#[test]
fn a_local_function_that_captures_nothing_is_fine() {
    let src = "var g = 5; fun outer() { var unused = 1; fun inner(a) { var b = a + g; return b; } return inner(unused); } print(outer()); { var x = 1; fun two() { return 2; } print(x + two()); }";
    check!(src, go(src), prints("6\n3\n"));
}

#[test]
fn every_instruction_has_a_line_and_every_jump_a_target() {
    let src = "var n = 0;\n{\n  var a = 1;\n  while (n < 3 and a) {\n    n = n + 1;\n    if (n == 2) continue; else a = !a or -n;\n  }\n  for (;;) { var z; break; }\n}\nprint(n);";
    let script = compiled(src);
    check!(src, script.lines.len(), script.code.len());
    check!(
        src,
        script.lines.iter().all(|&l| (1..=10).contains(&l)),
        true
    );
    check!(
        src,
        script
            .code
            .iter()
            .all(|op| !matches!(op, Op::Jump(t) | Op::JumpIfFalse(t) if *t >= script.code.len())),
        true
    );
    check!(
        "a loop jumps back",
        script
            .code
            .iter()
            .enumerate()
            .any(|(at, op)| matches!(op, Op::Jump(t) if *t < at)),
        true
    );
    check!(
        "the script ends with a return",
        script.code.last(),
        Some(&Op::Return)
    );
}

#[test]
fn the_machine_runs_code_it_is_given() {
    // Written by hand: print(2 * 3), with `print` looked up by name.
    let script = std::rc::Rc::new(Proto {
        name: "script".into(),
        arity: 0,
        code: vec![
            Op::GetGlobal(0),
            Op::Const(1),
            Op::Const(2),
            Op::Binary(BinOp::Mul),
            Op::Call(1),
            Op::Pop,
            Op::Nil,
            Op::Return,
        ],
        lines: vec![1; 8],
        consts: vec![Value::Str("print".into()), Value::Num(2.0), Value::Num(3.0)],
        protos: Vec::new(),
    });
    check!("hand-written code", execute(&script, 100), prints("6\n"));
}

#[test]
fn a_trace_never_says_line_zero() {
    let src = "var n = 0;\nwhile (true) {\n  n = n + 1;\n}";
    for fuel in 1..60 {
        let error = go_fuel(src, fuel).1.unwrap_or_default();
        let fine = ["1", "2", "3"].iter().any(|l| {
            error == format!("Fuel exhausted: step limit ({fuel}) exceeded.\n[line {l}] in script")
        });
        check!(format!("fuel {fuel}: {error}"), fine, true);
    }
}

#[test]
fn fuel_counts_inside_calls_too() {
    let error = go_fuel("fun f(n) { while (true) { n = n + 1; } } f(0);", 5_000)
        .1
        .unwrap_or_default();
    check!(
        "a loop inside a call",
        error,
        "Fuel exhausted: step limit (5000) exceeded.\n[line 1] in f()\n[line 1] in script"
            .to_string()
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

// Random programs, checked against the tree-walking interpreter that your starter already has.

#[allow(dead_code)]
#[derive(Default)]
struct Scope {
    vars: Vec<String>,
    /// Loop counters: read, never written, so that every loop ends.
    counters: Vec<String>,
    funs: Vec<(String, usize)>,
}

#[allow(dead_code)]
struct Gen {
    rng: anneal_prelude::Rng,
    scopes: Vec<Scope>,
    next: usize,
    out: String,
    /// The first scope the current function may see, besides the globals.
    floor: usize,
    functions: bool,
    captures: bool,
}

#[allow(dead_code)]
impl Gen {
    fn fresh(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    fn seen(&self) -> impl Iterator<Item = &Scope> {
        let floor = self.floor;
        self.scopes
            .iter()
            .enumerate()
            .filter(move |(i, _)| *i == 0 || *i >= floor)
            .map(|(_, s)| s)
    }

    fn assignable(&self) -> Vec<String> {
        self.seen().flat_map(|s| s.vars.clone()).collect()
    }

    fn number(&mut self, depth: u32) -> String {
        let mut names = self.assignable();
        names.extend(self.seen().flat_map(|s| s.counters.clone()));
        let funs: Vec<(String, usize)> = self.seen().flat_map(|s| s.funs.clone()).collect();
        if depth > 0 {
            match self.rng.below(10) {
                0..=3 => {
                    let (a, b) = (self.number(depth - 1), self.number(depth - 1));
                    return format!("({a} {} {b})", self.rng.pick(&["+", "-", "*"]));
                }
                4 => {
                    let a = self.number(depth - 1);
                    // Mostly a safe divisor; now and then one that may be zero.
                    let divisor = if self.rng.below(4) == 0 {
                        self.number(depth - 1)
                    } else {
                        (1 + self.rng.below(4)).to_string()
                    };
                    return format!("({a} {} {divisor})", self.rng.pick(&["/", "%"]));
                }
                5 if !funs.is_empty() => {
                    let (name, arity) = self.rng.pick(&funs).clone();
                    let args: Vec<String> = (0..arity).map(|_| self.number(depth - 1)).collect();
                    return format!("{name}({})", args.join(", "));
                }
                6 => {
                    let (c, a, b) = (
                        self.condition(depth - 1),
                        self.number(depth - 1),
                        self.number(depth - 1),
                    );
                    return format!("({c} and {a} or {b})");
                }
                7 => return format!("(-{})", self.number(depth - 1)),
                8 if !self.assignable().is_empty() => {
                    let all = self.assignable();
                    let target = self.rng.pick(&all).clone();
                    return format!("({target} = {})", self.number(depth - 1));
                }
                _ => {}
            }
        }
        if !names.is_empty() && self.rng.below(3) > 0 {
            self.rng.pick(&names).clone()
        } else {
            self.rng
                .pick(&["0", "1", "2", "3", "7", "2.5", "10"])
                .to_string()
        }
    }

    fn condition(&mut self, depth: u32) -> String {
        match if depth == 0 { 0 } else { self.rng.below(5) } {
            3 => {
                let (a, b) = (self.condition(depth - 1), self.condition(depth - 1));
                format!("({a} {} {b})", self.rng.pick(&["and", "or"]))
            }
            4 => format!("(!{})", self.condition(depth - 1)),
            _ => {
                let (a, b) = (
                    self.number(depth.saturating_sub(1)),
                    self.number(depth.saturating_sub(1)),
                );
                format!(
                    "({a} {} {b})",
                    self.rng.pick(&["<", "<=", ">", ">=", "==", "!="])
                )
            }
        }
    }

    fn line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn block(&mut self, most: usize, depth: u32, in_loop: bool, counter: Option<String>) {
        self.line("{");
        let mut scope = Scope::default();
        scope.counters.extend(counter);
        self.scopes.push(scope);
        for _ in 0..1 + self.rng.below(most) {
            self.statement(depth, in_loop);
        }
        self.scopes.pop();
        self.line("}");
    }

    fn statement(&mut self, depth: u32, in_loop: bool) {
        let kind = if in_loop && self.rng.below(7) == 0 {
            99
        } else if depth == 0 {
            self.rng.below(7)
        } else {
            self.rng.below(15)
        };
        match kind {
            0..=2 => {
                let value = self.number(2);
                let name = self.fresh("v");
                let text = format!("var {name} = {value};");
                self.line(&text);
                self.scopes.last_mut().unwrap().vars.push(name);
            }
            3 | 4 if !self.assignable().is_empty() => {
                let all = self.assignable();
                let target = self.rng.pick(&all).clone();
                let text = format!("{target} = {};", self.number(2));
                self.line(&text);
            }
            3..=6 => {
                let text = match self.rng.below(3) {
                    0 => format!("print({});", self.number(3)),
                    1 => format!("print({});", self.condition(2)),
                    _ => format!("print(\"s\" + str({}));", self.number(2)),
                };
                self.line(&text);
            }
            7 => {
                let text = format!("if {}", self.condition(2));
                self.line(&text);
                self.block(3, depth - 1, in_loop, None);
                if self.rng.bool() {
                    self.line("else");
                    self.block(2, depth - 1, in_loop, None);
                }
            }
            8 => self.block(4, depth - 1, in_loop, None),
            9 => {
                let i = self.fresh("i");
                let bound = 1 + self.rng.below(4);
                let text = format!("for (var {i} = 0; {i} < {bound}; {i} = {i} + 1)");
                self.line(&text);
                self.block(4, depth - 1, true, Some(i));
            }
            10 => {
                let w = self.fresh("w");
                let bound = 1 + self.rng.below(4);
                let text = format!("var {w} = 0;\nwhile ({w} < {bound}) {{\n{w} = {w} + 1;");
                self.line(&text);
                self.scopes.last_mut().unwrap().counters.push(w.clone());
                self.scopes.push(Scope::default());
                for _ in 0..1 + self.rng.below(4) {
                    self.statement(depth - 1, true);
                }
                self.scopes.pop();
                self.line("}");
            }
            11 | 12 if self.functions => {
                let name = self.fresh("f");
                let arity = self.rng.below(3);
                let params: Vec<String> = (0..arity).map(|_| self.fresh("p")).collect();
                let text = format!("fun {name}({}) {{", params.join(", "));
                self.line(&text);
                let outer = self.floor;
                if !self.captures {
                    self.floor = self.scopes.len();
                }
                self.scopes.push(Scope {
                    vars: params,
                    ..Scope::default()
                });
                for _ in 0..self.rng.below(4) {
                    self.statement(depth - 1, false);
                }
                if self.rng.below(3) == 0 {
                    let text = format!("if {} return {};", self.condition(1), self.number(1));
                    self.line(&text);
                }
                let text = format!("return {};\n}}", self.number(2));
                self.line(&text);
                self.scopes.pop();
                self.floor = outer;
                let result = format!("print({name}({}));", vec!["2"; arity].join(", "));
                self.line(&result);
                self.scopes.last_mut().unwrap().funs.push((name, arity));
            }
            13 if self.captures => {
                // Functions made in a loop, each holding a variable of one turn, called after the loop is over.
                let (kept, i, own, reader) = (
                    self.fresh("K"),
                    self.fresh("i"),
                    self.fresh("o"),
                    self.fresh("r"),
                );
                let text = format!("var {kept} = list();\nfor (var {i} = 0; {i} < 4; {i} = {i} + 1) {{\nvar {own} = {i} * 10;\nfun {reader}() {{ {own} = {own} + 1; return {own} + {i}; }}\npush({kept}, {reader});");
                self.line(&text);
                self.scopes.push(Scope {
                    vars: vec![own],
                    counters: vec![i],
                    funs: Vec::new(),
                });
                for _ in 0..self.rng.below(4) {
                    self.statement(depth - 1, true);
                }
                self.scopes.pop();
                let at = self.fresh("k");
                let text = format!("}}\nfor (var {at} = 0; {at} < len({kept}); {at} = {at} + 1) print(get({kept}, {at})());");
                self.line(&text);
            }
            99 => {
                let text = format!(
                    "if {} {};",
                    self.condition(1),
                    self.rng.pick(&["break", "continue"])
                );
                self.line(&text);
            }
            _ => {
                let (name, a, b) = (self.fresh("L"), self.number(1), self.number(1));
                let text = format!("var {name} = list(); push({name}, {a}); push({name}, {b}); print(len({name})); print(get({name}, 1)); print(str({name}));");
                self.line(&text);
            }
        }
    }
}

#[allow(dead_code)]
fn random_program(seed: u64, functions: bool, captures: bool) -> String {
    let mut gen = Gen {
        rng: anneal_prelude::Rng::new(seed),
        scopes: vec![Scope::default()],
        next: 0,
        out: String::new(),
        floor: 0,
        functions,
        captures,
    };
    for _ in 0..4 + gen.rng.below(6) {
        gen.statement(3, false);
    }
    gen.out
}

#[test]
fn random_programs_behave_as_on_the_tree_walker() {
    let (mut finished, mut failed) = (0, 0);
    for seed in 0..400 {
        let src = random_program(8000 + seed, true, false);
        let tree = run(&src, 100_000_000);
        assert!(
            tree.compile_errors.is_empty(),
            "{:?}\n{src}",
            tree.compile_errors
        );
        if tree.error.is_some() {
            failed += 1;
        } else {
            finished += 1;
        }
        check!(src.clone(), go(&src), (tree.out, tree.error));
    }
    check!(
        "both endings occur",
        (finished > 100, failed > 10),
        (true, true)
    );
}
