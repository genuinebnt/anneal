use std::ops::Range;

pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Dice {
        Dice(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

pub trait Worker<W> {
    /// Does one small thing. `true` means this worker has finished.
    fn step(&mut self, world: &mut W) -> bool;
}

pub type Workers<W> = Vec<Box<dyn Worker<W>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub trace: Vec<usize>,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub seed: u64,
    pub trace: Vec<usize>,
}

/// The one scheduling loop. `go_on` sees the world after each step and may stop the run.
fn schedule<W>(
    world: &mut W,
    workers: &mut Workers<W>,
    seed: u64,
    max_steps: usize,
    mut go_on: impl FnMut(&W) -> bool,
) -> (Run, bool) {
    let mut dice = Dice::new(seed);
    let mut live: Vec<usize> = (0..workers.len()).collect();
    let mut trace = Vec::new();
    while !live.is_empty() && trace.len() < max_steps {
        let at = dice.below(live.len());
        let who = live[at];
        trace.push(who);
        if workers[who].step(world) {
            live.remove(at);
        }
        if !go_on(world) {
            let finished = live.is_empty();
            return (Run { trace, finished }, true);
        }
    }
    let finished = live.is_empty();
    (Run { trace, finished }, false)
}

pub fn run<W>(world: &mut W, workers: &mut Workers<W>, seed: u64, max_steps: usize) -> Run {
    schedule(world, workers, seed, max_steps, |_| true).0
}

pub fn replay<W>(world: &mut W, workers: &mut Workers<W>, trace: &[usize]) -> Result<(), usize> {
    let mut finished = vec![false; workers.len()];
    for (at, &who) in trace.iter().enumerate() {
        if finished.get(who) != Some(&false) {
            return Err(at);
        }
        finished[who] = workers[who].step(world);
    }
    Ok(())
}

pub fn hunt<W>(
    seeds: Range<u64>,
    max_steps: usize,
    mut make: impl FnMut() -> (W, Workers<W>),
    holds: impl Fn(&W) -> bool,
) -> Option<Found> {
    for seed in seeds {
        let (mut world, mut workers) = make();
        let (run, broken) = schedule(&mut world, &mut workers, seed, max_steps, &holds);
        if broken {
            return Some(Found {
                seed,
                trace: run.trace,
            });
        }
    }
    None
}
