//! RECIPE 32 (language): lifetimes: what they are and the 5 situations you will actually hit.
//! LEVEL: simple
//! Use in: any struct/function that returns or stores a reference (parsers, views over input).   Run: cargo run --example lifetimes
//! COMMON USES: zero-copy parsers, views over buffers, iterators that borrow: the lifetime patterns behind most Rust libraries.
//!
//! A lifetime is the compiler's name for "how long this borrow is valid". It never changes runtime behaviour;
//! it only lets the compiler PROVE a reference never outlives the data it points to.
//! Read: Rust Book ch. 10.3 https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html
//!       and the Nomicon on lifetimes https://doc.rust-lang.org/nomicon/lifetimes.html
#![allow(dead_code)]

// 1. ELISION: one input reference => the output borrows from it. No annotation needed.
fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

// 2. TWO inputs: you must say which one the output borrows from. Here only `a`.
fn prefer_first<'a>(a: &'a str, _b: &str) -> &'a str {
    a
}

// 3. A STRUCT that borrows from its input. `'a` ties the struct's life to the source text.
//    Zero copies: `name` and `deps` are slices of `line`.
struct Entry<'a> {
    name: &'a str,
    deps: Vec<&'a str>,
}

fn parse_entry(line: &str) -> Entry<'_> {
    let (name, rest) = line.split_once(':').unwrap_or((line, ""));
    Entry {
        name: name.trim(),
        deps: rest
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

// 4. Returning a reference INTO a collection: the result lives as long as the collection borrow.
fn longest<'a>(items: &'a [String]) -> Option<&'a String> {
    items.iter().max_by_key(|s| s.len())
}

// 5. 'static: data that lives for the whole program (string literals, consts, leaked/owned-by-nobody data).
//    `T: 'static` as a bound means "T owns its data / holds no short borrows" (needed by `thread::spawn`).
const TOOL: &str = "flowforge"; // &'static str

fn main() {
    println!("{}", first_word("hello brave world"));
    println!("{}", prefer_first("a", "b"));
    let text = String::from("transcode: validate, upload");
    let e = parse_entry(&text);
    println!("{} depends on {:?}", e.name, e.deps);
    let names = vec!["a".to_string(), "longer".to_string()];
    println!("{:?}", longest(&names));
    println!("{TOOL}");

    // What the compiler PREVENTS (uncomment to see the error):
    // let dangling: &str;
    // { let temp = String::from("short-lived"); dangling = first_word(&temp); }
    // println!("{dangling}");   // error[E0597]: `temp` does not live long enough
    //
    // Rule of thumb: if you fight lifetimes, ask whether the struct should OWN its data (String, Vec) instead
    // of borrowing it. Borrow for short-lived views; own for long-lived state.
}
