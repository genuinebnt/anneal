//! RECIPE 14 (resilience): exponential backoff with full jitter.
//! LEVEL: simple
//! Use in: retry stage.   Run: cargo run --example retry_backoff_jitter
//! COMMON USES: every network client that retries (HTTP, RPC, queue consumers): jitter prevents synchronized retry storms.
//!
//! Pattern: wait `random(0, min(cap, base * 2^attempt))`. Exponential growth backs off quickly; the random
//! jitter stops many clients retrying in lockstep (thundering herd). Inject the RNG so tests are exact.
//! Read: https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/ (AWS builders' library)
#![allow(dead_code)]
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn delay_ms(attempt: u32, base: u64, cap: u64, rng: &mut Rng) -> u64 {
    let exp = base.saturating_mul(1u64 << attempt.min(20)).min(cap);
    rng.next() % (exp + 1)
}

fn main() {
    let (base, cap) = (100, 5_000);
    for seed in [1u64, 2] {
        let mut rng = Rng(seed * 0x9E37_79B9_7F4A_7C15);
        let delays: Vec<u64> = (0..8).map(|a| delay_ms(a, base, cap, &mut rng)).collect();
        println!("seed {seed}: {delays:?} ms");
        for (a, d) in delays.iter().enumerate() {
            let ceiling = (base << a.min(20)).min(cap);
            assert!(*d <= ceiling, "attempt {a}: {d} > {ceiling}");
        }
    }
}
