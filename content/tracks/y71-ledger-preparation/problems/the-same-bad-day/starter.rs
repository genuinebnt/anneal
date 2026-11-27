use std::ops::Range;

pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Dice {
        todo!()
    }

    pub fn next_u64(&mut self) -> u64 {
        todo!()
    }

    pub fn below(&mut self, n: usize) -> usize {
        todo!()
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

pub fn run<W>(world: &mut W, workers: &mut Workers<W>, seed: u64, max_steps: usize) -> Run {
    todo!()
}

pub fn replay<W>(world: &mut W, workers: &mut Workers<W>, trace: &[usize]) -> Result<(), usize> {
    todo!()
}

pub fn hunt<W>(
    seeds: Range<u64>,
    max_steps: usize,
    make: impl FnMut() -> (W, Workers<W>),
    holds: impl Fn(&W) -> bool,
) -> Option<Found> {
    todo!()
}
