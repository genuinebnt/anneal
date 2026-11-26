//! RECIPE 108 (macros): macro_rules! in depth: repetition, fragments, tt-munchers, internal rules, hygiene, $crate.
//! LEVEL: advanced
//! Use in: removing boilerplate, DSL-like literals, generated enums and impls.   Run: cargo run --release --example macro_rules
#![allow(dead_code)]
//!
//! WHAT: a declarative macro matches token trees against rules (like `match` for syntax) and expands to code.
//! Fragment specifiers: `expr`, `ident`, `ty`, `tt` (one token tree: the universal escape hatch), `literal`, `pat`, `block`, `stmt`.
//! Repetition: `$( ... ),*` (zero or more, comma separated), `$( ... );+` (one or more), and `$(,)?` accepts a trailing comma.
//! HOW: prefer a function or generic when one works; reach for macros when you need to generate items, take variable syntax,
//! or capture `stringify!`/`line!`. Internal rules begin with `@name` so helpers live in the same macro. Hygiene: local
//! variables created inside a macro are invisible to the caller and vice versa (items and macro names are NOT hygienic).
//! `$crate` names the defining crate so exported macros keep working when used from other crates.
//! COMMON USES: `vec!`/`format!`-style helpers, `hashmap!` literals (as in the maplit crate), newtype and error-enum boilerplate,
//!   test tables, implementing one trait for many numeric types.
//! PROC MACROS (not runnable here, need their own crate with `proc-macro = true`): derive (`#[derive(MyTrait)]`, how serde
//!   works), attribute (`#[route(GET, "/")]`) and function-like (`sql!(...)`) macros operate on `TokenStream`, typically with the
//!   syn and quote crates. Learn: https://github.com/dtolnay/proc-macro-workshop , inspect expansions with
//!   https://github.com/dtolnay/cargo-expand , and read the declarative-macro bible https://danielkeep.github.io/tlborm/book/
use std::collections::HashMap;
use std::str::FromStr;

macro_rules! hashmap {
    ($($k:expr => $v:expr),* $(,)?) => {{
        #[allow(unused_mut)] // empty `hashmap!{}` never inserts
        let mut m = HashMap::new();
        $( m.insert($k, $v); )*
        m
    }};
}

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum $name { $($variant),+ }
        impl $name {
            const ALL: &'static [$name] = &[$($name::$variant),+];
            fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }
        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s {
                    $($text => Ok($name::$variant),)+
                    other => Err(format!("unknown {}: {other}", stringify!($name))),
                }
            }
        }
    };
}
string_enum!(Color { Red => "red", Green => "green", Blue => "blue", });

macro_rules! impl_id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct $name(u32);
        impl $name { fn new(n: u32) -> Self { $name(n) } fn get(self) -> u32 { self.0 } }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    )+};
}
impl_id!(UserId, OrderId);

// tt-muncher: consume one token tree per step, add 1 each time.
macro_rules! count {
    () => { 0usize };
    ($head:tt $($tail:tt)*) => { 1usize + count!($($tail)*) };
}

// internal rules with @: a tiny "max" with an accumulator.
macro_rules! max_of {
    (@acc $m:expr ;) => { $m };
    (@acc $m:expr ; $h:expr $(, $t:expr)*) => {{ let h = $h; max_of!(@acc if h > $m { h } else { $m } ; $($t),*) }};
    ($first:expr $(, $rest:expr)* $(,)?) => { max_of!(@acc $first ; $($rest),*) };
}

// statements separated by `;`, with pat/block/ty fragments.
macro_rules! check_all {
    ($($val:expr => $p:pat),+ $(,)?) => { $( assert!(matches!($val, $p), "{} !~ {}", stringify!($val), stringify!($p)); )+ };
}
macro_rules! make_fn {
    ($name:ident ( $arg:ident : $t:ty ) -> $r:ty $body:block) => { fn $name($arg: $t) -> $r $body };
}
make_fn!(double(x: i64) -> i64 { x * 2 });

// small test-table macro: generates a Vec of (name, passed)
macro_rules! table {
    ($f:ident; $($input:literal => $expected:expr);+ $(;)?) => {
        vec![$( (concat!(stringify!($f), "(", stringify!($input), ")"), $f($input) == $expected) ),+]
    };
}

// hygiene: `x` inside the macro is a different variable from the caller's `x`.
macro_rules! using_x {
    ($e:expr) => {{
        let x = 100;
        $e + x
    }};
}
// $crate: path robust to where the macro is used.
macro_rules! make_color {
    ($s:literal) => {
        <$crate::Color as std::str::FromStr>::from_str($s)
    };
}

fn main() {
    let m = hashmap! { "a" => 1, "b" => 2, };
    assert_eq!((m.len(), m["b"]), (2, 2));
    let empty: HashMap<i32, i32> = hashmap! {};
    assert!(empty.is_empty());

    assert_eq!(Color::ALL.len(), 3);
    assert_eq!(Color::Green.as_str(), "green");
    assert_eq!("blue".parse::<Color>(), Ok(Color::Blue));
    assert_eq!(
        "pink".parse::<Color>(),
        Err("unknown Color: pink".to_string())
    );
    assert!(
        Color::ALL
            .iter()
            .all(|c| c.as_str().parse::<Color>() == Ok(*c))
    );
    assert_eq!(make_color!("red"), Ok(Color::Red));

    let (u, o) = (UserId::new(3), OrderId::new(3));
    assert_eq!((u.to_string(), o.get()), ("UserId#3".to_string(), 3));
    // assert_eq!(u, o); // ERROR: mismatched types, the macro made two distinct types

    assert_eq!(count!(), 0);
    assert_eq!(count!(a b c d), 4);
    assert_eq!(count!(1 (2 3) [4]), 3); // a delimited group is ONE tt
    assert_eq!(max_of!(3, 9, 4,), 9);
    assert_eq!(max_of!(7), 7);

    check_all!(Some(3) => Some(1..=5), Err::<i32, i32>(1) => Err(_), 'x' => 'a'..='z');
    assert_eq!(double(21), 42);

    fn is_even(n: u32) -> bool {
        n % 2 == 0
    }
    let results = table!(is_even; 2 => true; 3 => false; 10 => true);
    assert!(results.iter().all(|(_, ok)| *ok));
    assert_eq!(results[0].0, "is_even(2)");

    let x = 1;
    assert_eq!(using_x!(x), 101); // the caller's x (1) + the macro's own x (100)

    let line = line!();
    assert!(line > 100 && line!() == line + 1); // line!() is the 1-based line of the call site
    println!(
        "{} / {} / {}",
        stringify!(a + b * 2),
        concat!("v", 1, '.', 2.5),
        results[1].0
    );
}
