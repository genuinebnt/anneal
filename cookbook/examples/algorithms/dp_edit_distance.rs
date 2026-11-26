//! RECIPE 72 (dp): edit distance (Levenshtein) and "did you mean ...?" suggestions.
//! LEVEL: intermediate
//! Use in: friendly error messages for unknown names/commands/ids, fuzzy matching, spell-check, dedup.   Run: cargo run --release --example dp_edit_distance
//! COMMON USES: "did you mean ...?" suggestions in CLIs/compilers, fuzzy matching, spell checking, DNA alignment.
#![allow(dead_code)]
//!
//!   d[i][j] = minimum edits to turn the first i chars of A into the first j chars of B
//!   d[i][j] = min( d[i-1][j] + 1            // delete a char of A
//!                , d[i][j-1] + 1            // insert a char
//!                , d[i-1][j-1] + (A[i]!=B[j]) )   // substitute (free if equal)
//! Only the previous row is needed, so memory is O(len B). Work on `chars`, not bytes, for correct Unicode.
//! Real use: cargo/clap suggestions use Jaro-Winkler/Levenshtein-style similarity; see https://github.com/rapidfuzz/strsim-rs
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect(); // distance from empty A prefix
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// closest candidate if it is "close enough" (at most a third of the length, at least 1 edit allowed)
fn suggest<'a>(typo: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let limit = (typo.chars().count() / 3).max(1);
    candidates
        .iter()
        .map(|c| (edit_distance(typo, c), *c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, c)| (*d, *c)) // tie-break alphabetically: deterministic
        .map(|(_, c)| c)
}

fn main() {
    for (a, b) in [
        ("kitten", "sitting"),
        ("flaw", "lawn"),
        ("", "abc"),
        ("same", "same"),
        ("héllo", "hello"),
    ] {
        println!("{a:>8} -> {b:<8} = {}", edit_distance(a, b));
    }
    assert_eq!(edit_distance("kitten", "sitting"), 3);

    let tasks = ["upload", "validate", "transcode", "thumbnail", "publish"];
    for typo in ["transcod", "validte", "publsh", "xyz", "uplaod"] {
        match suggest(typo, &tasks) {
            Some(s) => println!("unknown task `{typo}`: did you mean `{s}`?"),
            None => println!("unknown task `{typo}`"),
        }
    }
    assert_eq!(suggest("transcod", &tasks), Some("transcode"));
    assert_eq!(suggest("xyz", &tasks), None);
    // metric properties make good property tests: symmetry, identity, triangle inequality
    for (a, b, c) in [("abc", "abd", "xbd"), ("", "q", "qq")] {
        assert_eq!(edit_distance(a, b), edit_distance(b, a));
        assert!(edit_distance(a, c) <= edit_distance(a, b) + edit_distance(b, c));
    }
}
