use solution::*;


#[allow(dead_code)]
fn pairs(items: &[(&str, i64)]) -> Vec<(String, i64)> {
    items.iter().map(|(n, s)| (n.to_string(), *s)).collect()
}
#[allow(dead_code)]
fn board(items: &[(&str, i64)]) -> Board {
    let mut b = Board::new();
    for (name, score) in items {
        b.set(name, *score);
    }
    b
}

#[test]
fn nobody_there() {
    let mut b = Board::new();
    check!("len", b.len(), 0);
    check!("rank", b.rank("x"), None);
    check!("score", b.score("x"), None);
    check!("remove", b.remove("x"), None);
    check!("count_below", b.count_below(i64::MAX), 0);
}

#[test]
fn the_same_score_again() {
    let mut b = board(&[("a", 1), ("b", 1), ("c", 1)]);
    check!("set b to 1 again", b.set("b", 1), Some(1));
    check!("nothing moved", b.range(0, 9), pairs(&[("a", 1), ("b", 1), ("c", 1)]));
}

#[test]
fn names_are_compared_as_bytes_and_may_be_any_text() {
    let b = board(&[("b", 0), ("B", 0), ("", 0), ("é", 0), ("a b", 0)]);
    check!("byte order", b.range(0, 9), pairs(&[("", 0), ("B", 0), ("a b", 0), ("b", 0), ("é", 0)]));
}

#[test]
fn extreme_scores() {
    let b = board(&[("hi", i64::MAX), ("lo", i64::MIN), ("mid", 0)]);
    check!("order", b.range(0, 9), pairs(&[("lo", i64::MIN), ("mid", 0), ("hi", i64::MAX)]));
    check!("below max", b.count_below(i64::MAX), 2);
}

#[test]
fn random_changes_match_a_sorted_model() {
    let mut rng = anneal_prelude::Rng::new(0x3103);
    for run in 0..300 {
        let mut b = Board::new();
        let mut model: Vec<(i64, String)> = Vec::new();
        let mut history = Vec::new();
        let names = 2 + rng.below(30);
        for _ in 0..120 {
            let name = format!("p{}", rng.below(names));
            let place = model.iter().position(|(_, n)| *n == name);
            let old = place.map(|i| model.remove(i).0);
            if rng.below(3) == 0 {
                history.push(format!("remove {name}"));
                check!(format!("run {run}: {history:?}"), b.remove(&name), old);
            } else {
                let score = rng.int(-5, 5);
                history.push(format!("set {name} {score}"));
                check!(format!("run {run}: {history:?}"), b.set(&name, score), old);
                model.push((score, name));
                model.sort();
            }
            let ctx = || format!("run {run}: after {history:?}");
            check!(ctx(), b.len(), model.len());
            let probe = format!("p{}", rng.below(names));
            check!(format!("{}, rank {probe}", ctx()), b.rank(&probe), model.iter().position(|(_, n)| *n == probe));
            let cut = rng.int(-7, 7);
            check!(format!("{}, count_below {cut}", ctx()), b.count_below(cut), model.iter().filter(|(s, _)| *s < cut).count());
            let (start, count) = (rng.below(model.len() + 2), rng.below(6));
            let want: Vec<(String, i64)> = model.iter().skip(start).take(count).map(|(s, n)| (n.clone(), *s)).collect();
            check!(format!("{}, range {start} {count}", ctx()), b.range(start, count), want);
        }
    }
}

#[test]
fn one_player_over_and_over() {
    let mut b = Board::new();
    for i in 0..1000 {
        b.set("solo", i);
        check!("first and only", (b.len(), b.rank("solo"), b.range(0, 5)), (1, Some(0), pairs(&[("solo", i)])));
    }
    b.remove("solo");
    check!("empty again", (b.len(), b.range(0, 5)), (0, vec![]));
}

#[test]
fn removed_players_free_their_room() {
    // a long run of churn on a small board: memory that is never reused would grow with every change
    let mut b = Board::new();
    for i in 0..300_000u64 {
        b.set(&format!("p{}", i % 100), (i % 977) as i64);
    }
    check!("still 100 players", b.len(), 100);
    check!("all in place", b.range(0, 200).len(), 100);
}

#[test]
fn a_large_board_stays_fast() {
    // 200000 players, rising and falling: any step that walks or shifts the whole board cannot finish in time
    let mut b = Board::new();
    let n = 200_000i64;
    // lowest scores first would suit a plain list, so scores arrive in a scattered order
    for i in 0..n {
        b.set(&format!("p{i:06}"), (i * 7919) % n);
    }
    for i in 0..n {
        let score = (i * 7919) % n;
        let name = format!("p{i:06}");
        check!(format!("rank of {name}"), b.rank(&name), Some(score as usize));
        check!(format!("count_below {score}"), b.count_below(score), score as usize);
        check!(format!("range {score} 1"), b.range(score as usize, 1), vec![(name.clone(), score)]);
    }
    // every player moves to the front and back again
    for i in 0..n {
        let name = format!("p{i:06}");
        let score = (i * 7919) % n;
        b.set(&name, -1);
        check!(format!("{name} is first"), b.rank(&name), Some(0));
        b.set(&name, score);
    }
}
