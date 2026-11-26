//! RECIPE 49 (testing): unit-test patterns: table-driven, Result-returning, should_panic, fakes, seeded randomness.
//! LEVEL: intermediate
//! Use in: every stage; unit tests live NEXT TO the code, black-box tests in `tests/`.   Run: cargo test --release --example testing_patterns
//! COMMON USES: every codebase: table-driven tests, injected fakes, seeded property tests.
#![allow(dead_code)]
//!
//! `cargo test` builds the `#[cfg(test)]` module only for tests. Good tests: one behaviour each, descriptive names,
//! failure messages that show the input, no reliance on time/network/randomness (inject fakes, seed RNGs).
//! Read: https://doc.rust-lang.org/book/ch11-00-testing.html and insta (snapshot tests) https://insta.rs
use std::error::Error;

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

pub struct Percent(u8);
impl Percent {
    pub fn new(v: u8) -> Percent {
        assert!(v <= 100, "percent out of range: {v}");
        Percent(v)
    }
}

pub fn parse_pair(s: &str) -> Result<(i32, i32), Box<dyn Error>> {
    let (a, b) = s.split_once(',').ok_or("missing comma")?;
    Ok((a.trim().parse()?, b.trim().parse()?)) // `?` converts any error into Box<dyn Error>
}

// a fake dependency injected for tests (see also token_bucket's FakeClock)
pub trait Clock {
    fn now(&self) -> u64;
}
pub fn is_expired(clock: &impl Clock, deadline: u64) -> bool {
    clock.now() >= deadline
}

fn main() {
    println!("{}", slugify("  Hello,  World! 42 "));
    println!("run the tests: cargo test --release --example testing_patterns");
}

#[cfg(test)]
mod tests {
    use super::*;

    // TABLE-DRIVEN: many cases, one loop, failure message names the input
    #[test]
    fn slugify_cases() {
        let cases = [
            ("Hello World", "hello-world"),
            ("  spaced   out  ", "spaced-out"),
            ("Already-Slug", "already-slug"),
            ("!!!", ""),
            ("", ""),
            ("a_b", "a-b"),
        ];
        for (input, want) in cases {
            assert_eq!(slugify(input), want, "input: {input:?}");
        }
    }

    /// Short strings from a small awkward alphabet. A fixed seed (xorshift): the same inputs on every run, no extra crate.
    fn awkward_strings() -> impl Iterator<Item = String> {
        let alphabet: Vec<char> = "ab Z-_!9 é".chars().collect();
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        (0..500).map(move |_| {
            (0..12)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    alphabet[(x % alphabet.len() as u64) as usize]
                })
                .collect()
        })
    }

    // PROPERTY: a second run changes nothing. Useful, but a slugify that always returns "" passes it.
    #[test]
    fn slugify_is_idempotent() {
        for s in awkward_strings() {
            let once = slugify(&s);
            assert_eq!(slugify(&once), once, "not idempotent for {s:?}");
        }
    }

    // PROPERTY: nothing is lost or invented. The ASCII letters and digits come through, in order, in lower case.
    #[test]
    fn slugify_keeps_letters_and_digits() {
        let kept = |s: &str| -> String {
            s.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect()
        };
        for s in awkward_strings() {
            assert_eq!(kept(&slugify(&s)), kept(&s), "input: {s:?}");
        }
    }

    // BOUNDARY: test the last value that is allowed, not only the first one that is not
    #[test]
    fn percent_accepts_100() {
        Percent::new(100);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn percent_rejects_over_100() {
        Percent::new(101);
    }

    // tests can return Result so you can use `?`
    #[test]
    fn parse_pair_ok() -> Result<(), Box<dyn Error>> {
        assert_eq!(parse_pair("3, 4")?, (3, 4));
        Ok(())
    }
    #[test]
    fn parse_pair_errors_are_descriptive() {
        assert!(parse_pair("34").unwrap_err().to_string().contains("comma"));
        assert!(parse_pair("a,4").is_err());
    }

    struct Fixed(u64);
    impl Clock for Fixed {
        fn now(&self) -> u64 {
            self.0
        }
    }
    #[test]
    fn expiry_uses_injected_clock() {
        assert!(!is_expired(&Fixed(9), 10));
        assert!(is_expired(&Fixed(10), 10));
    }

    // matches! for enums/Results without PartialEq
    #[test]
    fn matches_macro() {
        assert!(matches!(parse_pair("1,2"), Ok((1, 2))));
    }
}
