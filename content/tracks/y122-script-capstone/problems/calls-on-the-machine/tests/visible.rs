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
fn returning_from_inside_loops() {
    // returning from inside loops
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

#[test]
fn globals_may_be_declared_later() {
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
}
