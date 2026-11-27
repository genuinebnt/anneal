//! RECIPE 21 (testing): property testing with SHRINKING (what quickcheck/proptest do for you).
//! LEVEL: advanced
//! Use in: testing invariants (stage 7 onwards).   Run: cargo run --example shrinking
//! COMMON USES: finding minimal failing inputs in any property test (proptest/quickcheck do this automatically).
//!
//! Pattern: generate random input until the property fails, then greedily simplify the failing input
//! (remove elements, shrink values) while it STILL fails. You get a tiny counterexample instead of a
//! 40-element wall of noise. In real projects use the crates: https://github.com/BurntSushi/quickcheck
//! and https://github.com/proptest-rs/proptest
//! LIMITS: greedy shrinking finds a LOCAL minimum (no single removal or halving still fails), which in general need not be
//! the smallest failing input. Here two equal values stay as they are: halving only one of
//! them makes them differ, and the test passes. A property that panics instead of returning false would end this program at once.
#![allow(dead_code)]
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Supposedly sorts. Bug: `dedup` also drops duplicates.
fn buggy_sort(v: &[i32]) -> Vec<i32> {
    let mut out = v.to_vec();
    out.sort();
    out.dedup();
    out
}

/// Property: sorting never changes the number of elements.
fn holds(v: &[i32]) -> bool {
    buggy_sort(v).len() == v.len()
}

fn shrink(mut v: Vec<i32>) -> Vec<i32> {
    loop {
        let mut progressed = false;
        for i in 0..v.len() {
            let mut c = v.clone();
            c.remove(i);
            if !holds(&c) {
                v = c;
                progressed = true;
                break;
            }
        }
        if !progressed {
            for i in 0..v.len() {
                if v[i] != 0 {
                    let mut c = v.clone();
                    c[i] /= 2;
                    if !holds(&c) {
                        v = c;
                        progressed = true;
                        break;
                    }
                }
            }
        }
        if !progressed {
            return v;
        }
    }
}

fn main() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for case in 0..1000 {
        let len = (rng.next() % 40) as usize;
        let v: Vec<i32> = (0..len).map(|_| (rng.next() % 11) as i32 - 5).collect();
        if !holds(&v) {
            let small = shrink(v.clone());
            println!("case {case}: failing input had {} elements: {v:?}", v.len());
            println!("shrunk to {} elements: {small:?}", small.len());
            // it must still FAIL, with nothing left to remove: one value twice (not always zero, see LIMITS)
            assert!(!holds(&small), "the shrinker lost the failure");
            assert!(small.len() == 2 && small[0] == small[1], "{small:?}");
            return;
        }
    }
    panic!("no counterexample found, although the sort is broken");
}
