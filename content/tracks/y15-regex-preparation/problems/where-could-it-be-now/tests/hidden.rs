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

use anneal_prelude::Rng;

#[test]
fn long_silent_chains_are_followed_to_the_end() {
    let m = machine(6, &[(4, 5), (3, 4), (2, 3), (1, 2), (0, 1)], &[(5, 'a', 0)]);
    check!("listed back to front", trace(&m, 0, "aa"), vec![(0..6).collect::<Vec<_>>(); 3]);
}

#[test]
fn a_signal_move_is_taken_once_per_signal() {
    let m = machine(3, &[], &[(0, 'a', 1), (1, 'a', 2)]);
    check!("one a moves one step only", trace(&m, 0, "a"), vec![vec![0], vec![1]]);
}

#[test]
fn staying_where_you_are_needs_a_move() {
    let m = machine(2, &[(0, 1)], &[(1, 'a', 1)]);
    check!("0 is left behind after a signal", trace(&m, 0, "a"), vec![vec![0, 1], vec![1]]);
}

#[test]
fn random_machines_agree_with_marking_every_position_after_every_signal() {
    let mut rng = Rng::new(1505);
    for _ in 0..600 {
        let n = rng.int(1, 7) as usize;
        let k = rng.below(8);
        let silent: Vec<(usize, usize)> = (0..k).map(|_| (rng.below(n), rng.below(n))).collect();
        let k = rng.below(14);
        let signals: Vec<(usize, char, usize)> = (0..k).map(|_| (rng.below(n), *rng.pick(&['a', 'b']), rng.below(n))).collect();
        let start = rng.below(n);
        let len = rng.below(7);
        let heard = rng.string(len, "ab");
        let widen = |set: &mut Vec<bool>| {
            for _ in 0..n {
                for &(from, to) in &silent {
                    if set[from] {
                        set[to] = true;
                    }
                }
            }
        };
        let list = |set: &[bool]| (0..n).filter(|&i| set[i]).collect::<Vec<_>>();
        let mut set = vec![false; n];
        set[start] = true;
        widen(&mut set);
        let mut want = vec![list(&set)];
        for c in heard.chars() {
            let mut next = vec![false; n];
            for &(from, s, to) in &signals {
                if s == c && set[from] {
                    next[to] = true;
                }
            }
            widen(&mut next);
            want.push(list(&next));
            set = next;
        }
        let m = machine(n, &silent, &signals);
        check!(format!("{m:?} from {start} hearing {heard:?}"), trace(&m, start, &heard), want);
    }
}

#[test]
fn histories_double_but_the_answer_stays_small() {
    // From 0, each 'a' can go to 1 or 2, and both come silently back to 0.
    let m = machine(3, &[(1, 0), (2, 0)], &[(0, 'a', 1), (0, 'a', 2)]);
    let mut t = Tracker::new(&m, 0);
    for step in 0..20_000 {
        t.observe('a');
        let now = t.possible();
        if step % 1000 == 0 {
            check!(format!("after {} signals", step + 1), now, vec![0, 1, 2]);
        } else {
            check!(format!("size after {} signals", step + 1), now.len(), 3);
        }
    }
}

#[test]
fn a_large_ring_with_a_few_possibilities() {
    let n = 100_000;
    let signals: Vec<(usize, char, usize)> = (0..n).map(|i| (i, 'a', (i + 1) % n)).collect();
    let silent = vec![(0, n / 2)];
    let m = machine(n, &silent, &signals);
    let mut t = Tracker::new(&m, 0);
    for _ in 0..200 {
        t.observe('a');
    }
    check!("two histories, 200 signals on", t.possible(), vec![200, n / 2 + 200]);
}

#[test]
fn a_very_long_silent_chain() {
    let n = 300_000;
    let silent: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
    let m = machine(n, &silent, &[]);
    check!("300 000 silent moves in a row", Tracker::new(&m, 0).possible().len(), n);
}

#[test]
fn two_trackers_do_not_share_anything() {
    let m = machine(3, &[], &[(0, 'a', 1), (1, 'a', 2)]);
    let mut first = Tracker::new(&m, 0);
    let second = Tracker::new(&m, 1);
    first.observe('a');
    first.observe('a');
    check!("each keeps its own set", (first.possible(), second.possible()), (vec![2], vec![1]));
}
