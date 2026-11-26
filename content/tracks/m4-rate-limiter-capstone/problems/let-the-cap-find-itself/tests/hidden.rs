use solution::*;

/// The rules, written out over plain numbers.
struct Model {
    limit: u32,
    min: u32,
    max: u32,
    cuts: u32,
    /// (cuts when given, took the last place)
    out: Vec<(u32, bool)>,
}
impl Model {
    fn acquire(&mut self) -> bool {
        let n = self.out.len() as u32;
        if n >= self.limit {
            return false;
        }
        self.out.push((self.cuts, n + 1 == self.limit));
        true
    }
    fn report(&mut self, i: usize, what: usize) {
        let (cuts, filled) = self.out.swap_remove(i);
        if cuts != self.cuts {
            return;
        }
        if what == 0 && filled {
            self.limit = (self.limit + 1).min(self.max);
        }
        if what == 1 {
            self.cuts += 1;
            self.limit = (self.limit / 2).max(self.min);
        }
    }
}


fn take_all(a: &Adaptive) -> Vec<Ticket> {
    std::iter::from_fn(|| a.try_acquire()).take(10_000).collect()
}

#[test]
#[should_panic]
fn min_zero_panics() {
    Adaptive::new(1, 0, 5);
}

#[test]
#[should_panic]
fn min_above_max_panics() {
    Adaptive::new(1, 6, 5);
}

#[test]
fn the_ticket_that_filled_it_may_report_last_or_first() {
    for last_first in [false, true] {
        let a = Adaptive::new(3, 1, 10);
        let mut held = take_all(&a);
        if last_first {
            held.reverse();
        }
        for t in held {
            t.success();
        }
        check!(format!("reversed: {last_first}"), a.limit(), 4);
    }
}

#[test]
fn filling_is_decided_when_the_ticket_is_given() {
    let a = Adaptive::new(2, 1, 10);
    let first = a.try_acquire().unwrap();
    let second = a.try_acquire().unwrap();
    drop(first);
    second.success();
    check!("it filled the gate when it was given", a.limit(), 3);
    let b = Adaptive::new(2, 1, 10);
    let first = b.try_acquire().unwrap();
    let _second = b.try_acquire().unwrap();
    first.success();
    check!("the first one never filled it", b.limit(), 2);
}

#[test]
fn after_a_cut_the_gate_waits_for_room() {
    let a = Adaptive::new(8, 1, 10);
    let mut held = take_all(&a);
    held.pop().unwrap().overload();
    check!("(limit, in flight, more)", (a.limit(), a.in_flight(), a.try_acquire().is_some()), (4, 7, false));
    held.truncate(3);
    let again = a.try_acquire();
    check!("one place", (again.is_some(), a.in_flight()), (true, 4));
}

#[test]
fn a_cut_at_the_minimum_still_ends_an_episode() {
    let a = Adaptive::new(2, 2, 10);
    let mut held = take_all(&a);
    let filled = held.pop().unwrap();
    held.pop().unwrap().overload();
    filled.success();
    check!("the old success is ignored", a.limit(), 2);
}

#[test]
fn the_maximum_holds() {
    let a = Adaptive::new(u32::MAX, 1, u32::MAX);
    check!("start", a.limit(), u32::MAX);
    let b = Adaptive::new(3, 1, 3);
    for t in take_all(&b) {
        t.success();
    }
    check!("capped", b.limit(), 3);
}

#[test]
fn random_work_matches_the_rules() {
    let mut rng = anneal_prelude::Rng::new(9214);
    for _ in 0..500 {
        let min = 1 + rng.below(3) as u32;
        let max = min + rng.below(10) as u32;
        let initial = rng.below(15) as u32;
        let a = Adaptive::new(initial, min, max);
        let mut m = Model { limit: initial.clamp(min, max), min, max, cuts: 0, out: Vec::new() };
        let mut held: Vec<Ticket> = Vec::new();
        let mut calls = format!("new({initial}, {min}, {max}):");
        for _ in 0..80 {
            if held.is_empty() || rng.bool() {
                let got = a.try_acquire();
                calls += " acquire";
                check!(&calls, got.is_some(), m.acquire());
                held.extend(got);
            } else {
                let i = rng.below(held.len());
                let what = [0, 0, 0, 1, 2][rng.below(5)];
                calls += [" success", " overload", " drop"][what];
                calls += &format!("({i})");
                let ticket = held.swap_remove(i);
                match what {
                    0 => ticket.success(),
                    1 => ticket.overload(),
                    _ => drop(ticket),
                }
                m.report(i, what);
            }
            check!(&calls, (a.limit(), a.in_flight()), (m.limit, m.out.len() as u32));
        }
    }
}

#[test]
fn one_cut_however_many_threads_report() {
    for _ in 0..100 {
        let a = Adaptive::new(64, 1, 100);
        let mut held = take_all(&a);
        std::thread::scope(|s| {
            while !held.is_empty() {
                let batch: Vec<Ticket> = held.drain(..8).collect();
                s.spawn(move || batch.into_iter().for_each(Ticket::overload));
            }
        });
        check!("(limit, in flight)", (a.limit(), a.in_flight()), (32, 0));
    }
}

#[test]
fn threads_sharing_a_clone_leak_nothing() {
    let a = Adaptive::new(5, 1, 9);
    std::thread::scope(|s| {
        for w in 0..8 {
            let a = a.clone();
            s.spawn(move || {
                for i in 0..2000 {
                    if let Some(t) = a.try_acquire() {
                        match (w + i) % 11 {
                            0 => t.overload(),
                            1 => drop(t),
                            _ => t.success(),
                        }
                    }
                    let limit = a.limit();
                    assert!((1..=9).contains(&limit), "limit {limit} left its bounds");
                }
            });
        }
    });
    check!("nothing leaked", a.in_flight(), 0);
}
