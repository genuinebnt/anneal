//! RECIPE 1000 (std): Option<T> look-up page: every common method, with the scenario where it is the right tool.
//! LEVEL: simple
//! Use in: config merging, optional fields, caches, linked lists, lazy parsing.   Run: cargo run --release --example std_option_methods
//! COMMON USES: optional config keys and CLI flags, nullable DB columns, "not found" from lookups, cursors and
//! linked structures (`take()` moves out of `&mut`), memo cells (`get_or_insert_with`), `?` early return in parsers.
//! METHODS: (everyday first) Option::map, Option::and_then, Option::unwrap_or, Option::unwrap_or_else, Option::ok_or, Option::as_ref, Option::take, Option::filter, Option::is_some_and, Option::map_or, Option::map_or_else, Option::and_then, Option::and, Option::or, Option::or_else, Option::xor,
//! METHODS: Option::filter, Option::zip, Option::unzip, Option::take, Option::take_if, Option::replace, Option::insert,
//! METHODS: Option::get_or_insert, Option::get_or_insert_with, Option::as_ref, Option::as_mut, Option::as_deref, Option::as_deref_mut,
//! METHODS: Option::copied, Option::cloned, Option::flatten, Option::transpose, Option::ok_or, Option::ok_or_else,
//! METHODS: Option::is_some, Option::is_none, Option::is_some_and, Option::is_none_or, Option::unwrap_or, Option::unwrap_or_default,
//! METHODS: Option::unwrap_or_else, Option::unwrap, Option::expect, Option::inspect, Option::iter, Option::iter_mut, Option::into_iter,
//! METHODS: Option::as_slice, Option::as_mut_slice, Option::sum, Option::product, Iterator::flatten, Iterator::filter_map, Iterator::find_map
//! METHODS: Option::get_or_insert_default, Option::map_or_default
//! DOCS: https://doc.rust-lang.org/std/option/enum.Option.html
//! DEFINED IN: core (`core::option`), re-exported as `std::option`; usable in no_std.
//! NOT COVERED (and why): unwrap_unchecked (unsafe), as_pin_ref/as_pin_mut (see std_smart_pointers_use_cases), zip_with, get_or_try_insert_with,
//! flatten_ref/flatten_mut/into_flat_iter (all unstable in 1.98.1).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   - transform the value if present            -> map            - transform with something that may fail -> and_then
//!   - default if absent                         -> unwrap_or / unwrap_or_else (lazy) / unwrap_or_default
//!   - turn None into an error                   -> ok_or / ok_or_else           - bubble None up -> `?`
//!   - look without moving a String/Vec          -> as_ref / as_deref            - mutate in place -> as_mut
//!   - move the value out of &mut self.field     -> take / replace               - take only if -> take_if
//!   - compute once and cache                    -> get_or_insert_with           - first Some of several -> or / or_else
//!   - keep only if predicate                    -> filter                       - test present+predicate -> is_some_and
//!   - absent means "no limit"                   -> is_none_or                   - Option<Result> -> Result<Option> -> transpose
//!   - Option<&T> -> Option<T>                   -> copied / cloned              - Option<Option<T>> -> flatten
//!   - both present                              -> zip                          - exactly one present -> xor
//!   - drop Nones from an iterator               -> flatten / filter_map         - bind or return early -> let-else
//!
//! How to read this file: sections are grouped by what you want to DO with an optional value, not alphabetically.
//! Every section is a small scenario with asserts; PITFALL lines show the trap and the idiom to use instead.
//! Rule of thumb: Option is a value that may be absent; transform it with combinators, unwrap only where absence
//! is a bug, and prefer `?` / let-else when absence should leave the function.
//! Read: https://doc.rust-lang.org/std/option/ and https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html
//! TRY NEXT:
//!   - [S] In section 2 replace `map_or` with `map(..).unwrap_or(..)` using an expensive default (a `String::from`); use a counter to show when the default is evaluated, then switch to `map_or_else`.
//!   - [S] Predict what `Some(3).xor(Some(4))` returns, then which methods in section 3 would change if you used `or` instead; run it.
//!   - [M] Add `Cursor::pop_back` to the section 9 list (it needs `take()` on a `&mut Option<Box<Node>>` loop); assert it against a `Vec` oracle on 1000 seeded random pushes/pops.
//!   - [M] Port section 1 `merge` to a real struct with 6 optional CLI fields and a `Layers` builder (defaults < file < env < flags); compare with std_env_args_vars.
//!   - [A] PROJECT: combine this with std_result_methods and std_convert_from_into_asref: write `parse_config(&str) -> Result<Config, ConfigError>` where each optional key uses `transpose`, `ok_or_else` and `TryFrom`.
use std::collections::HashMap;

