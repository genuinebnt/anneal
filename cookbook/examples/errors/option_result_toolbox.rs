//! RECIPE 63 (errors): the Option/Result combinator toolbox (and when to just use `match` or `?`).
//! LEVEL: simple
//! Use in: every function that can fail or return nothing.   Run: cargo run --release --example option_result_toolbox
//! COMMON USES: all fallible code: the right combinator or `?` instead of unwrap.
#![allow(dead_code)]
//!
//! Prefer `?` for propagation, `match`/`if let`/`let else` for branching, and combinators for small transformations.
//! `unwrap()`/`expect()` are for bugs (impossible states, tests), NEVER for user input; `expect("why this cannot fail")`
//! documents the invariant. Cheat sheet: https://doc.rust-lang.org/std/option/enum.Option.html and .../result/enum.Result.html
use std::collections::HashMap;

fn first_even_square(xs: &[i32]) -> Option<i32> {
    let x = xs.iter().find(|x| *x % 2 == 0)?; // `?` works on Option too
    Some(x * x)
}

fn parse_age(s: &str) -> Result<u8, String> {
    s.trim()
        .parse::<u8>()
        .map_err(|e| format!("bad age `{s}`: {e}"))
}

fn main() {
    // Option
    let name: Option<&str> = Some("flow");
    println!(
        "{:?} {:?} {:?}",
        name.map(str::len),
        name.filter(|n| n.len() > 10),
        name.and_then(|n| n.chars().next())
    );
    println!(
        "{} {} {:?}",
        name.unwrap_or("anon"),
        None::<&str>.unwrap_or_default().len(),
        None::<i32>.or(Some(9))
    );
    println!(
        "{:?} {:?}",
        Some(1).zip(Some("a")),
        Some(5).xor(None::<i32>)
    );
    println!(
        "{:?} {:?}",
        name.ok_or("missing"),
        None::<i32>.ok_or_else(|| format!("computed {}", 1 + 1))
    );
    println!(
        "{} {}",
        name.is_some_and(|n| n.starts_with('f')),
        name.map_or(0, |n| n.len())
    );
    let mut slot = Some(3);
    let taken = slot.take(); // moves the value out, leaves None
    let old = slot.replace(8);
    println!("{taken:?} {old:?} {slot:?}");
    let filled = *slot.get_or_insert(1); // only inserts if None; returns &mut T
    println!("{filled}");
    let owned: Option<String> = Some("text".into());
    let borrowed: Option<&str> = owned.as_deref(); // Option<String> -> Option<&str> without moving
    println!("{borrowed:?} {:?}", owned.as_ref().map(|s| s.len()));

    // Result
    println!("{:?} {:?}", parse_age("42"), parse_age("-1"));
    println!(
        "{:?}",
        parse_age("7").map(|a| a * 2).and_then(|d| if d > 10 {
            Ok(d)
        } else {
            Err("too small".to_string())
        })
    );
    println!(
        "{} {}",
        parse_age("x").unwrap_or(0),
        parse_age("x").unwrap_or_else(|e| e.len() as u8)
    );
    println!(
        "{:?} {:?}",
        parse_age("5").ok(),
        parse_age("x").err().map(|e| e.len())
    );
    println!("{:?}", parse_age("x").or_else(|_| parse_age("9")));
    println!(
        "{}",
        parse_age("300").is_err_and(|e| e.contains("too large"))
    );

    // switching containers
    let a: Option<Result<u8, String>> = Some(parse_age("5"));
    let b: Result<Option<u8>, String> = a.transpose();
    println!("{b:?}");
    println!("{:?}", Some(Some(3)).flatten());
    println!(
        "{:?}",
        ["1", "2", "x"]
            .iter()
            .map(|s| parse_age(s))
            .collect::<Result<Vec<_>, _>>()
    );
    println!(
        "{:?}",
        ["1", "2"].iter().try_for_each(|s| parse_age(s).map(|_| ()))
    );
    println!(
        "{:?}",
        ["1", "z", "3"]
            .iter()
            .flat_map(|s| parse_age(s))
            .collect::<Vec<_>>()
    ); // Result is an iterator: Err skipped

    // maps return Option
    let m: HashMap<&str, i32> = [("a", 1)].into();
    println!(
        "{:?} {} {:?}",
        m.get("a").copied(),
        m.get("zz").copied().unwrap_or(-1),
        first_even_square(&[3, 4, 5])
    );
    println!("{:?}", first_even_square(&[1, 3]));
}
