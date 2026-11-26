#[derive(Debug, Clone, Default)]
pub struct Machine {
    pub positions: usize,
    /// `(from, to)`
    pub silent: Vec<(usize, usize)>,
    /// `(from, signal, to)`
    pub signals: Vec<(usize, char, usize)>,
}

pub struct Tracker {
    // your fields
}

impl Tracker {
    pub fn new(machine: &Machine, start: usize) -> Tracker {
        todo!()
    }

    pub fn observe(&mut self, signal: char) {
        todo!()
    }

    /// Sorted, without repeats.
    pub fn possible(&self) -> Vec<usize> {
        todo!()
    }
}
