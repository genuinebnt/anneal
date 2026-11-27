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
    // nested
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
}

#[test]
fn closures() {
    // counter
    let src = r##"fun makeCounter() { var n = 0; fun inc() { n = n + 1; return n; } return inc; }
var a = makeCounter(); var b = makeCounter();
print(a()); print(a()); print(b()); print(a());"##;
    check!(
        src,
        go(src),
        prints(
            r##"1
2
1
3
"##
        )
    );
    // shared
    let src = r##"var get_it; var bump;
{ var n = 10; fun g() { return n; } fun b() { n = n + 1; } get_it = g; bump = b; b(); print(n); n = 20; print(g()); }
bump(); bump(); print(get_it());"##;
    check!(
        src,
        go(src),
        prints(
            r##"11
20
22
"##
        )
    );
    // loop capture
    let src = r##"var fs = list(); var gs = list();
for (var i = 0; i < 3; i = i + 1) {
  var j = i;
  fun f() { return j; }
  fun g() { return i; }
  push(fs, f); push(gs, g);
}
for (var k = 0; k < 3; k = k + 1) print(str(get(fs, k)()) + "/" + str(get(gs, k)()));"##;
    check!(
        src,
        go(src),
        prints(
            r##"0/3
1/3
2/3
"##
        )
    );
    // leaving early
    let src = r##"var fs = list();
var i = 0;
while (i < 6) {
  var seen = i * 10; var step = 1;
  fun read() { seen = seen + 1; return seen; }
  push(fs, read);
  i = i + 1;
  if (i == 2) continue;
  { var deeper = i; fun also() { return deeper + seen; } push(fs, also); if (i == 4) break; }
  var after = 99;
}
var junk1 = 1; { var junk2 = 2; var junk3 = 3; var junk4 = 4;
for (var k = 0; k < len(fs); k = k + 1) print(get(fs, k)()); }"##;
    check!(
        src,
        go(src),
        prints(
            r##"1
2
11
21
24
31
35
"##
        )
    );
    // three deep
    let src = r##"fun outer() {
  var x = "x"; var unused_before = 1;
  fun middle() {
    var y = "y";
    fun inner() { x = x + "!"; return x + y; }
    return inner;
  }
  var f = middle(); print(f()); print(x);
  return f;
}
var f = outer(); print(f()); print(f()); var g = outer(); print(f()); "##;
    check!(
        src,
        go(src),
        prints(
            r##"x!y
x!
x!!y
x!!!y
x!y
x!
x!!!!y
"##
        )
    );
    // parameters
    let src = r##"fun adder(a) { fun add(b) { a = a + b; return a; } return add; }
var p = adder(100); var q = adder(0); print(p(1)); print(q(5)); print(p(2));
fun compose(f, g) { fun h(x) { return f(g(x)); } return h; }
fun inc(x) { return x + 1; } fun dbl(x) { return x * 2; }
print(compose(inc, dbl)(5)); print(compose(dbl, inc)(5));"##;
    check!(
        src,
        go(src),
        prints(
            r##"101
5
103
11
12
"##
        )
    );
    // a slot used again
    let src = r##"var f;
{ var a = "first"; fun g() { return a; } f = g; }
{ var b = "second"; print(f()); b = "third"; print(f()); }
fun h() { { var a = 1; fun k() { return a; } f = k; } var z = 2; var w = 3; print(f()); return f; }
print(h()());"##;
    check!(
        src,
        go(src),
        prints(
            r##"first
first
1
1
"##
        )
    );
    // return from inside
    let src = r##"fun make(n) { for (var i = 0; ; i = i + 1) { var k = i * 2; fun get_k() { return k + n; } if (i == 3) { var more = 1; { return get_k; } } } }
var a = make(100); var b = make(200); var x = 1; var y = 2; print(a()); print(b());"##;
    check!(
        src,
        go(src),
        prints(
            r##"106
206
"##
        )
    );
    // a local that calls itself
    let src = r##"fun run_it() { fun count(n) { if (n == 0) return "end"; return count(n - 1); } return count(5); }
print(run_it());
{ fun fact(n) { if (n < 2) return 1; return n * fact(n - 1); } print(fact(6)); }"##;
    check!(
        src,
        go(src),
        prints(
            r##"end
720
"##
        )
    );
    // fixed scope
    let src = r##"var a = "global";
{ fun show() { print(a); } show(); var a = "local"; show(); print(a); }"##;
    check!(
        src,
        go(src),
        prints(
            r##"global
global
local
"##
        )
    );
    // where it was written
    let src = r##"var who = "outer";
fun greet() { return who; }
fun other() { var who = "caller"; return greet(); }
print(other());
fun pair() { var items = list(); var total = 0; fun add(x) { push(items, x); total = total + x; return total; } fun show() { return str(items) + " " + str(total); } var p = list(); push(p, add); push(p, show); return p; }
var p = pair(); get(p, 0)(2); get(p, 0)(3); print(get(p, 1)());"##;
    check!(
        src,
        go(src),
        prints(
            r##"outer
[2, 3] 5
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
    let src = r##"fun wrap(f) { fun g() { return f; } return g; } var h = nil; while (true) h = wrap(h);"##;
    check!(
        src,
        error_starts(src, r##"Out of memory: too many live objects."##),
        r##"Out of memory: too many live objects."##.to_string()
    );
    let src = r##"fun wrap(f) { var box = list(); push(box, f); fun g() { return box; } return g; } var h = nil; while (true) h = wrap(h);"##;
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
fn recursion_that_never_ends_in_a_loop_runs_out_of_fuel_or_stack() {
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
