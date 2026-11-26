//! The HONEST tests: a differential test against a `Vec`, using the API the way a well-behaved caller would.
//! Run them under Miri against a planted variant to see how little an honest suite proves about soundness.

use shelf::Shelf;

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn honest_random_operations_match_a_vec() {
    let rounds: u64 = if cfg!(miri) { 4 } else { 2_000 };
    for seed in 1..=rounds {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut shelf = Shelf::<String, 8>::new();
        let mut model: Vec<String> = Vec::new();
        for step in 0..40 {
            let word = format!("s{seed}-{step}");
            match xorshift(&mut rng) % 5 {
                0 => {
                    let ok = shelf.push(word.clone()).is_ok();
                    assert_eq!(ok, model.len() < 8, "seed {seed} step {step}: push");
                    if ok {
                        model.push(word);
                    }
                }
                1 => {
                    let n = (xorshift(&mut rng) % 4) as usize;
                    let items = vec![word; n]; // an honest ExactSizeIterator
                    let fits = n <= 8 - model.len();
                    assert_eq!(shelf.extend_exact(items.clone()).is_ok(), fits, "seed {seed} step {step}: extend");
                    if fits {
                        model.extend(items);
                    }
                }
                2 => {
                    let n = (xorshift(&mut rng) % 4) as usize;
                    // a well-behaved closure: it does not touch the shelf and does not panic
                    let stored = shelf.fill_with(n, |k| format!("{word}-{k}"));
                    assert_eq!(stored, n.min(8 - model.len()), "seed {seed} step {step}: fill");
                    model.extend((0..stored).map(|k| format!("{word}-{k}")));
                }
                3 => {
                    let i = (xorshift(&mut rng) % 10) as usize;
                    assert_eq!(shelf.get(i), model.get(i), "seed {seed} step {step}: get({i})");
                }
                _ => {
                    if xorshift(&mut rng).is_multiple_of(4) {
                        shelf.clear();
                        model.clear();
                    }
                }
            }
            assert_eq!(shelf.len(), model.len(), "seed {seed} step {step}: len");
            assert!(shelf.iter().eq(model.iter()), "seed {seed} step {step}: contents");
        }
    }
}
