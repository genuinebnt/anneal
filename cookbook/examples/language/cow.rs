//! RECIPE 10 (language): `Cow`: borrow when you can, allocate only when you must.
//! LEVEL: simple
//! Use in: id normalisation, escaping, anywhere most inputs need no change.   Run: cargo run --example cow
//! COMMON USES: normalising ids/paths/HTML escaping: return borrowed data when no change is needed, allocate only on change.
//!
//! Pattern: return `Cow<'_, str>`. Clean inputs come back as a borrow (zero allocation); only dirty inputs
//! allocate a new `String`. Callers treat both as `&str`.
//! Docs: https://doc.rust-lang.org/std/borrow/enum.Cow.html
#![allow(dead_code)]
use std::borrow::Cow;

fn normalize(id: &str) -> Cow<'_, str> {
    if id.chars().any(|c| c.is_uppercase() || c == ' ') {
        Cow::Owned(id.to_lowercase().replace(' ', "-"))
    } else {
        Cow::Borrowed(id)
    }
}

fn main() {
    for id in ["upload", "Upload Video", "thumbnail-small"] {
        let n = normalize(id);
        let kind = if matches!(n, Cow::Borrowed(_)) {
            "borrowed (no alloc)"
        } else {
            "owned (allocated)"
        };
        println!("{id:>16} -> {n:<16} {kind}");
    }
}
