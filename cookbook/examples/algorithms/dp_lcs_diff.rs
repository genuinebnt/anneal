//! RECIPE 74 (dp): longest common subsequence, and turning it into a diff between two versions.
//! LEVEL: intermediate
//! Use in: showing what changed between two ordered lists (workflow versions, config files, task lists).   Run: cargo run --release --example dp_lcs_diff
//! COMMON USES: diff tools, version control, config drift detection, collaborative editing history.
#![allow(dead_code)]
//!
//!   L[i][j] = length of the LCS of the first i items of A and first j items of B
//!   L[i][j] = L[i-1][j-1] + 1            if A[i] == B[j]
//!           = max(L[i-1][j], L[i][j-1])   otherwise
//! Backtrack from (n, m): equal items are kept; otherwise step in the direction of the larger neighbour, which means
//! "delete from A" (up) or "insert from B" (left). Items in the LCS are unchanged, everything else is a change.
//! Real-world diffs use Myers' O(ND) algorithm: https://github.com/mitsuhiko/similar and git's diff.
//! Read: https://en.wikipedia.org/wiki/Longest_common_subsequence
#[derive(Debug, PartialEq)]
enum Op<'a> {
    Keep(&'a str),
    Remove(&'a str),
    Add(&'a str),
}

fn diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Op<'a>> {
    let (n, m) = (old.len(), new.len());
    let mut l = vec![vec![0usize; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            l[i][j] = if old[i - 1] == new[j - 1] {
                l[i - 1][j - 1] + 1
            } else {
                l[i - 1][j].max(l[i][j - 1])
            };
        }
    }
    let (mut i, mut j, mut ops) = (n, m, vec![]);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            ops.push(Op::Keep(old[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || l[i][j - 1] >= l[i - 1][j]) {
            ops.push(Op::Add(new[j - 1]));
            j -= 1;
        } else {
            ops.push(Op::Remove(old[i - 1]));
            i -= 1;
        }
    }
    ops.reverse();
    ops
}

fn main() {
    let v1 = ["upload", "validate", "transcode", "publish"];
    let v2 = [
        "upload",
        "validate",
        "thumbnail",
        "transcode",
        "notify",
        "publish",
    ];
    let ops = diff(&v1, &v2);
    for op in &ops {
        match op {
            Op::Keep(s) => println!("  {s}"),
            Op::Add(s) => println!("+ {s}"),
            Op::Remove(s) => println!("- {s}"),
        }
    }
    let added = ops.iter().filter(|o| matches!(o, Op::Add(_))).count();
    let removed = ops.iter().filter(|o| matches!(o, Op::Remove(_))).count();
    assert_eq!((added, removed), (2, 0));

    // invariant: applying the diff to `old` reproduces `new`, and keeps+removes reproduce `old`
    let rebuilt: Vec<&str> = ops
        .iter()
        .filter_map(|o| match o {
            Op::Keep(s) | Op::Add(s) => Some(*s),
            Op::Remove(_) => None,
        })
        .collect();
    assert_eq!(rebuilt, v2);
    let a = ["a", "b", "c", "d"];
    let b = ["b", "x", "d", "a"];
    let ops = diff(&a, &b);
    let old_again: Vec<&str> = ops
        .iter()
        .filter_map(|o| match o {
            Op::Keep(s) | Op::Remove(s) => Some(*s),
            Op::Add(_) => None,
        })
        .collect();
    assert_eq!(old_again, a);
    println!("{ops:?}");
}
