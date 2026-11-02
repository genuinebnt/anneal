use solution::*;

#[test]
fn strings_and_floats() {
    check!(r#"non-integer Copy types"#, { let mut s = vec!["pear", "apple", "fig"]; sort_by_c(&mut s, |a, b| a.cmp(b)); let mut f = vec![2.5f64, -1.0, 0.0]; sort_by_c(&mut f, |a, b| a.partial_cmp(b).unwrap()); (s, f) }, (vec!["apple", "fig", "pear"], vec![-1.0, 0.0, 2.5]));
}

#[test]
fn large_structs_use_the_right_element_size() {
    check!(r#"a 40-byte element"#, { #[derive(Clone, Copy, Debug, PartialEq)] struct Big { key: u32, pad: [u64; 4] } let mut v: Vec<Big> = (0..50u32).rev().map(|k| Big { key: k, pad: [k as u64; 4] }).collect(); sort_by_c(&mut v, |a, b| a.key.cmp(&b.key)); v.iter().enumerate().all(|(i, b)| b.key == i as u32 && b.pad == [i as u64; 4]) }, true);
}

#[test]
fn already_sorted_and_reversed() {
    check!(r#"common inputs"#, { let mut a: Vec<u32> = (0..500).collect(); let mut b: Vec<u32> = (0..500).rev().collect(); sort_by_c(&mut a, |x, y| x.cmp(y)); sort_by_c(&mut b, |x, y| x.cmp(y)); (a == (0..500).collect::<Vec<u32>>(), b == (0..500).collect::<Vec<u32>>()) }, (true, true));
}

#[test]
fn all_equal() {
    check!(r#"no order to impose"#, { let mut v = vec![7u8; 100]; sort_by_c(&mut v, |a, b| a.cmp(b)); v == vec![7u8; 100] }, true);
}

#[test]
fn zero_sized_elements() {
    check!(r#"size 0"#, { let mut v = vec![(); 10]; sort_by_c(&mut v, |_, _| std::cmp::Ordering::Equal); v.len() }, 10);
}

#[test]
fn matches_the_standard_sort_on_random_data() {
    let mut rng = anneal_prelude::Rng::new(31337);
    for round in 0..200 {
        let n = rng.below(300);
        let v: Vec<u32> = (0..n).map(|_| rng.below(1000) as u32).collect();
        let mut mine = v.clone();
        let mut std_sorted = v;
        sort_by_c(&mut mine, |a, b| a.cmp(b));
        std_sorted.sort();
        check!(format!("round {round}"), mine, std_sorted);
    }
}

#[test]
fn a_panic_reaches_the_caller_with_its_payload() {
    let mut v: Vec<i32> = (0..50).rev().collect();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sort_by_c(&mut v, |a, b| {
            if *a == 25 || *b == 25 {
                panic!("boom at 25");
            }
            a.cmp(b)
        });
    }));
    let err = r.expect_err("the panic must propagate out of sort_by_c");
    let msg = err.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| err.downcast_ref::<String>().cloned());
    check!("same payload", msg, Some("boom at 25".to_string()));
}

#[test]
fn the_closure_is_not_called_again_after_it_panicked() {
    use std::cell::Cell;
    let calls = Cell::new(0usize);
    let mut v: Vec<i32> = (0..200).rev().collect();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sort_by_c(&mut v, |a, b| {
            calls.set(calls.get() + 1);
            if calls.get() == 5 {
                panic!("fifth call");
            }
            a.cmp(b)
        });
    }));
    check!("it panicked", r.is_err(), true);
    check!("exactly five calls: nothing runs after the panic", calls.get(), 5);
}

#[test]
fn the_slice_stays_a_permutation_after_a_panic() {
    let mut v: Vec<i32> = (0..100).map(|i| (i * 37) % 101).collect();
    let mut before = v.clone();
    let mut n = 0;
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sort_by_c(&mut v, |a, b| {
            n += 1;
            if n == 40 {
                panic!("stop");
            }
            a.cmp(b)
        });
    }));
    before.sort();
    let mut after = v.clone();
    after.sort();
    check!("nothing lost or duplicated", after, before);
}
