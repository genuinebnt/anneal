/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

/// Sessions and when each one ends.
#[derive(Default)]
pub struct Sessions {}

impl Sessions {
    pub fn new() -> Sessions {
        Sessions::default()
    }

    /// Adds the session, or moves its end.
    pub fn set(&mut self, id: &str, ends_at: u64) {
        todo!()
    }

    pub fn remove(&mut self, id: &str) -> bool {
        todo!()
    }

    pub fn ends_at(&self, id: &str) -> Option<u64> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes ended sessions found by chance, and returns their ids.
    pub fn sweep(&mut self, now: u64, dice: &mut dyn Dice, max_rounds: usize) -> Vec<String> {
        todo!()
    }
}
