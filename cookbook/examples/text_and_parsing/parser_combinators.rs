//! RECIPE 64 (parsing): hand-written parser combinators: small functions that compose into a parser.
//! LEVEL: intermediate
//! Use in: config/DSL fragments, durations ("1h30m"), ids, anywhere regex is too weak and a full grammar is too heavy.   Run: cargo run --release --example parser_combinators
//! COMMON USES: durations/sizes/versions in configs, small DSL fragments, anything regex handles badly.
#![allow(dead_code)]
//!
//! Idea: a parser is `fn(&str) -> Option<(value, rest_of_input)>`. Tiny parsers (digits, a unit) combine with
//! sequencing (`?` chains) and repetition (`many1`) into bigger ones. Errors: `None` = "does not match here".
//! This is exactly the model of the `nom` crate https://github.com/rust-bakery/nom and `winnow`
//! https://github.com/winnow-rs/winnow  (read their docs after writing your own once).
type PResult<'a, T> = Option<(T, &'a str)>;

fn digits(i: &str) -> PResult<'_, u64> {
    let end = i.find(|c: char| !c.is_ascii_digit()).unwrap_or(i.len());
    if end == 0 {
        return None;
    }
    Some((i[..end].parse().ok()?, &i[end..]))
}

/// unit -> multiplier in milliseconds. Order matters: try "ms" before "m".
fn unit(i: &str) -> PResult<'_, u64> {
    for (name, mult) in [("ms", 1u64), ("h", 3_600_000), ("m", 60_000), ("s", 1000)] {
        if let Some(rest) = i.strip_prefix(name) {
            return Some((mult, rest));
        }
    }
    None
}

fn term(i: &str) -> PResult<'_, u64> {
    let (n, i) = digits(i)?;
    let (mult, i) = unit(i)?;
    Some((n.checked_mul(mult)?, i))
}

/// apply `p` one or more times
fn many1<'a, T>(p: impl Fn(&'a str) -> PResult<'a, T>) -> impl Fn(&'a str) -> PResult<'a, Vec<T>> {
    move |mut i| {
        let mut out = vec![];
        while let Some((v, rest)) = p(i) {
            out.push(v);
            i = rest;
        }
        if out.is_empty() { None } else { Some((out, i)) }
    }
}

fn duration_ms(s: &str) -> Result<u64, String> {
    match many1(term)(s) {
        Some((terms, "")) => Ok(terms.iter().sum()),
        Some((_, rest)) => Err(format!("unexpected `{rest}`")),
        None => Err(format!("expected a duration like 1h30m, got `{s}`")),
    }
}

fn main() {
    for s in [
        "250ms",
        "15s",
        "1h30m",
        "2m30s500ms",
        "1h30",
        "abc",
        "",
        "5x",
        "99999999999999999h",
    ] {
        println!("{s:>20} -> {:?}", duration_ms(s));
    }
    assert_eq!(duration_ms("1h30m"), Ok(5_400_000));
    assert_eq!(duration_ms("2m30s500ms"), Ok(150_500));
    assert!(duration_ms("1h30").is_err());
}
