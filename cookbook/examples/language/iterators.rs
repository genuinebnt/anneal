//! RECIPE 12 (iterators): iterator pipelines and `collect` into Result / maps.
//! LEVEL: simple
//! Use in: everywhere; replaces index loops.   Run: cargo run --example iterators
//! COMMON USES: data pipelines: parse-validate-transform lines, group/count, windowed diffs without index loops.
//!
//! Pattern: describe the transformation (map/filter/zip/windows/fold) and let `collect` build the result.
//! `collect::<Result<Vec<_>, _>>()` stops at the FIRST error, which is great for validation.
//! Book: https://doc.rust-lang.org/book/ch13-02-iterators.html
#![allow(dead_code)]
use std::collections::BTreeMap;

fn parse_line(l: &str) -> Result<(String, u32), String> {
    let (k, v) = l.split_once('=').ok_or(format!("`{l}` has no `=`"))?;
    let v = v.parse::<u32>().map_err(|e| format!("`{l}`: {e}"))?;
    Ok((k.to_string(), v))
}

fn main() {
    let good = ["a=1", "b=2", "c=3"];
    let bad = ["a=1", "b=oops", "c"];

    let ok: Result<Vec<_>, _> = good.iter().map(|l| parse_line(l)).collect();
    println!("{ok:?}");
    let err: Result<Vec<_>, _> = bad.iter().map(|l| parse_line(l)).collect();
    println!("{err:?}  (stopped at the first failure)");

    // keep the good ones, ignore the rest
    let kept: Vec<_> = bad.iter().filter_map(|l| parse_line(l).ok()).collect();
    println!("{kept:?}");

    // count by first letter into a sorted map
    let words = ["apple", "avocado", "banana", "blueberry", "cherry"];
    let counts = words.iter().fold(BTreeMap::new(), |mut m, w| {
        *m.entry(w.chars().next().unwrap()).or_insert(0) += 1;
        m
    });
    println!("{counts:?}");

    // adjacent pairs and zip
    let xs = [1, 4, 9, 16];
    let diffs: Vec<_> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    let paired: Vec<_> = xs.iter().zip(diffs.iter()).collect();
    println!("{diffs:?} {paired:?}");
}
