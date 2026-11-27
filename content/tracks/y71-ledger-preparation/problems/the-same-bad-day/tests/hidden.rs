use solution::*;

/// A shared counter, and a log of what happened.
#[allow(dead_code)]
#[derive(Default, Debug)]
struct World {
    count: u32,
    done: u32,
    log: Vec<usize>,
}

/// A clerk who reads in one step and writes in the next.
#[allow(dead_code)]
struct Clerk {
    seen: Option<u32>,
}

impl Worker<World> for Clerk {
    fn step(&mut self, w: &mut World) -> bool {
        match self.seen.take() {
            None => {
                self.seen = Some(w.count);
                false
            }
            Some(n) => {
                w.count = n + 1;
                w.done += 1;
                true
            }
        }
    }
}

/// Takes `left` steps and writes its name in the log each time.
#[allow(dead_code)]
struct Scribe {
    name: usize,
    left: usize,
}

impl Worker<World> for Scribe {
    fn step(&mut self, w: &mut World) -> bool {
        assert!(self.left > 0, "a finished worker was stepped");
        self.left -= 1;
        w.log.push(self.name);
        self.left == 0
    }
}

#[allow(dead_code)]
fn clerks(n: usize) -> (World, Workers<World>) {
    let list = (0..n).map(|_| Box::new(Clerk { seen: None }) as Box<dyn Worker<World>>).collect();
    (World::default(), list)
}

#[allow(dead_code)]
fn scribes(steps: &[usize]) -> (World, Workers<World>) {
    let list = steps.iter().enumerate().map(|(name, &left)| Box::new(Scribe { name, left }) as Box<dyn Worker<World>>).collect();
    (World::default(), list)
}

/// Nobody's work is lost.
#[allow(dead_code)]
fn nothing_lost(w: &World) -> bool {
    w.count == w.done
}

#[test]
fn seed_zero_and_uneven_work() {
    let (mut world, mut workers) = scribes(&[3, 1, 2]);
    check!("seed 0", run(&mut world, &mut workers, 0, 100).trace, vec![0, 0, 0, 1, 2, 2]);
}

#[test]
fn another_known_day() {
    let (mut world, mut workers) = scribes(&[4, 4]);
    check!("seed 99", run(&mut world, &mut workers, 99, 100).trace, vec![1, 0, 1, 1, 1, 0, 0, 0]);
}

#[test]
fn the_step_limit() {
    let (mut world, mut workers) = scribes(&[5, 5]);
    let cut = run(&mut world, &mut workers, 99, 3);
    check!("three steps", (cut.trace.len(), cut.finished), (3, false));
    check!("world", world.log.len(), 3);
    let (mut world, mut workers) = scribes(&[2]);
    check!("exactly enough", run(&mut world, &mut workers, 1, 2), Run { trace: vec![0, 0], finished: true });
    let (mut world, mut workers) = scribes(&[2]);
    check!("none allowed", run(&mut world, &mut workers, 1, 0), Run { trace: vec![], finished: false });
}

#[test]
fn no_workers() {
    let (mut world, mut workers) = clerks(0);
    check!("empty", run(&mut world, &mut workers, 3, 10), Run { trace: vec![], finished: true });
    check!("nothing to break", hunt(0..5, 10, || clerks(0), |_| false), None);
}

#[test]
fn one_clerk_never_loses_anything() {
    check!("alone", hunt(0..200, 100, || clerks(1), nothing_lost), None);
}

#[test]
fn an_empty_range_of_seeds() {
    check!("no seeds", hunt(5..5, 100, || clerks(2), nothing_lost), None);
}

#[test]
fn a_broken_moment_that_heals_is_still_found() {
    // the rule is "the log never has exactly two lines"; every full run passes through that moment
    let found = hunt(7..9, 100, || scribes(&[3, 3]), |w| w.log.len() != 2);
    check!("seed and length", found.map(|f| (f.seed, f.trace.len())), Some((7, 2)));
}

#[test]
fn hunt_reports_the_first_seed_and_stops_at_the_break() {
    let found = hunt(0..1000, 100, || clerks(3), nothing_lost).expect("three clerks collide");
    for seed in 0..=found.seed {
        let (mut w, mut c) = clerks(3);
        let full = run(&mut w, &mut c, seed, 100);
        let (mut w, mut c) = clerks(3);
        let mut first_break = None;
        for (i, &who) in full.trace.iter().enumerate() {
            replay(&mut w, &mut c, &[who]).ok();
            if first_break.is_none() && !nothing_lost(&w) {
                first_break = Some(full.trace[..=i].to_vec());
            }
        }
        // `replay` forgets who finished between calls, which is fine here: the trace came from `run`
        let want = if seed == found.seed { Some(found.trace.clone()) } else { None };
        check!(format!("seed {seed}"), first_break, want);
    }
}

#[test]
fn random_crews_replay_exactly() {
    let mut rng = anneal_prelude::Rng::new(7111);
    for _ in 0..300 {
        let steps: Vec<usize> = (0..rng.below(6)).map(|_| 1 + rng.below(5)).collect();
        let seed = rng.next_u64();
        let max = rng.below(30);
        let (mut world, mut workers) = scribes(&steps);
        let got = run(&mut world, &mut workers, seed, max);
        let total: usize = steps.iter().sum();
        let input = format!("steps {steps:?}, seed {seed}, max {max}");
        check!(input.clone(), (got.trace.len(), got.finished), (total.min(max), total <= max));
        for (i, &s) in steps.iter().enumerate() {
            check!(input.clone(), got.trace.iter().filter(|&&w| w == i).count() <= s, true);
        }
        let (mut again, mut workers) = scribes(&steps);
        check!(input.clone(), replay(&mut again, &mut workers, &got.trace), Ok(()));
        check!(input.clone(), again.log, world.log);
        // the same schedule, worked out from the rules with this crate's own dice
        let mut dice = Dice::new(seed);
        let mut left = steps.clone();
        let mut want = Vec::new();
        while want.len() < max {
            let live: Vec<usize> = (0..left.len()).filter(|&i| left[i] > 0).collect();
            if live.is_empty() {
                break;
            }
            let who = live[dice.below(live.len())];
            left[who] -= 1;
            want.push(who);
        }
        check!(input, got.trace, want);
    }
}
