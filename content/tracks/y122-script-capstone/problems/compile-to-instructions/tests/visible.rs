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
fn a_sieve() {
    // a sieve
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
}

#[test]
fn loops_with_break_and_continue() {
    // loops with break and continue
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
}

#[test]
fn a_runtime_error_stops_the_program() {
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
}

/// Compiles a program that has no mistakes.
#[allow(dead_code)]
fn compiled(src: &str) -> std::rc::Rc<Proto> {
    let (program, _) = front_end(src).expect("no mistakes");
    compile(&program).expect("supported")
}

#[test]
fn locals_live_in_stack_slots() {
    let src = "{ var a = 1; { var b = a; a = b; print(a); } }";
    let code = compiled(src).code.clone();
    let globals: Vec<usize> = code
        .iter()
        .filter_map(|op| match op {
            Op::GetGlobal(i) | Op::SetGlobal(i) | Op::DefineGlobal(i) => Some(*i),
            _ => None,
        })
        .collect();
    check!("only `print` is looked up by name", globals.len(), 1);
    check!(
        "a is read from slot 1",
        code.contains(&Op::GetLocal(1)),
        true
    );
    check!(
        "b is read from slot 2",
        code.contains(&Op::GetLocal(2)),
        true
    );
    check!(
        "a is written to slot 1",
        code.contains(&Op::SetLocal(1)),
        true
    );
}

#[test]
fn scopes() {
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
}
