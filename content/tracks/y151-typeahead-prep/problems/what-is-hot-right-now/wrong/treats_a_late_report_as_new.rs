use std::collections::HashMap;

pub struct Chart {
    half_life: f64,
    songs: HashMap<String, (f64, u64)>,
}

impl Chart {
    pub fn new(half_life: u64) -> Chart {
        Chart {
            half_life: half_life as f64,
            songs: HashMap::new(),
        }
    }

    pub fn report(&mut self, song: &str, plays: u32, at: u64) {
        if plays == 0 {
            return;
        }
        let (value, last) = self.songs.entry(song.to_string()).or_insert((0.0, at));
        let gap = at.saturating_sub(*last) as f64;
        *value = *value * (-gap / self.half_life).exp2() + f64::from(plays);
        *last = at.max(*last);
    }

    pub fn score(&self, song: &str, now: u64) -> f64 {
        self.songs.get(song).map_or(0.0, |&(v, last)| {
            v * (-(now.saturating_sub(last) as f64) / self.half_life).exp2()
        })
    }

    pub fn top(&self, n: usize) -> Vec<String> {
        let weight = |&(v, last): &(f64, u64)| v.log2() + last as f64 / self.half_life;
        let mut songs: Vec<(&String, f64)> =
            self.songs.iter().map(|(s, p)| (s, weight(p))).collect();
        songs.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        songs.into_iter().take(n).map(|(s, _)| s.clone()).collect()
    }
}
