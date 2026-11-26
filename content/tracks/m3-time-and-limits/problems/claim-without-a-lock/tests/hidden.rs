use solution::*;

use std::ops::Range;

#[test]
fn the_units_run_out_in_the_middle() {
    let r = Region::new(8);
    let mut taken = false;
    let got = r.claim_with(|_| {
        if !taken {
            taken = true;
            r.claim(6).unwrap();
        }
        5
    });
    check!("only 2 were left at the end", got, None);
    check!("they are still there", r.claim(2), Some(6..8));
}

#[test]
fn huge_requests() {
    let r = Region::new(100);
    r.claim(10).unwrap();
    check!("usize::MAX", r.claim(usize::MAX), None);
    check!("half of usize::MAX", r.claim(usize::MAX / 2 + 1), None);
    check!("unharmed", r.claim(90), Some(10..100));
}

#[test]
fn an_empty_region() {
    let r = Region::new(0);
    check!("nothing", r.claim(0), Some(0..0));
    check!("one", r.claim(1), None);
    check!("close", r.close(), 0);
}

#[test]
#[should_panic]
fn too_large_a_region_panics() {
    Region::new(usize::MAX / 2 + 1);
}

#[test]
fn the_largest_region() {
    let r = Region::new(usize::MAX / 2);
    check!("all of it", r.claim(usize::MAX / 2), Some(0..usize::MAX / 2));
    check!("close", r.close(), usize::MAX / 2);
    check!("and stays closed", r.claim(0), None);
}

#[test]
fn random_calls_inside_random_calls_match_a_counter() {
    let mut rng = anneal_prelude::Rng::new(9105);
    for _ in 0..600 {
        let len = rng.below(30);
        let r = Region::new(len);
        let (mut used, mut closed) = (0usize, false);
        let mut calls = format!("len {len}");
        for _ in 0..12 {
            let (kind, want, meanwhile, extra) = (rng.below(5), rng.below(9), rng.below(4), rng.below(7));
            if kind == 0 && rng.below(4) == 0 {
                calls += ", close";
                check!(&calls, r.close(), used);
                closed = true;
                continue;
            }
            calls += &format!(", claim_with(min(free, {want})) with action {meanwhile}({extra}) inside");
            let closed_before = closed;
            let mut done = false;
            let got = r.claim_with(|free| {
                if !done {
                    done = true;
                    // something else happens while the caller is thinking
                    if meanwhile == 1 && !closed && extra <= len - used {
                        assert_eq!(r.claim(extra), Some(used..used + extra), "{calls}: the inner claim");
                        used += extra;
                    } else if meanwhile == 2 {
                        assert_eq!(r.close(), used, "{calls}: the inner close");
                        closed = true;
                    }
                }
                want.min(free)
            });
            let expected = if closed || closed_before {
                None
            } else {
                let n = want.min(len - used);
                used += n;
                Some(used - n..used)
            };
            check!(&calls, got, expected);
            check!(&calls, r.free(), if closed { 0 } else { len - used });
        }
    }
}

#[test]
fn an_empty_claim_notices_a_claim_in_the_middle() {
    let r = Region::new(10);
    let mut first = true;
    let got = r.claim_with(|_| {
        if std::mem::take(&mut first) {
            r.claim(7).unwrap();
        }
        0
    });
    check!("empty, at the position of that moment", got, Some(7..7));
}

#[test]
fn threads_never_share_a_unit() {
    for round in 0..50 {
        let r = Region::new(4000);
        let (pieces, total) = std::thread::scope(|s| {
            let workers: Vec<_> = (0..8usize)
                .map(|w| {
                    let r = &r;
                    s.spawn(move || {
                        let mut mine: Vec<Range<usize>> = Vec::new();
                        for i in 0..200 {
                            mine.extend(r.claim(1 + (w + i) % 5));
                        }
                        mine
                    })
                })
                .collect();
            let closer = s.spawn(|| {
                while r.free() > 400 * (round % 9) {
                    std::thread::yield_now();
                    if r.free() == 0 {
                        break;
                    }
                }
                r.close()
            });
            // the closer must not wait for ever if the workers finish first
            let mut pieces: Vec<Range<usize>> = Vec::new();
            for w in workers {
                pieces.extend(w.join().unwrap());
            }
            r.close();
            (pieces, closer.join().unwrap())
        });
        let mut pieces = pieces;
        pieces.sort_by_key(|p| p.start);
        let mut next = 0;
        for p in &pieces {
            check!(format!("round {round}: piece {p:?} starts where the one before ended"), p.start, next);
            next = p.end;
        }
        check!(format!("round {round}: close counted every piece"), total, next);
    }
}
