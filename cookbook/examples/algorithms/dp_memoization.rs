//! RECIPE 75 (dp): memoization vs tabulation on coin change, and why greedy is not enough.
//! LEVEL: simple
//! Use in: the first DP you write; choosing between recursion+cache and a table.   Run: cargo run --release --example dp_memoization
//! COMMON USES: the first DP everyone writes: change-making, resource planning; memo vs table trade-offs.
#![allow(dead_code)]
//!
//! Problem: fewest coins summing to `amount`.  min_coins(a) = 1 + min over coins c <= a of min_coins(a - c).
//! Overlapping subproblems (the same a-c recur) + optimal substructure = dynamic programming.
//!   top-down: natural recursion, cache results in a map (`HashMap` or `Vec<Option<_>>`); recursion depth = amount!
//!   bottom-up: fill a Vec from 0 upwards; no recursion, usually faster, easy to reason about.
//! Greedy (always take the biggest coin) works for some coin systems but fails for others, so you need the DP.
//! Read: https://www.geeksforgeeks.org/coin-change-dp-7/ (any intro) and https://cp-algorithms.com/dynamic_programming/intro-to-dp.html
use std::collections::HashMap;

fn min_coins_top_down(a: u32, coins: &[u32], memo: &mut HashMap<u32, Option<u32>>) -> Option<u32> {
    if a == 0 {
        return Some(0);
    }
    if let Some(&cached) = memo.get(&a) {
        return cached;
    }
    let best = coins
        .iter()
        .filter(|&&c| c <= a)
        .filter_map(|&c| min_coins_top_down(a - c, coins, memo).map(|n| n + 1))
        .min();
    memo.insert(a, best);
    best
}

fn min_coins_bottom_up(amount: u32, coins: &[u32]) -> Option<u32> {
    let mut best: Vec<Option<u32>> = vec![None; amount as usize + 1];
    best[0] = Some(0);
    for a in 1..=amount as usize {
        best[a] = coins
            .iter()
            .filter(|&&c| c as usize <= a)
            .filter_map(|&c| best[a - c as usize].map(|n| n + 1))
            .min();
    }
    best[amount as usize]
}

fn greedy(mut amount: u32, coins: &[u32]) -> u32 {
    let mut sorted = coins.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let mut n = 0;
    for c in sorted {
        n += amount / c;
        amount %= c;
    }
    n
}

fn main() {
    let us = [1, 5, 10, 25];
    println!(
        "63 cents: top-down {:?}, bottom-up {:?}",
        min_coins_top_down(63, &us, &mut HashMap::new()),
        min_coins_bottom_up(63, &us)
    );

    // both strategies agree everywhere (an invariant test)
    for a in 0..=300 {
        assert_eq!(
            min_coins_top_down(a, &us, &mut HashMap::new()),
            min_coins_bottom_up(a, &us),
            "amount {a}"
        );
    }
    // greedy happens to be optimal for US coins...
    assert!((0..=300).all(|a| Some(greedy(a, &us)) == min_coins_bottom_up(a, &us)));
    // ...but not for [1,3,4]: amount 6 is 3+3 (2 coins) but greedy gives 4+1+1 (3 coins)
    let odd = [1, 3, 4];
    println!(
        "amount 6 with {odd:?}: dp={:?} greedy={}",
        min_coins_bottom_up(6, &odd),
        greedy(6, &odd)
    );
    assert_eq!(min_coins_bottom_up(6, &odd), Some(2));
    assert_eq!(greedy(6, &odd), 3);
    println!("impossible amount: {:?}", min_coins_bottom_up(7, &[2, 4]));

    // recursion depth risk: top-down needs `amount` stack frames when coin 1 exists. Bottom-up never recurses.
    println!(
        "bottom-up amount 1,000,000: {:?}",
        min_coins_bottom_up(1_000_000, &us)
    );
}
