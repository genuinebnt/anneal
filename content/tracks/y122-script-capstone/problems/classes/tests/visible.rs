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
fn a_hierarchy() {
    // a hierarchy
    let src = r##"class Animal {
  init(name) { this.name = name; }
  speak() { return this.name + " makes a sound"; }
  describe() { return "Animal:" + this.name; }
}
class Dog < Animal {
  init(name, breed) { super.init(name); this.breed = breed; }
  speak() { return super.speak() + " (woof)"; }
}
var d = Dog("Rex", "lab");
print(d.speak()); print(d.describe()); print(d.breed); print(d); print(Dog);
var m = d.speak; print(m());"##;
    check!(
        src,
        go(src),
        prints(
            r##"Rex makes a sound (woof)
Animal:Rex
lab
Dog instance
<class Dog>
Rex makes a sound (woof)
"##
        )
    );
}

#[test]
fn chained_and_bound_methods() {
    // chained and bound methods
    let src = r##"class Counter { init() { this.n = 0; } inc() { this.n = this.n + 1; return this; } get() { return this.n; } }
var c = Counter(); c.inc().inc().inc(); print(c.get());
var f = c.inc; f(); print(c.n);
print(c.init() == c); print(c.n);
class A { m() { return "A"; } }
class B < A { m() { return "B>" + super.m(); } }
class C < B { m() { return "C>" + super.m(); } }
print(C().m()); print(type(C)); print(type(C())); print(type(c.inc)); print(c.inc);"##;
    check!(
        src,
        go(src),
        prints(
            r##"3
4
true
0
C>B>A
class
instance
function
<fn inc>
"##
        )
    );
}

#[test]
fn super_is_decided_where_it_is_written() {
    // super is decided where it is written
    let src = r##"class A { who() { return "A"; } }
class B < A { who() { return "B"; } ask() { return super.who(); } }
class C < B { who() { return "C"; } }
print(C().ask()); print(B().ask()); print(C().who());
var m = C().ask; print(m());"##;
    check!(
        src,
        go(src),
        prints(
            r##"A
A
C
A
"##
        )
    );
}

#[test]
fn property_errors() {
    // property of a number
    let src = r##"var x = 1;
print(x.y);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Only instances have properties.
[line 2] in script"##
        )
    );
    // no such property
    let src = r##"class A {}
print(A().z);"##;
    check!(
        src,
        go(src),
        fails(
            r##""##,
            r##"Undefined property 'z'.
[line 2] in script"##
        )
    );
}

#[test]
fn fields_and_late_binding() {
    // fields
    let src = r##"class Box {} var a = Box(); var b = Box(); a.x = 1; b.x = 2; print(a.x + b.x); print(a == a); print(a == b); print(a.x = 7); print(a.x);
class P { f() { return "method"; } } var p = P(); fun mine() { return "field"; } print(p.f()); p.f = mine; print(p.f());
var held = a; held.y = list(); push(a.y, 3); print(held.y);
a.self = a; print(a.self.self.x); print(a);"##;
    check!(
        src,
        go(src),
        prints(
            r##"3
true
false
7
7
method
field
[3]
7
Box instance
"##
        )
    );
    // late binding
    let src = r##"class Base { hello() { return "hi " + this.name(); } name() { return "base"; } }
class Kid < Base { name() { return "kid"; } }
print(Kid().hello()); print(Base().hello());"##;
    check!(
        src,
        go(src),
        prints(
            r##"hi kid
hi base
"##
        )
    );
}
