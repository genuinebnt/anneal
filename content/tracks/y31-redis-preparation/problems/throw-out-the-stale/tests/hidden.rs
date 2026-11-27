use solution::*;


use anneal_prelude::Rng;

/// Real randomness, from a seed. It remembers every question.
#[allow(dead_code)]
struct Seeded {
    rng: Rng,
    asked: Vec<usize>,
}
#[allow(dead_code)]
impl Seeded {
    fn new(seed: u64) -> Seeded {
        Seeded { rng: Rng::new(seed), asked: Vec::new() }
    }
}
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has no answer");
        self.asked.push(n);
        self.rng.below(n)
    }
}

/// Gives the same number every time, or the largest allowed if that is smaller.
#[allow(dead_code)]
struct Loaded(usize);
impl Dice for Loaded {
    fn below(&mut self, n: usize) -> usize {
        self.0.min(n - 1)
    }
}

/// `total` sessions; those whose number is under `ended` end at 10, the rest at 1000.
#[allow(dead_code)]
fn sessions(total: usize, ended: usize) -> Sessions {
    let mut s = Sessions::new();
    for i in 0..total {
        s.set(&format!("s{i}"), if i < ended { 10 } else { 1000 });
    }
    s
}
#[allow(dead_code)]
fn number(id: &str) -> usize {
    id[1..].parse().expect("an id")
}

#[test]
fn a_session_ends_at_its_time_exactly() {
    let mut s = sessions(1, 1);
    check!("at 9", s.sweep(9, &mut Seeded::new(1), 1).len(), 0);
    check!("at 10", s.sweep(10, &mut Seeded::new(1), 1), vec!["s0".to_string()]);
}

#[test]
fn each_look_asks_for_an_index_among_the_sessions_left() {
    let mut s = sessions(30, 30);
    let mut dice = Seeded::new(5);
    s.sweep(10, &mut dice, 1);
    check!("one round of 20 looks, the store shrinking each time", dice.asked, (11..=30).rev().collect::<Vec<_>>());
}

#[test]
fn a_round_stops_when_the_store_is_empty() {
    let mut s = sessions(3, 3);
    let mut dice = Seeded::new(6);
    check!("all three", s.sweep(10, &mut dice, 100).len(), 3);
    check!("three questions", dice.asked, vec![3, 2, 1]);
}

#[test]
fn different_indexes_name_different_sessions() {
    // one session of 12 has ended; a die stuck on i finds it for exactly one i
    for ended in 0..12 {
        let mut found = 0;
        for i in 0..12 {
            let mut s = Sessions::new();
            for j in 0..12 {
                s.set(&format!("s{j}"), if j == ended { 10 } else { 1000 });
            }
            found += s.sweep(10, &mut Loaded(i), 1).len();
        }
        check!(format!("session s{ended}"), found, 1);
    }
}

#[test]
fn the_indexes_stay_one_to_one_after_removals() {
    let mut rng = Rng::new(0x3105);
    for run in 0..200 {
        let mut s = Sessions::new();
        let mut live: Vec<String> = Vec::new();
        let mut history = Vec::new();
        for step in 0..40 {
            if !live.is_empty() && rng.below(3) == 0 {
                let id = live.swap_remove(rng.below(live.len()));
                history.push(format!("remove {id}"));
                check!(format!("run {run}: {history:?}"), s.remove(&id), true);
            } else {
                let id = format!("s{step}");
                history.push(format!("set {id}"));
                s.set(&id, 1000);
                live.push(id);
            }
        }
        // end one of them: still exactly one stuck die finds it
        if live.is_empty() {
            continue;
        }
        let victim = live[rng.below(live.len())].clone();
        let mut hits = 0;
        for i in 0..live.len() {
            s.set(&victim, 10);
            let gone = s.sweep(10, &mut Loaded(i), 1);
            if !gone.is_empty() {
                check!(format!("run {run}: {history:?}"), gone, vec![victim.clone()]);
                hits += 1;
                // put it back; the indexes may now be assigned anew
                break;
            }
        }
        check!(format!("run {run}: every session can be reached after {history:?}"), hits, 1);
    }
}