const COVERED: &[&str] = &[
    "get_or_insert_default",
    "map_or_default",
    "map",
    "map_or",
    "map_or_else",
    "and_then",
    "and",
    "or",
    "or_else",
    "xor",
    "filter",
    "zip",
    "unzip",
    "take",
    "take_if",
    "replace",
    "insert",
    "get_or_insert",
    "get_or_insert_with",
    "as_ref",
    "as_mut",
    "as_deref",
    "as_deref_mut",
    "copied",
    "cloned",
    "flatten",
    "transpose",
    "ok_or",
    "ok_or_else",
    "is_some",
    "is_none",
    "is_some_and",
    "is_none_or",
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_or_else",
    "unwrap",
    "expect",
    "inspect",
    "iter",
    "iter_mut",
    "into_iter",
    "as_slice",
    "as_mut_slice",
    "sum",
    "product",
];

// ---------------------------------------------------------------------------------------------------------------
// 1. Config merging: layered optional settings (map, or, or_else, unwrap_or, unwrap_or_default, unwrap_or_else)
// ---------------------------------------------------------------------------------------------------------------
#[derive(Debug, Default, Clone, PartialEq)]
struct Partial {
    port: Option<u16>,
    host: Option<String>,
    verbose: Option<bool>,
}

#[derive(Debug, PartialEq)]
struct Final {
    port: u16,
    host: String,
    verbose: bool,
}

impl Partial {
    /// `self` has priority. `or` takes the first Some; its argument is evaluated EAGERLY, so for anything that
    /// allocates or does I/O use `or_else` (lazy closure).
    fn merge(self, lower: Partial) -> Partial {
        Partial {
            port: self.port.or(lower.port),
            host: self.host.or_else(|| lower.host.clone()),
            verbose: self.verbose.or(lower.verbose),
        }
    }
    fn finish(self) -> Final {
        Final {
            port: self.port.unwrap_or(8080),
            // unwrap_or_else: the default allocates, so build it only when needed.
            host: self.host.unwrap_or_else(|| "localhost".to_string()),
            // unwrap_or_default: bool::default() == false, shorter than unwrap_or(false).
            verbose: self.verbose.unwrap_or_default(),
        }
    }
}

fn section_config_merge() {
    let defaults = Partial {
        port: Some(80),
        host: Some("0.0.0.0".into()),
        verbose: None,
    };
    let file = Partial {
        port: None,
        host: Some("example.org".into()),
        verbose: Some(true),
    };
    let flags = Partial {
        port: Some(9000),
        ..Partial::default()
    };
    let merged = flags.merge(file).merge(defaults);
    assert_eq!(
        merged.clone().finish(),
        Final {
            port: 9000,
            host: "example.org".into(),
            verbose: true
        }
    );
    assert_eq!(
        Partial::default().finish(),
        Final {
            port: 8080,
            host: "localhost".into(),
            verbose: false
        }
    );
    println!("1. layered config merge -> {:?}", merged.finish());
}

// ---------------------------------------------------------------------------------------------------------------
// 2. Transforming: map / map_or / map_or_else / and_then / and / filter / inspect
// ---------------------------------------------------------------------------------------------------------------
fn parse_kv(line: &str) -> Option<(&str, &str)> {
    line.split_once('=')
}

