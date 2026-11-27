use solution::*;

#[allow(dead_code)]
fn rec(pos: u64, day: u32, shop: &str, kind: Kind, cents: u64) -> Record {
    Record { pos, day, shop: shop.to_string(), kind, cents }
}

#[allow(dead_code)]
fn small_log() -> Vec<Record> {
    vec![
        rec(0, 1, "north", Kind::Sale, 500),
        rec(1, 1, "south", Kind::Sale, 300),
        rec(2, 2, "north", Kind::Refund, 500),
        rec(3, 2, "south", Kind::Sale, 50),
    ]
}

#[test]
fn a_hole_inside_a_batch_counts_nothing_of_it() {
    let log = small_log();
    let mut b = Board::new();
    b.apply(&log[..1]).unwrap();
    let before = b.clone();
    let batch = vec![log[1].clone(), log[3].clone()];
    check!("1 then 3", b.apply(&batch), Err(Gap { expected: 2, got: 3 }));
    check!("unchanged", b, before);
}

#[test]
fn repeats_inside_a_batch() {
    let log = small_log();
    let batch = vec![log[0].clone(), log[0].clone(), log[1].clone(), log[0].clone(), log[2].clone()];
    let mut b = Board::new();
    check!("counted", b.apply(&batch), Ok(3));
    check!("board", b, Board::as_of(&log, 3));
}

#[test]
fn an_empty_batch() {
    let mut b = Board::new();
    check!("empty", b.apply(&[]), Ok(0));
    check!("board", b, Board::new());
}

#[test]
fn huge_positions_are_holes() {
    let mut b = Board::new();
    check!("max", b.apply(&[rec(u64::MAX, 0, "a", Kind::Sale, 1)]), Err(Gap { expected: 0, got: u64::MAX }));
}

#[test]
fn huge_amounts() {
    let log: Vec<Record> = (0..6).map(|i| rec(i, u32::MAX, "a", if i < 4 { Kind::Sale } else { Kind::Refund }, u64::MAX)).collect();
    let b = Board::rebuild(&log);
    let m = i128::from(u64::MAX);
    check!("net", b.net("a"), 2 * m);
    check!("day", b.day(u32::MAX), (4 * u128::from(u64::MAX), 2 * u128::from(u64::MAX)));
}

#[test]
fn only_refunds() {
    let b = Board::rebuild(&[rec(0, 0, "a", Kind::Refund, 7)]);
    check!("negative", b.top(5), vec![("a".to_string(), -7)]);
}

#[test]
fn a_messy_feed_gives_the_same_board_as_a_rebuild() {
    let mut rng = anneal_prelude::Rng::new(7109);
    for _ in 0..300 {
        let n = rng.below(25);
        let log: Vec<Record> = (0..n as u64)
            .map(|i| {
                let kind = if rng.below(4) == 0 { Kind::Refund } else { Kind::Sale };
                rec(i, rng.below(4) as u32, *rng.pick(&["a", "b", "c"]), kind, rng.below(100) as u64)
            })
            .collect();
        let mut board = Board::new();
        let mut trace = vec![format!("{log:?}")];
        for _ in 0..30 {
            let from = rng.below(n + 1);
            let to = from + rng.below(n - from + 1);
            let at = board.next() as usize;
            let want = if from == to {
                Ok(0)
            } else if from <= at {
                Ok(to.saturating_sub(at))
            } else {
                Err(Gap { expected: at as u64, got: from as u64 })
            };
            trace.push(format!("apply {from}..{to}"));
            check!(trace.join("; "), board.apply(&log[from..to]), want);
            let pos = board.next();
            check!(trace.join("; "), board.clone(), Board::as_of(&log, pos));
        }
        // the totals against a plain sum
        let full = Board::rebuild(&log);
        for shop in ["a", "b", "c"] {
            let want: i128 = log.iter().filter(|r| r.shop == shop).map(|r| if r.kind == Kind::Sale { r.cents as i128 } else { -(r.cents as i128) }).sum();
            check!(trace[0].clone(), full.net(shop), want);
        }
        for day in 0..4 {
            let sum = |k: Kind| log.iter().filter(|r| r.day == day && r.kind == k).map(|r| u128::from(r.cents)).sum::<u128>();
            check!(trace[0].clone(), full.day(day), (sum(Kind::Sale), sum(Kind::Refund)));
        }
    }
}

#[test]
fn one_record_at_a_time_stays_quick() {
    let n = 200_000u64;
    let log: Vec<Record> = (0..n).map(|i| rec(i, (i / 1000) as u32, &format!("shop{}", i % 5000), Kind::Sale, 1)).collect();
    let mut b = Board::new();
    for i in 0..log.len() {
        assert_eq!(b.apply(&log[i..=i]), Ok(1));
    }
    check!("next", b.next(), n);
    check!("one shop", b.net("shop0"), 40);
}