#[test]
fn another_round_only_if_more_than_a_quarter_were_ended() {
    let mut rng = Rng::new(0x3107);
    let (mut stopped_at_five, mut went_on_at_six) = (0, 0);
    for run in 0..3000 {
        // large enough that three rounds never empty it
        let total = 61 + rng.below(60);
        let ended = rng.below(total * 2 / 3);
        let mut s = sessions(total, ended);
        let mut dice = Seeded::new(10_000 + run);
        let removed = s.sweep(10, &mut dice, 3).len();
        let looks = dice.asked.len();
        let ctx = format!("run {run}: {total} sessions, {ended} ended; the dice were asked below(n) for n = {:?}", dice.asked);
        check!(format!("{ctx}: whole rounds"), looks % 20 == 0 && (20..=60).contains(&looks), true);
        // the size drops by one after each look that removes, so the questions show each round's removals
        let mut sizes = dice.asked.clone();
        sizes.push(total - removed);
        let rounds = looks / 20;
        for round in 0..rounds {
            let hits = sizes[round * 20] - sizes[(round + 1) * 20];
            if round + 1 < rounds {
                check!(format!("{ctx}: round {} removed {hits}, yet another ran", round + 1), hits >= 6, true);
                went_on_at_six += usize::from(hits == 6);
            } else if rounds < 3 {
                check!(format!("{ctx}: round {} removed {hits}, and none followed", round + 1), hits <= 5, true);
                stopped_at_five += usize::from(hits == 5);
            }
        }
    }
    check!("both sides of the threshold came up", stopped_at_five > 0 && went_on_at_six > 0, true);
}

#[test]
fn only_ended_sessions_are_ever_removed() {
    let mut rng = Rng::new(0x3106);
    for run in 0..300 {
        let total = rng.below(60);
        let ended = rng.below(total + 1);
        let mut s = sessions(total, ended);
        let rounds = rng.below(8);
        let mut dice = Seeded::new(run);
        let gone = s.sweep(10, &mut dice, rounds);
        let ctx = format!("run {run}: {total} sessions, {ended} ended, {rounds} rounds, removed {gone:?}");
        check!(ctx.clone(), gone.iter().all(|id| number(id) < ended && s.ends_at(id).is_none()), true);
        let unique: std::collections::HashSet<&String> = gone.iter().collect();
        check!(ctx.clone(), (unique.len(), s.len()), (gone.len(), total - gone.len()));
        check!(format!("{ctx}: at most {rounds} rounds"), dice.asked.len() <= rounds * 20, true);
    }
}

#[test]
fn with_half_ended_the_sweep_keeps_going() {
    // stopping needs a round with 5 ended or fewer among 20, which is rare while half have ended
    let gone: usize = (0..20).map(|seed| sessions(2000, 1000).sweep(10, &mut Seeded::new(seed), usize::MAX).len()).sum();
    check!(format!("removed {gone} over 20 sweeps of 1000 ended each"), gone > 2000 && gone <= 20_000, true);
}

#[test]
fn repeated_sweeps_find_everything() {
    let mut s = sessions(500, 40);
    let mut dice = Seeded::new(8);
    let total: usize = (0..2000).map(|_| s.sweep(10, &mut dice, 1).len()).sum();
    check!("all 40", (total, s.len()), (40, 460));
}

#[test]
fn a_large_store_stays_fast() {
    // picking, adding or removing by walking the store cannot finish in time
    let mut s = Sessions::new();
    let n = 200_000;
    for i in 0..n {
        s.set(&format!("s{i}"), (i % 2) as u64);
    }
    let mut dice = Seeded::new(9);
    let first = s.sweep(0, &mut dice, usize::MAX).len();
    check!("only ended sessions go", first > 0 && first <= n / 2, true);
    let mut left = n - first;
    for _ in 0..20_000 {
        left -= s.sweep(1, &mut dice, 1).len();
    }
    check!("len agrees", s.len(), left);
    for i in 0..n {
        s.remove(&format!("s{i}"));
    }
    check!("empty", s.len(), 0);
}
