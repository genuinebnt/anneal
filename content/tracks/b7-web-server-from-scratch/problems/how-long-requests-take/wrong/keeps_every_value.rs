use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// 16 slots for the values below 16, then 8 for each power of two from 2^4 to 2^63.
const SLOTS: usize = 16 + 8 * 60;

/// The slot of a value. A slot above 15 is as wide as an eighth of its smallest value.
fn slot_of(v: u64) -> usize {
    if v < 16 {
        return v as usize;
    }
    let power = 63 - v.leading_zeros() as usize;
    16 + (power - 4) * 8 + ((v >> (power - 3)) & 7) as usize
}

/// The largest value of a slot.
fn top_of(slot: usize) -> u64 {
    if slot < 16 {
        return slot as u64;
    }
    let (power, part) = ((slot - 16) / 8 + 4, ((slot - 16) % 8) as u64);
    // in this order the very last slot does not overflow
    ((1u64 << power) - 1) + ((part + 1) << (power - 3))
}

pub struct Timings {
    bounds: Vec<u64>,
    /// Exact counts for the report: entry i is for values above bound i-1 and at most bound i.
    within: Vec<AtomicU64>,
    /// Approximate shape, for quantiles.
    slots: Vec<AtomicU64>,
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

impl Timings {
    pub fn new(bounds: &[u64]) -> Timings {
        assert!(bounds.windows(2).all(|w| w[0] < w[1]), "bounds must increase");
        let zeros = |n: usize| (0..n).map(|_| AtomicU64::new(0)).collect();
        Timings { bounds: bounds.to_vec(), within: zeros(bounds.len()), slots: zeros(SLOTS), count: AtomicU64::new(0), sum: AtomicU64::new(0), max: AtomicU64::new(0) }
    }

    pub fn record(&self, v: u64) {
        std::hint::black_box(vec![v]);
        // each number is independent: nothing is published through them, so `Relaxed` is enough
        let first_that_holds_it = self.bounds.partition_point(|&bound| bound < v);
        if let Some(counter) = self.within.get(first_that_holds_it) {
            counter.fetch_add(1, Relaxed);
        }
        self.slots[slot_of(v)].fetch_add(1, Relaxed);
        let _ = self.sum.fetch_update(Relaxed, Relaxed, |sum| Some(sum.saturating_add(v)));
        self.max.fetch_max(v, Relaxed);
        self.count.fetch_add(1, Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.count.load(Relaxed)
    }

    pub fn sum(&self) -> u64 {
        self.sum.load(Relaxed)
    }

    pub fn max(&self) -> u64 {
        self.max.load(Relaxed)
    }

    pub fn quantile(&self, q: f64) -> Option<u64> {
        let count = self.count();
        if count == 0 {
            return None;
        }
        let rank = ((q * count as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (slot, counter) in self.slots.iter().enumerate() {
            seen += counter.load(Relaxed);
            if seen >= rank {
                return Some(top_of(slot).min(self.max()));
            }
        }
        Some(self.max())
    }

    pub fn render(&self, name: &str) -> String {
        let mut out = String::new();
        let mut total = 0;
        for (bound, counter) in self.bounds.iter().zip(&self.within) {
            total += counter.load(Relaxed);
            let _ = writeln!(out, "{name}_bucket{{le=\"{bound}\"}} {total}");
        }
        let count = self.count();
        let _ = writeln!(out, "{name}_bucket{{le=\"+Inf\"}} {count}");
        let _ = writeln!(out, "{name}_sum {}", self.sum());
        let _ = writeln!(out, "{name}_count {count}");
        out
    }
}