fn section_transform() {
    // map: Some(x) -> Some(f(x)); None stays None. Use it when f cannot fail.
    assert_eq!(Some("42").map(str::len), Some(2));
    // and_then (flatMap): f itself returns Option. map would give Option<Option<_>>.
    let n: Option<u32> = Some("42").and_then(|s| s.parse().ok());
    assert_eq!(n, Some(42));
    let nested: Option<Option<u32>> = Some("42").map(|s| s.parse().ok());
    assert_eq!(nested.flatten(), n);
    // map_or / map_or_else: collapse to a plain value without an intermediate Option.
    assert_eq!(parse_kv("a=1").map_or(0, |(_, v)| v.len()), 1);
    assert_eq!(
        parse_kv("garbage").map_or_else(|| "missing".to_string(), |(k, _)| k.to_string()),
        "missing"
    );
    // Alternative side by side: map(..).unwrap_or(..) evaluates its default eagerly, map_or does too.
    let mut evaluated = 0;
    let mut costly = || {
        evaluated += 1;
        String::from("default")
    };
    let _ = Some(1).map(|n| n.to_string()).unwrap_or(costly()); // costly() ran although unused
    let _ = Some(1).map_or_else(|| costly(), |n| n.to_string()); // lazy: not run
    assert_eq!(evaluated, 1, "only the eager variant evaluated its default");
    // and: discard self, take the other if self is Some. A "both must be present" gate.
    assert_eq!(Some(1).and(Some("x")), Some("x"));
    assert_eq!(None::<i32>.and(Some("x")), None);
    // filter: keep value only if predicate holds (validation without Result).
    let even = |x: &i32| x % 2 == 0;
    assert_eq!(Some(4).filter(even), Some(4));
    assert_eq!(Some(5).filter(even), None);
    // inspect (since 1.76): peek for logging without changing the value.
    let mut log = vec![];
    let r = Some(7).inspect(|v| log.push(*v)).map(|v| v * 2);
    assert_eq!((r, log), (Some(14), vec![7]));
    println!("2. map/and_then/filter/inspect ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 3. Choosing between options: or / or_else / xor / zip / unzip
// ---------------------------------------------------------------------------------------------------------------
fn section_choose() {
    // or_else with a fallible lookup chain: env var, then file, then built-in.
    let env: HashMap<&str, &str> = HashMap::from([("PORT", "9")]);
    let file: HashMap<&str, &str> = HashMap::from([("PORT", "7"), ("HOST", "h")]);
    let get = |k: &str| env.get(k).copied().or_else(|| file.get(k).copied());
    assert_eq!(get("PORT"), Some("9"));
    assert_eq!(get("HOST"), Some("h"));
    assert_eq!(get("NOPE"), None);
    // xor: exactly one side set. Useful for mutually exclusive flags (--file XOR --stdin).
    let (file_arg, stdin_arg) = (Some("a.txt"), None::<&str>);
    assert_eq!(file_arg.xor(stdin_arg), Some("a.txt"));
    assert_eq!(Some(1).xor(Some(2)), None, "both set -> error condition");
    assert_eq!(
        None::<i32>.xor(None),
        None,
        "neither set -> also None; xor can't tell the two Nones apart"
    );
    // zip: pair up two Options; None if either is None. Great for (width, height) style pairs.
    assert_eq!(Some(3).zip(Some('c')), Some((3, 'c')));
    assert_eq!(Some(3).zip(None::<char>), None);
    // unzip (since 1.66): inverse.
    let (a, b): (Option<i32>, Option<char>) = Some((1, 'z')).unzip();
    assert_eq!((a, b), (Some(1), Some('z')));
    let (a, b): (Option<i32>, Option<char>) = None.unzip();
    assert_eq!((a, b), (None, None));
    println!("3. or/or_else/xor/zip/unzip ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 4. Predicates: is_some / is_none / is_some_and / is_none_or
// ---------------------------------------------------------------------------------------------------------------
fn section_predicates() {
    let limit: Option<u32> = Some(10);
    // is_some_and (1.70): "present AND satisfies". Replaces `matches!(x, Some(v) if ..)` and `x.map_or(false, ..)`.
    assert!(limit.is_some_and(|l| l >= 10));
    assert!(!None::<u32>.is_some_and(|_| true));
    // is_none_or (1.82): "absent OR satisfies". A missing limit means unlimited, so anything is allowed.
    let allowed = |limit: Option<u32>, n: u32| limit.is_none_or(|l| n <= l);
    assert!(allowed(None, 1_000_000));
    assert!(allowed(Some(10), 10));
    assert!(!allowed(Some(10), 11));
    assert!(limit.is_some() && !limit.is_none());
    // PITFALL: is_some_and takes self by value. For a non-Copy Option<String> use `.as_ref()` first or the Option moves.
    let name = Some(String::from("abc"));
    assert!(name.as_ref().is_some_and(|s| s.len() == 3));
    assert_eq!(name.as_deref(), Some("abc")); // still usable
    println!("4. is_some_and/is_none_or ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 5. Borrowing views: as_ref / as_mut / as_deref / as_deref_mut / copied / cloned / as_slice
// ---------------------------------------------------------------------------------------------------------------
fn section_views() {
    let mut title: Option<String> = Some("hello".into());
    // as_ref: Option<String> -> Option<&String> so you can look without moving.
    assert_eq!(title.as_ref().map(|s| s.len()), Some(5));
    // as_deref: Option<String> -> Option<&str> (via Deref). Ideal for passing to fns taking Option<&str>.
    fn greet(n: Option<&str>) -> String {
        format!("hi {}", n.unwrap_or("stranger"))
    }
    assert_eq!(greet(title.as_deref()), "hi hello");
    // as_mut: modify the inside in place.
    if let Some(s) = title.as_mut() {
        s.push('!');
    }
    // as_deref_mut: Option<String> -> Option<&mut str>.
    if let Some(s) = title.as_deref_mut() {
        s.make_ascii_uppercase();
    }
    assert_eq!(title.as_deref(), Some("HELLO!"));
    // copied / cloned: Option<&T> -> Option<T>. copied for Copy types, cloned for Clone.
    let v = vec![1, 2, 3];
    assert_eq!(v.first().copied(), Some(1));
    let names = vec![String::from("a")];
    let first: Option<String> = names.first().cloned();
    assert_eq!(first.as_deref(), Some("a"));
    // as_slice (1.75): treat an Option as a 0- or 1-element slice (feeds slice-taking APIs).
    assert_eq!(Some(5).as_slice(), &[5]);
    assert_eq!(None::<i32>.as_slice(), &[] as &[i32]);
    let mut o = Some(1);
    o.as_mut_slice()[0] = 9;
    assert_eq!(o, Some(9));
    println!("5. views ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 6. Moving out of &mut: take / replace / insert / take_if / get_or_insert(_with)
// ---------------------------------------------------------------------------------------------------------------
struct Cache {
    slot: Option<Vec<u32>>,
    computed: u32,
}
impl Cache {
    /// get_or_insert_with: compute once, hand out `&mut` to the cached value afterwards.
    fn data(&mut self) -> &mut Vec<u32> {
        let computed = &mut self.computed;
        self.slot.get_or_insert_with(|| {
            *computed += 1;
            (1..=5).collect()
        })
    }
}

fn section_mutation() {
    // take(): leave None behind, give you the value. The only way to move out from behind `&mut` without Clone.
    let mut pending = Some(String::from("job"));
    let job = pending.take();
    assert_eq!((job.as_deref(), pending.is_none()), (Some("job"), true));
    // replace: put a new value, get the old one back (like mem::replace but typed).
    let mut cur = Some(1);
    assert_eq!(cur.replace(2), Some(1));
    // insert: unconditionally set and get &mut to it.
    *cur.insert(10) += 1;
    assert_eq!(cur, Some(11));
    // take_if (1.80): take only when the predicate holds, e.g. expire a cache entry.
    let mut token = Some((String::from("t"), 100u64)); // value, expiry
    let now = 150;
    let expired = token.take_if(|(_, exp)| *exp < now);
    assert!(expired.is_some() && token.is_none());
    let mut fresh = Some(("f", 200u64));
    assert!(fresh.take_if(|(_, e)| *e < now).is_none() && fresh.is_some());
    // get_or_insert: eager default. get_or_insert_with: lazy.
    let mut c = Cache {
        slot: None,
        computed: 0,
    };
    c.data().push(6);
    c.data().push(7);
    assert_eq!(c.computed, 1, "closure ran exactly once");
    assert_eq!(c.data().len(), 7);
    let mut x = None;
    *x.get_or_insert(5) += 1;
    assert_eq!(x, Some(6));
    // PITFALL: `if let Some(v) = &mut self.opt { .. } else { self.opt = Some(..) }` fights the borrow checker
    // in some shapes (conditional return of the borrow); get_or_insert_with avoids that.
    // get_or_insert_default (since 1.83): same as get_or_insert_with(T::default); "give me the bucket, creating it".
    let mut bucket: Option<Vec<u8>> = None;
    bucket.get_or_insert_default().push(1);
    bucket.get_or_insert_default().push(2);
    assert_eq!(bucket, Some(vec![1, 2]));
    // map_or_default (since 1.98): map the value, or T::default() when None.
    assert_eq!(Some("abc").map_or_default(str::len), 3);
    assert_eq!(None::<&str>.map_or_default(str::len), 0);
    println!("6. take/replace/insert/take_if/get_or_insert ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 7. Conversions: ok_or / ok_or_else / transpose / flatten / Option<Option>
// ---------------------------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
enum CfgErr {
    Missing(&'static str),
    Bad(String),
}

fn port_from(map: &HashMap<&str, &str>) -> Result<u16, CfgErr> {
    // ok_or_else: build the error lazily; ok_or would construct it even for Some.
    let raw = map.get("port").copied().ok_or(CfgErr::Missing("port"))?;
    raw.parse().map_err(|_| CfgErr::Bad(raw.to_string()))
}

fn optional_port(map: &HashMap<&str, &str>) -> Result<Option<u16>, std::num::ParseIntError> {
    // transpose: Option<Result<T,E>> -> Result<Option<T>,E>. "absent is fine, present-but-bad is an error".
    map.get("port").map(|s| s.parse::<u16>()).transpose()
}

fn section_conversions() {
    let m = HashMap::from([("port", "80")]);
    assert_eq!(port_from(&m), Ok(80));
    assert_eq!(port_from(&HashMap::new()), Err(CfgErr::Missing("port")));
    assert_eq!(
        port_from(&HashMap::from([("port", "x")])),
        Err(CfgErr::Bad("x".into()))
    );
    assert_eq!(optional_port(&HashMap::new()), Ok(None));
    assert_eq!(optional_port(&m), Ok(Some(80)));
    assert!(optional_port(&HashMap::from([("port", "x")])).is_err());
    assert_eq!(Some(Ok::<i32, ()>(1)).transpose(), Ok(Some(1)));
    // Option<Option<T>> arises from map(fallible) or HashMap<K, Option<V>>::get. flatten collapses it.
    let h: HashMap<&str, Option<i32>> = HashMap::from([("a", Some(1)), ("b", None)]);
    assert_eq!(h.get("a").copied().flatten(), Some(1));
    assert_eq!(h.get("b").copied().flatten(), None); // key present, value null
    assert_eq!(h.get("c").copied().flatten(), None); // key absent: same result, so match on the nested form if you must tell apart
    assert_eq!(h.get("b"), Some(&None));
    assert_eq!(h.get("c"), None);
    println!("7. ok_or/transpose/flatten ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 8. Unwrapping: unwrap / expect and when they are legitimate, plus catch_unwind showing the panic
// ---------------------------------------------------------------------------------------------------------------
fn section_unwrap() {
    // Legit: invariant established by the code just above (the compiler cannot see it).
    let words = ["x", "y"];
    let last = words.last().expect("array literal is non-empty");
    assert_eq!(*last, "y");
    // PITFALL: unwrap on None panics with a message pointing at the call site. expect(msg) says WHY it can't be None.
    let r = std::panic::catch_unwind(|| None::<i32>.expect("port must be set before start"));
    assert!(r.is_err());
    // Prefer: unwrap_or / ? / let-else. Never unwrap on user input.
    println!("8. unwrap/expect ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 9. Linked-list cursor with take(): the classic "move out of &mut" scenario
// ---------------------------------------------------------------------------------------------------------------
struct Node {
    val: i32,
    next: Option<Box<Node>>,
}
#[derive(Default)]
struct Stack {
    head: Option<Box<Node>>,
}
impl Stack {
    fn push(&mut self, val: i32) {
        // take(): move the old head out (leaving None), then store it as `next` of the new node.
        let next = self.head.take();
        self.head = Some(Box::new(Node { val, next }));
    }
    fn pop(&mut self) -> Option<i32> {
        // `?`-free version: and_then-like with take: head.take().map(...)
        self.head.take().map(|n| {
            self.head = n.next;
            n.val
        })
    }
    fn peek(&self) -> Option<&i32> {
        self.head.as_ref().map(|n| &n.val)
    }
    fn peek_mut(&mut self) -> Option<&mut i32> {
        self.head.as_mut().map(|n| &mut n.val)
    }
    fn reverse(&mut self) {
        let mut prev: Option<Box<Node>> = None;
        let mut cur = self.head.take();
        while let Some(mut node) = cur {
            cur = node.next.take(); // detach the tail
            node.next = prev; // point back
            prev = Some(node);
        }
        self.head = prev;
    }
    fn iter(&self) -> impl Iterator<Item = &i32> {
        // Option<&Node> as a cursor: successors repeatedly applies `next`.
        std::iter::successors(self.head.as_deref(), |n| n.next.as_deref()).map(|n| &n.val)
    }
}
impl Drop for Stack {
    fn drop(&mut self) {
        // iterative drop: the default recursive drop would overflow the stack on a long list.
        let mut cur = self.head.take();
        while let Some(mut n) = cur {
            cur = n.next.take();
        }
    }
}

fn section_cursor() {
    let mut s = Stack::default();
    for i in 1..=5 {
        s.push(i);
    }
    assert_eq!(s.peek(), Some(&5));
    *s.peek_mut().unwrap() = 50;
    assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![50, 4, 3, 2, 1]);
    s.reverse();
    assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3, 4, 50]);
    assert_eq!(s.pop(), Some(1));
    let mut big = Stack::default();
    for i in 0..200_000 {
        big.push(i);
    }
    drop(big); // would overflow with the derived recursive Drop
    println!("9. linked-list cursor ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 10. Control flow: ?, let-else, if-let chains, matches!, Option<Option>
// ---------------------------------------------------------------------------------------------------------------
fn first_even_square(v: &[i32]) -> Option<i32> {
    let first = v.first()?; // `?` on Option returns None from the function
    let x = v.iter().find(|x| *x % 2 == 0)?;
    Some(first * x)
}

fn describe(line: &str) -> String {
    // let-else: bind or diverge. Keeps the happy path unindented.
    let Some((k, v)) = line.split_once(':') else {
        return format!("no colon in {line:?}");
    };
    // if-let chain (edition 2024, stable since 1.88): mix patterns and boolean conditions in one `if`.
    if let Ok(n) = v.trim().parse::<i32>()
        && n > 0
        && !k.is_empty()
    {
        format!("{k} is positive {n}")
    } else {
        format!("{k} is something else")
    }
}

fn section_control_flow() {
    assert_eq!(first_even_square(&[3, 4]), Some(12));
    assert_eq!(first_even_square(&[]), None);
    assert_eq!(first_even_square(&[1, 3]), None);
    assert_eq!(describe("a: 5"), "a is positive 5");
    assert_eq!(describe("a: -5"), "a is something else");
    assert_eq!(describe("nocolon"), "no colon in \"nocolon\"");
    assert!(matches!(Some(3), Some(n) if n > 2));
    // match on Option<Option<T>>: distinguishes "no entry" from "entry is explicitly null".
    let v: Option<Option<i32>> = Some(None);
    let s = match v {
        None => "absent",
        Some(None) => "null",
        Some(Some(_)) => "value",
    };
    assert_eq!(s, "null");
    println!("10. ?/let-else/if-let chains ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 11. Option as an iterator: iter / iter_mut / into_iter / flatten / filter_map / sum / product / collect
// ---------------------------------------------------------------------------------------------------------------
fn section_iteration() {
    let o = Some(3);
    assert_eq!(o.iter().count(), 1);
    assert_eq!(None::<i32>.iter().count(), 0);
    let mut m = Some(1);
    for x in m.iter_mut() {
        *x += 1;
    }
    assert_eq!(m, Some(2));
    // Option implements IntoIterator, so `flatten` on an iterator of Options drops the Nones.
    let v = vec![Some(1), None, Some(3)];
    assert_eq!(v.iter().flatten().sum::<i32>(), 4);
    assert_eq!(v.iter().filter_map(|x| *x).collect::<Vec<_>>(), vec![1, 3]);
    // Chain an optional extra element without allocating a Vec: iter::chain(Option).
    let base = [1, 2];
    let extra = Some(9);
    assert_eq!(
        base.iter().copied().chain(extra).collect::<Vec<_>>(),
        vec![1, 2, 9]
    );
    // collect Option<Vec<_>>: all-or-nothing; stops at the first None. Same for sum/product.
    let all: Option<Vec<i32>> = v.iter().copied().collect();
    assert_eq!(all, None);
    let ok: Option<Vec<i32>> = [Some(1), Some(2)].into_iter().collect();
    assert_eq!(ok, Some(vec![1, 2]));
    assert_eq!([Some(2), Some(5)].into_iter().sum::<Option<i32>>(), Some(7));
    assert_eq!([Some(2), None].into_iter().product::<Option<i32>>(), None);
    // find_map: first Some produced by a closure (parse the first valid number).
    let first_num = ["a", "7", "8"].iter().find_map(|s| s.parse::<i32>().ok());
    assert_eq!(first_num, Some(7));
    println!("11. Option as iterator ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 12. Lazy parse: Option field filled on first use, plus Less known notes
// ---------------------------------------------------------------------------------------------------------------
struct LazyLine<'a> {
    raw: &'a str,
    parsed: Option<Vec<u32>>,
    parses: u32,
}
impl<'a> LazyLine<'a> {
    fn numbers(&mut self) -> &[u32] {
        let (raw, parses) = (self.raw, &mut self.parses);
        self.parsed.get_or_insert_with(|| {
            *parses += 1;
            raw.split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        })
    }
}

fn section_lazy_and_less_known() {
    let mut l = LazyLine {
        raw: "1, 2, x, 4",
        parsed: None,
        parses: 0,
    };
    assert_eq!(l.numbers(), &[1, 2, 4]);
    assert_eq!(l.numbers().len(), 3);
    assert_eq!(l.parses, 1);
    // Less known: Option<T> ordering: None < Some(_). Handy for "no deadline sorts first".
    let mut v = vec![Some(3), None, Some(1)];
    v.sort();
    assert_eq!(v, vec![None, Some(1), Some(3)]);
    assert!(None < Some(i32::MIN));
    // Less known: niche optimisation, Option<&T> / Option<Box<T>> are pointer sized.
    assert_eq!(size_of::<Option<&u64>>(), size_of::<&u64>());
    assert_eq!(size_of::<Option<Box<u64>>>(), size_of::<usize>());
    assert_eq!(size_of::<Option<std::num::NonZeroU32>>(), 4);
    assert_eq!(size_of::<Option<u32>>(), 8);
    // Less known: Option<T>: Default is None (no T: Default bound needed).
    struct NoDefault;
    let d: Option<NoDefault> = Default::default();
    assert!(d.is_none());
    // Less known: From<T> for Option<T> lets `impl Into<Option<T>>` accept both x and None-less calls.
    fn set_timeout(t: impl Into<Option<u32>>) -> Option<u32> {
        t.into()
    }
    assert_eq!(set_timeout(5), Some(5));
    assert_eq!(set_timeout(None), None);
    println!("12. lazy parse + less known ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 0. Everyday core: the ~12 methods used in most real code (details and alternatives in later sections)
// ---------------------------------------------------------------------------------------------------------------
fn section_everyday_core() {
    let name: Option<String> = Some("  Ada ".into());
    let cleaned = name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    assert_eq!(cleaned.as_deref(), Some("Ada"));
    let age: Option<u32> = "36".parse().ok();
    assert_eq!(age.and_then(|a| a.checked_sub(40)), None);
    assert_eq!(age.unwrap_or(0), 36);
    assert_eq!(None::<u32>.unwrap_or_else(|| 1 + 1), 2);
    assert_eq!(None::<u32>.ok_or("missing"), Err("missing"));
    let mut slot = Some(5);
    assert_eq!(slot.take(), Some(5));
    assert!(slot.is_none() && Some(3).is_some_and(|x| x > 2));
    println!("0. everyday core ok");
}

// ---------------------------------------------------------------------------------------------------------------
// 13. Production tips and tricks
// ---------------------------------------------------------------------------------------------------------------
fn section_production_tips() {
    // 1. `map(..).unwrap_or(..)` -> `map_or` / `is_some_and` (clippy::map_unwrap_or, option_map_or_none).
    let o = Some(4);
    assert_eq!(o.map(|x| x > 3).unwrap_or(false), o.is_some_and(|x| x > 3));
    // 2. Hand-written match that is a map (clippy::manual_map).
    let m = match o {
        Some(x) => Some(x + 1),
        None => None,
    };
    assert_eq!(m, o.map(|x| x + 1));
    // 3. `unwrap_or(expensive())` evaluates eagerly (clippy::or_fun_call): use the _else form.
    let mut calls = 0;
    let _ = Some(1).unwrap_or_else(|| {
        calls += 1;
        0
    });
    assert_eq!(calls, 0);
    // 4. Option<&T> from lookups: `.copied()` / `.cloned()` instead of `.map(|x| *x)` (clippy::map_clone).
    let v = [1, 2];
    assert_eq!(v.first().copied(), v.first().map(|x| *x));
    // 5. Return Option<&str> from structs holding String: `as_deref` avoids cloning (clippy::option_as_ref_deref).
    struct U {
        nick: Option<String>,
    }
    impl U {
        fn nick(&self) -> Option<&str> {
            self.nick.as_deref()
        }
    }
    assert_eq!(
        U {
            nick: Some("a".into())
        }
        .nick(),
        Some("a")
    );
    // 6. `.is_some()` then `.unwrap()` is a smell (clippy::unnecessary_unwrap): bind with if-let/let-else.
    let Some(x) = o else { unreachable!() };
    assert_eq!(x, 4);
    // 7. Errors from absence: attach context lazily with ok_or_else so the hot path allocates nothing.
    let e: Result<i32, String> = None.ok_or_else(|| format!("key {} missing", "k"));
    assert_eq!(e.unwrap_err(), "key k missing");
    // 8. `Option<Option<T>>` in APIs is a smell; use an enum (Unset/Null/Value) for PATCH-style updates.
    #[derive(Debug, PartialEq)]
    enum Patch<T> {
        Unset,
        Null,
        Value(T),
    }
    let from = |o: Option<Option<i32>>| match o {
        None => Patch::Unset,
        Some(None) => Patch::Null,
        Some(Some(v)) => Patch::Value(v),
    };
    assert_eq!(from(Some(None)), Patch::Null);
    // 9. Prefer Option<NonZeroU32>/Option<&T>/Option<Box<T>> in big arrays: no size overhead (niche).
    assert_eq!(size_of::<Option<std::num::NonZeroU32>>(), size_of::<u32>());
    // 10. `iter().flatten()` over Vec<Option<T>> instead of filter(is_some).map(unwrap) (clippy::filter_map_identity).
    let vs = vec![Some(1), None, Some(2)];
    assert_eq!(
        vs.iter().flatten().count(),
        vs.iter().filter(|x| x.is_some()).count()
    );
    // 11. `take()` before an await/long call to avoid holding a borrow of self; put it back after.
    let mut st = Some(String::from("state"));
    let local = st.take().unwrap();
    st = Some(local + "!");
    assert_eq!(st.as_deref(), Some("state!"));
    // 12. `#[must_use]`-style: functions returning Option should say what None means in docs; use `Option<T>`
    // for "not found", `Result<T,E>` when the caller needs the reason.
    println!("13. production tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday_core();
    section_config_merge();
    section_transform();
    section_choose();
    section_predicates();
    section_views();
    section_mutation();
    section_conversions();
    section_unwrap();
    section_cursor();
    section_control_flow();
    section_iteration();
    section_lazy_and_less_known();
    section_production_tips();
    println!("all {} Option methods exercised; done", COVERED.len());
}
