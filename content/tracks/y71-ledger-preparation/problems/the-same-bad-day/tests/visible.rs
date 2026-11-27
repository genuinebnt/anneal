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
fn the_first_rolls() {
    let mut d = Dice::new(1);
    check!("seed 1", [d.next_u64(), d.next_u64(), d.next_u64()], [1082269761, 1152992998833853505, 11177516664432764457]);
    check!("seed 0", Dice::new(0).next_u64(), 15860402102123842989);
}

#[test]
fn a_known_day() {
    let (mut world, mut workers) = scribes(&[2, 2, 2]);
    let run_ = run(&mut world, &mut workers, 5, 100);
    check!("seed 5", run_.trace.clone(), vec![0, 0, 2, 2, 1, 1]);
    check!("finished", run_.finished, true);
    check!("each step really ran", world.log, run_.trace);
}

#[test]
fn the_same_seed_gives_the_same_day() {
    let once = {
        let (mut w, mut c) = clerks(4);
        (run(&mut w, &mut c, 42, 100), w.count)
    };
    let twice = {
        let (mut w, mut c) = clerks(4);
        (run(&mut w, &mut c, 42, 100), w.count)
    };
    check!("seed 42 twice", once, twice);
}

#[test]
fn a_lost_update_is_found_and_replayed() {
    let found = hunt(0..1000, 100, || clerks(2), nothing_lost).expect("some seed loses an update");
    let (mut world, mut workers) = clerks(2);
    check!("replay", replay(&mut world, &mut workers, &found.trace), Ok(()));
    check!("broken again", nothing_lost(&world), false);
}

#[test]
fn replay_refuses_a_bad_trace() {
    let (mut world, mut workers) = scribes(&[1, 2]);
    check!("a finished worker", replay(&mut world, &mut workers, &[0, 1, 0, 1]), Err(2));
    check!("steps before it ran", world.log, vec![0, 1]);
    let (mut world, mut workers) = scribes(&[1]);
    check!("no such worker", replay(&mut world, &mut workers, &[3]), Err(0));
}
