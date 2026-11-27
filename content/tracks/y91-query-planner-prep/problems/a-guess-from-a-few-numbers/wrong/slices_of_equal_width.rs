pub struct Summary {
    pub kept: Vec<i64>,
    pub total: usize,
}

pub fn summarise(mut times: Vec<i64>, limit: usize) -> Summary {
    times.sort_unstable();
    let total = times.len();
    if total <= limit {
        return Summary { kept: times, total };
    }
    // the smallest, the largest, and how many fall in each of the equally wide slices
    let (low, high) = (times[0] as i128, times[total - 1] as i128);
    let slices = (limit.saturating_sub(2)).max(1);
    let mut kept = vec![low as i64, high as i64];
    if limit > 2 {
        kept.resize(limit, 0);
        for &t in &times {
            let s = ((t as i128 - low) * slices as i128 / (high - low + 1)) as usize;
            kept[2 + s] += 1;
        }
    }
    Summary { kept, total }
}

impl Summary {
    pub fn at_most(&self, x: i64) -> usize {
        if self.kept.len() == self.total {
            return self.kept.partition_point(|&v| v <= x);
        }
        let (low, high) = (self.kept[0] as i128, self.kept[1] as i128);
        if (x as i128) < low {
            return 0;
        }
        if x as i128 >= high {
            return self.total;
        }
        let counts = &self.kept[2..];
        if counts.is_empty() {
            return self.total / 2;
        }
        let at = (x as i128 - low) as f64 / (high - low + 1) as f64 * counts.len() as f64;
        let whole = at as usize;
        let before: i64 = counts[..whole].iter().sum();
        before as usize + ((at - whole as f64) * counts[whole] as f64) as usize
    }
}
