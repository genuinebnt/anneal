//! RECIPE 45 (iterators): a tour of adapters and consumers, plus writing your own `Iterator`.
//! LEVEL: intermediate
//! Use in: replacing index loops, streaming over large inputs lazily, readable data pipelines.   Run: cargo run --example iterator_adapters
//! COMMON USES: readable data pipelines, streaming lazily over large inputs, writing your own iterator types.
#![allow(dead_code)]
//!
//! Iterators are LAZY: adapters (`map`, `filter`...) do nothing until a consumer (`collect`, `sum`, `for`, `next`)
//! pulls items. The compiler usually optimises chains to the same machine code as a hand-written loop.
//! Read: https://doc.rust-lang.org/std/iter/trait.Iterator.html and https://doc.rust-lang.org/book/ch13-02-iterators.html
//! Crate for more adapters: https://docs.rs/itertools
use std::collections::HashMap;
use std::iter;

// Writing your own iterator: implement `next`, get ~70 adapter methods free.
struct Fib {
    a: u64,
    b: u64,
}
impl Iterator for Fib {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let r = self.a;
        self.a = self.b;
        self.b += r;
        Some(r)
    }
}

fn main() {
    println!(
        "{:?}",
        Fib { a: 0, b: 1 }
            .skip(5)
            .step_by(2)
            .take(5)
            .collect::<Vec<_>>()
    );
    println!(
        "{:?}",
        Fib { a: 0, b: 1 }
            .take_while(|&x| x < 50)
            .filter(|x| x % 2 == 0)
            .collect::<Vec<_>>()
    );

    // peekable: look ahead without consuming (run-length grouping of log levels)
    let levels = ["INFO", "INFO", "WARN", "INFO", "INFO", "INFO"];
    let mut it = levels.iter().peekable();
    let mut runs = vec![];
    while let Some(&cur) = it.next() {
        let mut n = 1;
        while it.peek() == Some(&&cur) {
            it.next();
            n += 1;
        }
        runs.push((cur, n));
    }
    println!("{runs:?}");

    // scan: running state (prefix sums); fold: collapse to one value; try_fold: early exit on failure
    println!(
        "{:?}",
        [1, 2, 3, 4]
            .iter()
            .scan(0, |acc, &x| {
                *acc += x;
                Some(*acc)
            })
            .collect::<Vec<_>>()
    );
    println!("{}", (1..=5).fold(String::new(), |s, n| format!("{s}{n}")));
    let checked = [100u8, 100, 100]
        .iter()
        .try_fold(0u8, |acc, &x| acc.checked_add(x));
    println!("{checked:?}"); // None: overflowed, stopped early

    // splitting streams
    let (evens, odds): (Vec<i32>, Vec<i32>) = (1..=6).partition(|x| x % 2 == 0);
    let (names, ages): (Vec<&str>, Vec<u32>) = [("a", 1), ("b", 2)].into_iter().unzip();
    println!("{evens:?} {odds:?} {names:?} {ages:?}");

    // combining
    println!(
        "{:?}",
        ["a", "b"]
            .iter()
            .zip(1..)
            .map(|(s, i)| format!("{s}{i}"))
            .collect::<Vec<_>>()
    );
    println!(
        "{:?}",
        [1, 2].iter().chain([3].iter()).rev().collect::<Vec<_>>()
    );
    println!(
        "{:?}",
        vec![vec![1, 2], vec![], vec![3]]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    );
    println!(
        "{:?}",
        ["ab", "cd"]
            .iter()
            .flat_map(|s| s.chars())
            .collect::<String>()
    );
    println!("{:?}", [1, 2, 3].iter().cycle().take(7).collect::<Vec<_>>());

    // finding and aggregating
    let words = ["apple", "fig", "banana", "kiwi"];
    println!(
        "{:?} {:?}",
        words.iter().max_by_key(|w| w.len()),
        words
            .iter()
            .min_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)))
    );
    println!(
        "{:?} {:?} {:?}",
        words.iter().position(|&w| w == "fig"),
        words.iter().rposition(|w| w.len() == 4),
        words.iter().nth(2)
    );
    println!(
        "{} {} {}",
        words.iter().any(|w| w.starts_with('k')),
        words.iter().all(|w| w.len() > 2),
        words.iter().map(|w| w.len()).sum::<usize>()
    );
    println!(
        "{:?}",
        words
            .iter()
            .map(|w| (w.len(), *w))
            .collect::<std::collections::BTreeMap<_, _>>()
    );
    let counts =
        "hello world"
            .chars()
            .filter(|c| c.is_alphabetic())
            .fold(HashMap::new(), |mut m, c| {
                *m.entry(c).or_insert(0) += 1;
                m
            });
    let mut top: Vec<_> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("{:?}", &top[..3]);

    // generators without a struct
    let mut n = 0;
    let counter = iter::from_fn(|| {
        n += 1;
        (n <= 3).then_some(n)
    });
    println!("{:?}", counter.collect::<Vec<_>>());
    println!(
        "{:?}",
        iter::successors(Some(1u32), |&x| (x < 200).then_some(x * 3)).collect::<Vec<_>>()
    );
    println!("{:?}", iter::repeat("ab").take(3).collect::<String>());

    // by_ref: use part of an iterator then keep going; inspect: debug a pipeline
    let mut nums = 1..10;
    let first3: Vec<_> = nums.by_ref().take(3).collect();
    let rest_sum: i32 = nums.sum();
    println!("{first3:?} {rest_sum}");
    let v: Vec<_> = [1, 2, 3]
        .iter()
        .inspect(|x| print!("[saw {x}]"))
        .map(|x| x * 2)
        .collect();
    println!(" {v:?}");

    // collect into Result / Option / String / map
    let r: Result<Vec<i32>, _> = ["1", "2", "x"].iter().map(|s| s.parse::<i32>()).collect();
    let o: Option<Vec<i32>> = [Some(1), Some(2)].into_iter().collect();
    println!("{} {o:?}", r.is_err());
}
