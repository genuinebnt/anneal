/// Players ordered by score, then name.
#[derive(Default)]
pub struct Board {
    order: Vec<(i64, String)>,
    scores: std::collections::HashMap<String, i64>,
}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }
    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
    pub fn score(&self, name: &str) -> Option<i64> {
        self.scores.get(name).copied()
    }
    pub fn set(&mut self, name: &str, score: i64) -> Option<i64> {
        let old = self.remove(name);
        let key = (score, name.to_string());
        let at = self.order.partition_point(|e| e < &key);
        self.order.insert(at, key);
        self.scores.insert(name.to_string(), score);
        old
    }
    pub fn remove(&mut self, name: &str) -> Option<i64> {
        let score = self.scores.remove(name)?;
        let at = self.order.partition_point(|e| (e.0, e.1.as_str()) < (score, name));
        self.order.remove(at);
        Some(score)
    }
    pub fn rank(&self, name: &str) -> Option<usize> {
        let score = self.score(name)?;
        Some(self.order.partition_point(|e| (e.0, e.1.as_str()) < (score, name)))
    }
    pub fn count_below(&self, score: i64) -> usize {
        self.order.partition_point(|e| e.0 < score)
    }
    pub fn range(&self, start: usize, count: usize) -> Vec<(String, i64)> {
        self.order.iter().skip(start).take(count).map(|(s, n)| (n.clone(), *s)).collect()
    }
}
