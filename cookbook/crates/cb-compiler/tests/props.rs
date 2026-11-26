//! Type-soundness smoke tests: a type-directed generator builds well-typed
//! programs; they must check without errors and run without *internal*
//! (type-confusion) errors. Overflow/bounds traps are legitimate outcomes.

mod common;
use common::progen::Gen;
use common::*;
use pebble::interp::RuntimeErrorKind;
use pebble::sema::CheckOptions;

#[test]
fn generated_programs_typecheck_and_run_soundly() {
    big_stack(|| {
        let mut traps = 0;
        for seed in 0..500u64 {
            let mut g = Gen {
                rng: Rng::new(seed + 100),
                fns: Vec::new(),
                counter: 0,
            };
            let src = g.program();
            let an = pebble::analyze(&src, &CheckOptions::default());
            if an.has_errors() {
                let file = pebble::source::SourceFile::new("gen.peb", src.clone());
                panic!(
                    "seed {seed}: generated program has errors\n{}\n{src}",
                    pebble::diag::render_all(&an.diagnostics, &file, &Default::default())
                );
            }
            let mut out = Vec::new();
            if let Err(e) = pebble::interp::run(&an.hir, &mut out) {
                assert_ne!(
                    e.kind,
                    RuntimeErrorKind::Internal,
                    "seed {seed}: type unsoundness: {e}\n{src}"
                );
                traps += 1;
            }
        }
        assert!(traps < 250, "suspiciously many traps: {traps}");
    });
}

#[test]
fn mutated_generated_programs_never_panic_checker() {
    big_stack(|| {
        let mut rng = Rng::new(77);
        for seed in 0..400u64 {
            let mut g = Gen {
                rng: Rng::new(seed + 9_000),
                fns: Vec::new(),
                counter: 0,
            };
            let mut chars: Vec<char> = g.program().chars().collect();
            for _ in 0..1 + rng.below(3) {
                let i = rng.below(chars.len());
                match rng.below(3) {
                    0 => {
                        chars.remove(i);
                    }
                    1 => chars.insert(
                        i,
                        *rng.pick(&['(', ')', '{', '}', ';', '+', '1', 'x', '"', '[', ':']),
                    ),
                    _ => {
                        let j = rng.below(chars.len());
                        chars.swap(i, j);
                    }
                }
            }
            let src: String = chars.into_iter().collect();
            let an = pebble::analyze(&src, &CheckOptions { warn_shadow: true });
            // If it still checks, it must also run without internal errors.
            if !an.has_errors() {
                let mut out = Vec::new();
                if let Err(e) = pebble::interp::run(&an.hir, &mut out) {
                    assert_ne!(e.kind, RuntimeErrorKind::Internal, "{e}\n{src}");
                }
            }
        }
    });
}
