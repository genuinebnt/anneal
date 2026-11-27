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

#[test]
fn a_counter_remembers() {
    // a counter remembers
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
}

#[test]
fn two_functions_share_one_variable() {
    // two functions share one variable
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
}

#[test]
fn each_turn_has_its_own_local() {
    // each turn has its own local
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
}

#[test]
fn a_variable_outlives_its_scope() {
    // a variable outlives its scope
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
}

#[test]
fn three_levels_deep() {
    // three levels deep
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
}
