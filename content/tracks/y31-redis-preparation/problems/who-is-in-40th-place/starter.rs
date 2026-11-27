/// Players ordered by score, then name.
#[derive(Default)]
pub struct Board {}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn score(&self, name: &str) -> Option<i64> {
        todo!()
    }

    /// Adds the player or changes their score. Returns the earlier score.
    pub fn set(&mut self, name: &str, score: i64) -> Option<i64> {
        todo!()
    }

    pub fn remove(&mut self, name: &str) -> Option<i64> {
        todo!()
    }

    /// The player's place in the order, from 0.
    pub fn rank(&self, name: &str) -> Option<usize> {
        todo!()
    }

    /// How many players have a score below `score`.
    pub fn count_below(&self, score: i64) -> usize {
        todo!()
    }

    /// Up to `count` players from place `start` on.
    pub fn range(&self, start: usize, count: usize) -> Vec<(String, i64)> {
        todo!()
    }
}
