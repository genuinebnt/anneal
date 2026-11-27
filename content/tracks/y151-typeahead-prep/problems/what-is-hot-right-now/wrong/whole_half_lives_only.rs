use std::collections::HashMap;

pub struct Chart {
    half_life: f64,
    /// Per song, the base-2 logarithm of the sum of `plays * 2^(at / half_life)`. Every score at time `now` is
    /// that sum divided by `2^(now / half_life)`, so the stored numbers keep the order of the scores for ever.
    weights: HashMap<String, f64>,
}

/// log2(2^a + 2^b), without forming either power.
fn log_add(a: f64, b: f64) -> f64 {
    let (high, low) = if a >= b { (a, b) } else { (b, a) };
    high + (low - high).exp2().ln_1p() / std::f64::consts::LN_2
}

impl Chart {
    pub fn new(half_life: u64) -> Chart {
        Chart {
            half_life: half_life as f64,
            weights: HashMap::new(),
        }
    }

    /// `plays` plays of `song`, made at time `at`.
    pub fn report(&mut self, song: &str, plays: u32, at: u64) {
        if plays == 0 {
            return;
        }
        let new = f64::from(plays).log2() + (at / self.half_life as u64) as f64;
        match self.weights.get_mut(song) {
            Some(weight) => *weight = log_add(*weight, new),
            None => {
                self.weights.insert(song.to_string(), new);
            }
        }
    }

    pub fn score(&self, song: &str, now: u64) -> f64 {
        match self.weights.get(song) {
            Some(weight) => (weight - (now / self.half_life as u64) as f64).exp2(),
            None => 0.0,
        }
    }

    /// The names of at most `n` songs, hottest first.
    pub fn top(&self, n: usize) -> Vec<String> {
        let mut songs: Vec<(&String, f64)> = self.weights.iter().map(|(s, &w)| (s, w)).collect();
        songs.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        songs.into_iter().take(n).map(|(s, _)| s.clone()).collect()
    }
}
