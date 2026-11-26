//! RECIPE 41 (language): strings: String vs &str, UTF-8 facts, and the methods you will use daily.
//! LEVEL: simple
//! Use in: parsing ids/paths/log lines, building messages, validating input.   Run: cargo run --example strings
//! COMMON USES: parsing ids, paths and log lines; building messages; UTF-8-safe truncation.
//!
//! `String` owns a growable UTF-8 buffer. `&str` is a borrowed view (pointer + length) into UTF-8 bytes anywhere.
//! Take `&str` in parameters; return `String` when you build something new. Indexing by position (`s[0]`) is not
//! allowed because a character may span 1-4 bytes; use `chars()`, `char_indices()` or byte ranges on boundaries.
//! Docs: https://doc.rust-lang.org/std/primitive.str.html and https://doc.rust-lang.org/std/string/struct.String.html
#![allow(dead_code)]
use std::fmt::Write; // for write!/writeln! into a String

fn main() {
    // bytes vs chars
    let s = "héllo";
    println!(
        "len={} chars={} is_char_boundary(2)={}",
        s.len(),
        s.chars().count(),
        s.is_char_boundary(2)
    );
    println!("{:?}", s.char_indices().collect::<Vec<_>>());
    // &s[0..2] would PANIC (cuts 'é' in half). Safe: s.get(0..2) returns None.
    println!("{:?} {:?}", s.get(0..2), s.get(0..3));

    // splitting and slicing
    let line = "  task-42 : transcode ; retries=3  ";
    let line = line.trim();
    println!("{:?}", line.split_once(':'));
    println!(
        "{:?}",
        line.rsplit_once(';').map(|(a, b)| (a.trim(), b.trim()))
    );
    println!("{:?}", "a,b,,c".split(',').collect::<Vec<_>>());
    println!("{:?}", "a,b,c,d".splitn(3, ',').collect::<Vec<_>>());
    println!(
        "{:?}",
        "one two\tthree\n four"
            .split_whitespace()
            .collect::<Vec<_>>()
    );
    println!("{:?}", "l1\nl2\r\nl3".lines().collect::<Vec<_>>());
    println!(
        "{:?} {:?}",
        "job-7".strip_prefix("job-"),
        "job-7".strip_suffix(".json")
    );
    println!("{:?}", "--flag--".trim_matches('-'));

    // searching and testing
    println!("{:?} {:?}", "banana".find("an"), "banana".rfind("an"));
    println!(
        "{} {}",
        "banana".matches("an").count(),
        "Banana".eq_ignore_ascii_case("bANANA")
    );
    println!(
        "{} {}",
        "abc".starts_with(char::is_alphabetic),
        "abc".contains(['x', 'c'])
    );
    println!(
        "{:?}",
        "x=1;y=22"
            .split(';')
            .filter_map(|kv| kv.split_once('='))
            .collect::<Vec<_>>()
    );

    // transforming
    println!(
        "{} {} {}",
        "Hello".to_uppercase(),
        "Hello".to_ascii_lowercase(),
        "ab".repeat(3)
    );
    println!(
        "{} {:?}",
        "a-b-c".replace('-', "_"),
        "abc".chars().rev().collect::<String>()
    );
    println!("{:?}", "ß".to_uppercase()); // Unicode-aware: one char can become two

    // building
    let mut out = String::with_capacity(64);
    out.push_str("tasks:");
    out.push(' ');
    write!(out, "{} ready, {} blocked", 3, 1).unwrap();
    writeln!(out).unwrap();
    out += "done";
    println!("{out:?} cap>={}", out.capacity());
    println!("{}", ["a", "b", "c"].join(" -> "));
    println!(
        "{}",
        (1..=4).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
    );

    // parsing and formatting
    println!(
        "{:?} {:?} {:?}",
        "42".parse::<u32>(),
        "-1".parse::<u32>().is_err(),
        "3.5".parse::<f64>()
    );
    println!("{:?} {:?}", 'z'.to_digit(36), char::from_digit(7, 10));
    let width = 10;
    println!(
        "[{:>width$}] [{:<8}] [{:^8}] [{:+}] [{:05}] [{:.2}]",
        "id", "name", "mid", 5, 42, 3.14159
    );

    // owned <-> borrowed, retain, drain
    let mut name = String::from("flow-forge");
    name.retain(|c| c != '-');
    let tail: String = name.drain(..4).collect();
    let popped = name.pop();
    println!("{tail} | {name} | popped {popped:?}");
    let bytes = "hi".as_bytes();
    println!("{bytes:?} {:?}", String::from_utf8(vec![104, 105]));
}
