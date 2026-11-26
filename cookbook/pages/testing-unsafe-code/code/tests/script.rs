//! Random scripts through `shelf::script::run`: a seeded, dependency-free stand-in for a fuzzer that also runs under Miri.

use shelf::script::{run, Reached};

fn random_script(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 32) as u8
        })
        .collect()
}

#[test]
fn random_hostile_scripts() {
    let seeds = if cfg!(miri) { 24 } else { 5_000 };
    let mut total = Reached::default();
    for seed in 0..seeds {
        let script = random_script(seed, 120);
        // Name the seed if the shelf disagrees with the model, so the failure can be replayed.
        let reached = std::panic::catch_unwind(|| run(&script)).unwrap_or_else(|_| panic!("seed {seed} failed"));
        total += reached;
    }
    // A random test that never reaches the hostile paths proves nothing, so assert that it does.
    assert!(total.lies > 0 && total.reentrant_pushes > 0 && total.reentrant_reads > 0, "{total:?}");
    assert!(total.panics > 0 && total.clears > 0, "{total:?}");
}
