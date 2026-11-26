use solution::*;

#[allow(dead_code)]
fn machine(positions: usize, silent: &[(usize, usize)], signals: &[(usize, char, usize)]) -> Machine {
    Machine { positions, silent: silent.to_vec(), signals: signals.to_vec() }
}

/// The possible positions before the first signal and after each one.
#[allow(dead_code)]
fn trace(m: &Machine, start: usize, heard: &str) -> Vec<Vec<usize>> {
    let mut t = Tracker::new(m, start);
    let mut out = vec![t.possible()];
    for c in heard.chars() {
        t.observe(c);
        out.push(t.possible());
    }
    out
}

#[test]
fn one_signal_one_move() {
    let m = machine(3, &[], &[(0, 'a', 1), (1, 'b', 2)]);
    check!("hearing ab", trace(&m, 0, "ab"), vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn a_signal_can_lead_to_several_places() {
    let m = machine(3, &[], &[(0, 'a', 2), (0, 'a', 1), (1, 'b', 0), (2, 'c', 0)]);
    check!("hearing ab", trace(&m, 0, "ab"), vec![vec![0], vec![1, 2], vec![0]]);
}

#[test]
fn silent_moves_add_places_before_and_after_signals() {
    let m = machine(4, &[(0, 1), (2, 3)], &[(1, 'a', 2)]);
    check!("hearing a", trace(&m, 0, "a"), vec![vec![0, 1], vec![2, 3]]);
}

#[test]
fn impossible_stays_impossible() {
    let m = machine(2, &[], &[(0, 'a', 1), (1, 'a', 0)]);
    check!("hearing axa", trace(&m, 0, "axa"), vec![vec![0], vec![1], vec![], vec![]]);
}

#[test]
fn silent_loops_end() {
    let m = machine(3, &[(0, 1), (1, 2), (2, 0), (1, 1)], &[(2, 'a', 2)]);
    check!("a loop of silent moves", trace(&m, 1, "a"), vec![vec![0, 1, 2], vec![0, 1, 2]]);
}
