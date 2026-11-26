//! RECIPE 71 (dp): 0/1 knapsack: pick items under a capacity to maximise value, and recover WHICH items.
//! LEVEL: intermediate
//! Use in: choosing a best subset under a budget (memory, slots, time): the core of resource-constrained packing.   Run: cargo run --release --example dp_knapsack
//! COMMON USES: resource-constrained selection: ad budgets, cargo loading, feature selection under a time budget.
#![allow(dead_code)]
//!
//! DP recipe: (1) define the state, (2) write the recurrence, (3) fix the fill order, (4) read the answer, (5) backtrack.
//!   best[i][c] = best value using only the first i items with capacity c
//!   best[i][c] = max( best[i-1][c]                       // skip item i
//!                   , best[i-1][c - w_i] + v_i  if w_i <= c )   // take item i
//! O(items * capacity) time. The 2-D table lets us backtrack; if you only need the VALUE, one row suffices
//! (iterate capacity downwards). Greedy-by-ratio is NOT optimal here (see the assert).
//! Read: https://en.wikipedia.org/wiki/Knapsack_problem and https://cp-algorithms.com/dynamic_programming/knapsack.html
struct Item {
    name: &'static str,
    weight: usize,
    value: u32,
}

fn knapsack(items: &[Item], cap: usize) -> (u32, Vec<&'static str>) {
    let n = items.len();
    let mut best = vec![vec![0u32; cap + 1]; n + 1];
    for i in 1..=n {
        let it = &items[i - 1];
        for c in 0..=cap {
            best[i][c] = best[i - 1][c];
            if it.weight <= c {
                best[i][c] = best[i][c].max(best[i - 1][c - it.weight] + it.value);
            }
        }
    }
    // backtrack: an item was taken iff the value changed when it became available
    let (mut c, mut taken) = (cap, vec![]);
    for i in (1..=n).rev() {
        if best[i][c] != best[i - 1][c] {
            taken.push(items[i - 1].name);
            c -= items[i - 1].weight;
        }
    }
    taken.reverse();
    (best[n][cap], taken)
}

fn main() {
    let items = [
        Item {
            name: "transcode",
            weight: 5,
            value: 10,
        },
        Item {
            name: "thumbnail",
            weight: 4,
            value: 40,
        },
        Item {
            name: "subtitles",
            weight: 6,
            value: 30,
        },
        Item {
            name: "metadata",
            weight: 3,
            value: 50,
        },
    ];
    let (value, taken) = knapsack(&items, 10);
    println!("capacity 10 -> value {value}, take {taken:?}");
    assert_eq!(value, 90);

    // Greedy by value/weight ratio can be wrong. Ratios: a=15, b=10, c=10, d=9. Greedy takes a (w1), then b (w5): 65,
    // and nothing else fits in 9. The DP finds b+c (w9) = 90.
    let tricky = [
        Item {
            name: "a",
            weight: 1,
            value: 15,
        },
        Item {
            name: "b",
            weight: 5,
            value: 50,
        },
        Item {
            name: "c",
            weight: 4,
            value: 40,
        },
        Item {
            name: "d",
            weight: 5,
            value: 45,
        },
    ];
    let (v, t) = knapsack(&tricky, 9);
    println!("capacity 9 -> value {v}, take {t:?}");
    assert_eq!(v, 90);
}
