//! RECIPE 35 (language): From/Into, TryFrom, AsRef, Borrow, ToString, `as`: choosing the right conversion.
//! LEVEL: simple
//! Use in: ergonomic APIs, error conversion (`?`), accepting &str/String/Path flexibly.   Run: cargo run --example conversions
//! COMMON USES: ergonomic APIs that accept &str/String/Path, error conversion with `?`, validated construction.
//!
//! Rules of thumb:
//!   * infallible conversion you own  -> implement `From<A> for B` (you get `Into` for free)
//!   * can fail                        -> `TryFrom` / `FromStr`
//!   * accept "anything string-like"   -> `impl AsRef<str>` (borrow) or `impl Into<String>` (take ownership)
//!   * `as` casts silently truncate    -> prefer `try_from` when the value comes from outside
//! Read: https://doc.rust-lang.org/std/convert/index.html
#![allow(dead_code)]
use std::borrow::Borrow;
use std::collections::HashMap;
use std::path::Path;

struct Task {
    name: String,
}
impl Task {
    // Into<String>: caller may pass &str OR String; String moves in without a clone.
    fn new(name: impl Into<String>) -> Self {
        Task { name: name.into() }
    }
}

// AsRef<str>: we only READ, so we borrow whatever string-like thing we got.
fn shout(s: impl AsRef<str>) -> String {
    s.as_ref().to_uppercase()
}
// AsRef<Path>: accept &str, String, PathBuf, &Path...
fn extension(p: impl AsRef<Path>) -> Option<String> {
    p.as_ref()
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
}

// TryFrom: validated construction
#[derive(Debug)]
struct Percent(u8);
impl TryFrom<i64> for Percent {
    type Error = String;
    fn try_from(v: i64) -> Result<Self, String> {
        if (0..=100).contains(&v) {
            Ok(Percent(v as u8))
        } else {
            Err(format!("{v} is not a percentage"))
        }
    }
}

// From for error conversion (this is what `?` calls)
#[derive(Debug)]
enum MyErr {
    Parse(std::num::ParseIntError),
}
impl From<std::num::ParseIntError> for MyErr {
    fn from(e: std::num::ParseIntError) -> Self {
        MyErr::Parse(e)
    }
}
fn parse_double(s: &str) -> Result<i32, MyErr> {
    Ok(s.parse::<i32>()? * 2) // `?` converts ParseIntError -> MyErr via From
}

fn main() {
    let a = Task::new("literal");
    let b = Task::new(String::from("owned"));
    println!("{} {}", a.name, b.name);
    println!("{} {}", shout("abc"), shout(String::from("xyz")));
    println!(
        "{:?} {:?}",
        extension("video.mp4"),
        extension(std::path::PathBuf::from("noext"))
    );
    println!("{:?} {:?}", Percent::try_from(50), Percent::try_from(500));
    let p: Result<Percent, _> = 7i64.try_into(); // TryInto comes free
    println!("{p:?} {:?} {:?}", parse_double("21"), parse_double("x"));

    // Borrow lets a HashMap<String, _> be queried with &str (no allocation to look up)
    let m: HashMap<String, u32> = HashMap::from([("a".to_string(), 1)]);
    println!("{:?}", m.get("a"));
    fn takes_borrow<K: Borrow<str>>(k: K) -> usize {
        k.borrow().len()
    }
    println!("{}", takes_borrow("four"));

    // `as` pitfalls
    let big: u64 = 5_000_000_000;
    println!(
        "as u32 = {} (silent truncation!), try_from = {:?}",
        big as u32,
        u32::try_from(big)
    );
    println!("{} {} {}", -1i32 as u32, 300i32 as u8, 3.99f64 as i32);
    println!("{} {}", 42.to_string(), format!("{:?}", "quoted"));
}
