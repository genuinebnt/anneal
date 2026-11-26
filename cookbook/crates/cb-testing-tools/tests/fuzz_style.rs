//! A fuzz-style harness without cargo-fuzz: deterministic, runs under plain `cargo test`.
//! Pieces: (1) `fuzz_target(&[u8])` (the same signature libFuzzer uses), (2) a seeded mutation
//! fuzzer over the seed corpus in `tests/corpus/`, (3) a crash MINIMISER, (4) a deliberately
//! crashing target to prove (2) and (3) work. Real coverage-guided fuzzing: see README.
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use cb_testing_tools::fuzz_target;

/// xorshift64*: tiny seeded PRNG so every CI run explores the SAME inputs.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const MAX_LEN: usize = 256;

/// One random mutation step: the classic libFuzzer/AFL repertoire.
fn mutate(rng: &mut Rng, data: &mut Vec<u8>, pool: &[Vec<u8>], dict: &[Vec<u8>]) {
    for _ in 0..1 + rng.below(4) {
        match rng.below(7) {
            0 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] ^= 1 << rng.below(8); // bit flip
            }
            1 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] = rng.next() as u8; // byte replace
            }
            2 => {
                let i = rng.below(data.len() + 1);
                data.insert(i, rng.next() as u8); // byte insertion
            }
            3 if !data.is_empty() => {
                let i = rng.below(data.len());
                let n = 1 + rng.below(8);
                data.drain(i..(i + n).min(data.len())); // deletion of a short range
            }
            4 => {
                let other = &pool[rng.below(pool.len())]; // splice: head of ours + tail of another
                let (a, b) = (rng.below(data.len() + 1), rng.below(other.len() + 1));
                data.truncate(a);
                data.extend_from_slice(&other[b..]);
            }
            5 if !dict.is_empty() => {
                let tok = &dict[rng.below(dict.len())]; // dictionary token insertion
                let i = rng.below(data.len() + 1);
                data.splice(i..i, tok.iter().copied());
            }
            _ if !data.is_empty() => {
                let i = rng.below(data.len());
                let j = (i + 1 + rng.below(8)).min(data.len());
                let chunk = data[i..j].to_vec(); // duplicate a chunk (grows nesting/repetition)
                data.splice(i..i, chunk);
            }
            _ => {}
        }
        data.truncate(MAX_LEN);
    }
}

fn load_corpus() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus");
    let mut corpus = Vec::new();
    let mut dict = Vec::new();
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort(); // read_dir order is unspecified; sorted => deterministic
    for p in names {
        let bytes = fs::read(&p).unwrap();
        if p.file_name().unwrap() == "dict.txt" {
            dict = bytes
                .split(|b| *b == b'\n')
                .filter(|t| !t.is_empty())
                .map(<[u8]>::to_vec)
                .collect();
        } else {
            corpus.push(bytes);
        }
    }
    (corpus, dict)
}

/// True if `target` panics on `data` OR exceeds the time budget (a "hang").
fn crashes(target: &dyn Fn(&[u8]), data: &[u8]) -> bool {
    let t = Instant::now();
    let r = catch_unwind(AssertUnwindSafe(|| target(data)));
    r.is_err() || t.elapsed() > Duration::from_millis(250)
}

/// Mutation fuzzing loop; returns the first crashing input.
fn fuzz(target: &dyn Fn(&[u8]), seed: u64, iterations: usize) -> Option<Vec<u8>> {
    let (mut pool, dict) = load_corpus();
    let mut rng = Rng(seed);
    for i in 0..iterations {
        let mut input = pool[rng.below(pool.len())].clone();
        mutate(&mut rng, &mut input, &pool, &dict);
        if crashes(target, &input) {
            return Some(input);
        }
        if i % 16 == 0 && pool.len() < 512 {
            pool.push(input); // crude "corpus growth" (a real fuzzer keeps inputs that add coverage)
        }
    }
    None
}

/// Delta-debugging minimiser: repeatedly delete chunks (halving the chunk size) while the
/// input still crashes, then simplify bytes to '0'. Result: a small, readable reproducer.
fn minimise(target: &dyn Fn(&[u8]), mut data: Vec<u8>) -> Vec<u8> {
    assert!(crashes(target, &data), "minimise needs a crashing input");
    let mut chunk = data.len().div_ceil(2).max(1);
    loop {
        let mut i = 0;
        let mut shrunk = false;
        while i < data.len() {
            let end = (i + chunk).min(data.len());
            let mut cand = data[..i].to_vec();
            cand.extend_from_slice(&data[end..]);
            if crashes(target, &cand) {
                data = cand;
                shrunk = true;
            } else {
                i += chunk;
            }
        }
        if chunk == 1 && !shrunk {
            break;
        }
        chunk = (chunk / 2).max(1);
    }
    for i in 0..data.len() {
        let mut cand = data.clone();
        cand[i] = b'0';
        if data[i] != b'0' && crashes(target, &cand) {
            data = cand;
        }
    }
    data
}

/// A deliberately buggy target: "crashes" whenever two open parens are adjacent.
fn buggy_target(data: &[u8]) {
    if data.windows(2).any(|w| w == b"((") {
        panic!("simulated bug on nested parens");
    }
    fuzz_target(data);
}

#[test]
fn real_target_survives_50k_mutations() {
    let crash = fuzz(&fuzz_target, 0xF00D, 50_000);
    assert!(
        crash.is_none(),
        "fuzz_target crashed on {:?}",
        crash.map(|c| String::from_utf8_lossy(&c).into_owned())
    );
}

#[test]
fn fuzz_target_handles_adversarial_shapes() {
    // hand-written nasties that mutation might take long to find: deep nesting, huge numbers, invalid UTF-8
    let deep_parens = "(".repeat(10_000);
    let deep_neg = "-".repeat(10_000);
    let long_sum = "1+".repeat(50_000) + "1";
    for input in [
        deep_parens.as_bytes(),
        deep_neg.as_bytes(),
        long_sum.as_bytes(),
        b"\xff\xfe(",
        b"99999999999999999999d",
    ] {
        assert!(
            !crashes(&fuzz_target, input),
            "crash on adversarial input of len {}",
            input.len()
        );
    }
}

#[test]
fn fuzzer_finds_and_minimiser_shrinks_the_planted_bug() {
    let found =
        fuzz(&buggy_target, 1234, 50_000).expect("mutation fuzzing should find the planted bug");
    let min = minimise(&buggy_target, found);
    assert_eq!(min, b"((", "minimal reproducer is the two adjacent parens");
}

#[test]
fn minimiser_on_a_padded_input() {
    let min = minimise(
        &buggy_target,
        b"1 + 2 * (x - ((y))) / 99999 + zzzzzzzzz".to_vec(),
    );
    assert_eq!(min, b"((");
}
