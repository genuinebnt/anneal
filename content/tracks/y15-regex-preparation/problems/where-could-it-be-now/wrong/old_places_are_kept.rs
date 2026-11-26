#[derive(Debug, Clone, Default)]
pub struct Machine {
    pub positions: usize,
    /// `(from, to)`
    pub silent: Vec<(usize, usize)>,
    /// `(from, signal, to)`
    pub signals: Vec<(usize, char, usize)>,
}

pub struct Tracker {
    silent: Vec<Vec<usize>>,
    signals: Vec<Vec<(char, usize)>>,
    current: Vec<usize>,
}

impl Tracker {
    pub fn new(machine: &Machine, start: usize) -> Tracker {
        let mut silent = vec![Vec::new(); machine.positions];
        for &(from, to) in &machine.silent {
            silent[from].push(to);
        }
        let mut signals = vec![Vec::new(); machine.positions];
        for &(from, signal, to) in &machine.signals {
            signals[from].push((signal, to));
        }
        let mut tracker = Tracker {
            silent,
            signals,
            current: Vec::new(),
        };
        tracker.current = tracker.close(vec![start]);
        tracker
    }

    /// The seeds and everything silently reachable from them, each once.
    fn close(&self, mut work: Vec<usize>) -> Vec<usize> {
        let mut seen = vec![false; self.silent.len()];
        let mut set = Vec::new();
        while let Some(at) = work.pop() {
            if std::mem::replace(&mut seen[at], true) {
                continue;
            }
            set.push(at);
            work.extend_from_slice(&self.silent[at]);
        }
        set
    }

    pub fn observe(&mut self, signal: char) {
        let mut seeds: Vec<usize> = self
            .current
            .iter()
            .flat_map(|&from| &self.signals[from])
            .filter(|(heard, _)| *heard == signal)
            .map(|&(_, to)| to)
            .collect();
        if !seeds.is_empty() {
            seeds.extend(&self.current);
        }
        self.current = self.close(seeds);
    }

    /// Sorted, without repeats.
    pub fn possible(&self) -> Vec<usize> {
        let mut out = self.current.clone();
        out.sort_unstable();
        out
    }
}
