use solution::*;

/// A balance, and what each of two clerks read from it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
struct Bank {
    balance: u32,
    seen: [u32; 2],
    done: u32,
}

#[allow(dead_code)]
fn clerks() -> Vec<Vec<Step<Bank>>> {
    vec![
        vec![
            |b: &mut Bank| b.seen[0] = b.balance,
            |b: &mut Bank| {
                b.balance = b.seen[0] + 10;
                b.done += 1;
            },
        ],
        vec![
            |b: &mut Bank| b.seen[1] = b.balance,
            |b: &mut Bank| {
                b.balance = b.seen[1] + 10;
                b.done += 1;
            },
        ],
    ]
}

#[allow(dead_code)]
fn no_lost_deposit(b: &Bank) -> bool {
    b.balance == 10 * b.done
}

#[test]
fn replay_shows_every_state() {
    let states = replay(&Bank::default(), &clerks(), &[0, 0, 1, 1]).expect("a good schedule");
    let balances: Vec<u32> = states.iter().map(|b| b.balance).collect();
    check!("one after the other", balances, vec![0, 0, 10, 10, 20]);
}

#[test]
fn replay_refuses_a_schedule_that_cannot_run() {
    check!("actor 2", replay(&Bank::default(), &clerks(), &[0, 2]), None);
    check!("a third step", replay(&Bank::default(), &clerks(), &[0, 0, 0]), None);
    check!("empty", replay(&Bank::default(), &clerks(), &[]), Some(vec![Bank::default()]));
}

#[test]
fn the_lost_deposit_is_found() {
    let found = shortest_failure(&Bank::default(), &clerks(), no_lost_deposit);
    // both read 0, then both write 10
    check!("schedule", found, Some(vec![0, 1, 0, 1]));
}

#[test]
fn nothing_to_find_with_one_clerk() {
    let one = vec![clerks().remove(0)];
    check!("alone", shortest_failure(&Bank::default(), &one, no_lost_deposit), None);
}

#[test]
fn a_bad_start_needs_no_steps() {
    let bad = Bank { balance: 3, ..Bank::default() };
    check!("start", shortest_failure(&bad, &clerks(), no_lost_deposit), Some(vec![]));
}

#[test]
fn a_failure_in_the_middle_counts() {
    // actor 0 takes the lock down and puts it back
    let actors: Vec<Vec<Step<i32>>> = vec![vec![|n: &mut i32| *n -= 1, |n: &mut i32| *n += 1]];
    check!("after one step", shortest_failure(&0, &actors, |&n| n >= 0), Some(vec![0]));
}
