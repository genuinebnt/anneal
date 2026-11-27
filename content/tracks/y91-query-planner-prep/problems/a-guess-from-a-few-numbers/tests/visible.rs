use solution::*;

/// Ask the question of a summary rebuilt from its two public fields, so nothing else can be remembered.
#[allow(dead_code)]
fn ask(s: &Summary, x: i64) -> usize {
    Summary { kept: s.kept.clone(), total: s.total }.at_most(x)
}

#[test]
fn with_room_for_everything_the_answers_are_exact() {
    let s = summarise(vec![30, 10, 20, 20], 10);
    check!("at most 9", ask(&s, 9), 0);
    check!("at most 20", ask(&s, 20), 3);
    check!("at most 25", ask(&s, 25), 3);
    check!("at most 30", ask(&s, 30), 4);
}

#[test]
fn a_thousand_times_in_eleven_numbers() {
    let s = summarise((1..=1000).collect(), 11);
    check!("at most 11 numbers are kept", s.kept.len() <= 11, true);
    // 1000 / (11 - 1) = 100
    for x in [1, 250, 500, 999] {
        let guess = ask(&s, x);
        check!(format!("at most {x}: guessed {guess}, within 100 of the truth"), guess.abs_diff(x as usize) <= 100, true);
    }
    check!("below the smallest, exactly", ask(&s, 0), 0);
    check!("at the largest, exactly", ask(&s, 1000), 1000);
}

#[test]
fn no_times_at_all() {
    let s = summarise(vec![], 5);
    check!("total and answer", (s.total, ask(&s, 7)), (0, 0));
}

#[test]
fn one_time_many_times() {
    let s = summarise(vec![42; 500], 4);
    check!("below, at and above 42", (ask(&s, 41), ask(&s, 42), ask(&s, 43)), (0, 500, 500));
}

#[test]
fn most_deliveries_are_quick_and_a_few_take_weeks() {
    let mut times: Vec<i64> = (1..=990).collect();
    times.extend((1..=10).map(|i| i * 1_000_000));
    let s = summarise(times, 101);
    // 1000 / (101 - 1) = 10
    let guess = ask(&s, 500);
    check!(format!("at most 500: guessed {guess}, the truth is 500, allowed error 10"), guess.abs_diff(500) <= 10, true);
}
