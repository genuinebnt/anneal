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
fn recursion() {
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
fn errors_carry_a_trace() {
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
}

#[test]
fn endless_recursion_is_an_error() {
    let src = r##"fun f() { f(); } f();"##;
    check!(
        src,
        error_starts(
            src,
            r##"Stack overflow.
[line 1] in f()"##
        ),
        r##"Stack overflow.
[line 1] in f()"##
            .to_string()
    );
}
