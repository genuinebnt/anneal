use solution::{ensure, map, max_of};

/// Nothing here imports `HashMap`, `Failure` or anything else from the library.
mod bare {
    pub fn pairs() -> std::collections::HashMap<u8, char> {
        solution::map!(1 => 'x', 2 => 'y')
    }
}

/// A caller with their own types of the same names.
mod namesakes {
    #[allow(dead_code)]
    pub struct HashMap;
    #[allow(dead_code)]
    pub struct Failure;
    #[allow(dead_code, non_snake_case)]
    pub fn Err(_: u8) -> u8 {
        0
    }
    pub type Result<T> = std::result::Result<T, solution::Failure>;

    pub fn positive(n: i32) -> Result<i32> {
        solution::ensure!(n > 0, "{n} is not positive",);
        Ok(n)
    }

    pub fn build() -> usize {
        solution::map!("k" => 1).len()
    }
}

#[test]
fn map_works_without_imports() {
    let mut keys: Vec<u8> = bare::pairs().into_keys().collect();
    keys.sort();
    check!("map! in a module with no imports", keys, vec![1, 2]);
}

#[test]
fn namesakes_do_not_confuse_the_macros() {
    check!("positive(3)", namesakes::positive(3).ok(), Some(3));
    check!("positive(-1)", namesakes::positive(-1).map_err(|e| e.0), std::result::Result::Err("-1 is not positive".to_string()));
    check!("map! beside a local HashMap", namesakes::build(), 1);
}

#[test]
fn empty_map() {
    let m: std::collections::HashMap<u8, u8> = map!();
    check!("map!()", m.len(), 0);
}

#[test]
fn repeated_key_keeps_the_last() {
    let m = map!(1 => "first", 1 => "second");
    check!("two entries for key 1", (m.len(), m[&1]), (1, "second"));
}

#[test]
fn map_evaluates_in_written_order() {
    let mut log = Vec::new();
    let mut note = |x: u8| {
        log.push(x);
        x
    };
    let _m = map!(note(1) => note(2), note(3) => note(4));
    check!("order of evaluation", log, vec![1, 2, 3, 4]);
}

#[test]
fn max_of_order_of_evaluation() {
    let mut log = Vec::new();
    let mut note = |x: u8| {
        log.push(x);
        x
    };
    let got = max_of!(note(5), note(9), note(7), note(1));
    check!("max and order for 5, 9, 7, 1", (got, log), (9, vec![5, 9, 7, 1]));
}

#[test]
fn max_of_non_copy() {
    let a = String::from("pear");
    let got = max_of!(a, String::from("apple"), String::from("zoo"));
    check!("three Strings", got, "zoo".to_string());
}

/// Ordered by `key` only, so a test can tell equal values apart.
#[derive(Debug)]
struct Tagged(u8, char);
impl PartialEq for Tagged {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}
impl PartialOrd for Tagged {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&o.0)
    }
}

#[test]
fn first_of_equal_values_wins() {
    check!("keys 2, 7, 7, 3", max_of!(Tagged(2, 'a'), Tagged(7, 'b'), Tagged(7, 'c'), Tagged(3, 'd')).1, 'b');
}

#[test]
fn incomparable_values_keep_the_first() {
    check!("max_of!(1.0, NaN)", max_of!(1.0, f64::NAN), 1.0);
    check!("max_of!(NaN, 1.0) is NaN", max_of!(f64::NAN, 1.0).is_nan(), true);
}

#[test]
fn max_of_with_names_the_macro_might_use() {
    let first = 3;
    let rest = 8;
    let a = 1;
    let b = 2;
    check!("variables called first, rest, a, b", max_of!(a, first, rest, b), 8);
}

#[test]
fn condition_is_evaluated_once() {
    fn go(calls: &mut u32, answer: bool) -> Result<(), solution::Failure> {
        ensure!(
            {
                *calls += 1;
                answer
            },
            "no"
        );
        Ok(())
    }
    let mut calls = 0;
    let _ = go(&mut calls, true);
    let _ = go(&mut calls, false);
    check!("two calls of the function", calls, 2);
}

#[test]
fn ensure_negates_the_whole_condition() {
    fn go(a: bool, b: bool) -> Result<(), solution::Failure> {
        ensure!(a || b, "neither");
        Ok(())
    }
    check!("a || b for all four", (go(false, false).is_ok(), go(true, false).is_ok(), go(false, true).is_ok(), go(true, true).is_ok()), (false, true, true, true));
}

#[test]
fn random_max_matches_iterator_max() {
    let mut rng = anneal_prelude::Rng::new(8114);
    for _ in 0..300 {
        let v: [i64; 5] = std::array::from_fn(|_| rng.int(-4, 4));
        let got = max_of!(v[0], v[1], v[2], v[3], v[4]);
        check!(format!("{v:?}"), got, *v.iter().max().unwrap());
    }
}
