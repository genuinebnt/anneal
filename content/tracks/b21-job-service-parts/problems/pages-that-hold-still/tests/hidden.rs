use solution::*;

#[allow(dead_code)]
pub fn rows(page: &Page) -> Vec<(u64, u64)> {
    page.entries.iter().map(|e| (e.player, e.score)).collect()
}

#[allow(dead_code)]
pub fn board(entries: &[(u64, u64)]) -> Board {
    let mut b = Board::new();
    for &(player, score) in entries {
        b.set(player, score);
    }
    b
}

#[allow(dead_code)]
pub fn walk(b: &Board, limit: usize) -> Vec<Vec<(u64, u64)>> {
    let mut pages = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = b.page(cursor.as_deref(), limit).expect("a cursor the board made");
        pages.push(rows(&page));
        match page.next {
            Some(next) => cursor = Some(next),
            None => return pages,
        }
        assert!(pages.len() <= 300_000, "the walk never ends");
    }
}

#[test]
fn removing_the_last_row_of_a_page_keeps_the_place() {
    let mut b = board(&[(1, 30), (2, 20), (3, 10), (4, 5)]);
    let first = b.page(None, 2).unwrap();
    check!("remove(2)", b.remove(2), true);
    let second = b.page(first.next.as_deref(), 2).unwrap();
    check!("second page after the first page's last row was removed", rows(&second), vec![(3, 10), (4, 5)]);
}

#[test]
fn a_removal_higher_up_skips_nothing() {
    let mut b = board(&[(1, 30), (2, 20), (3, 10), (4, 5)]);
    let first = b.page(None, 2).unwrap();
    b.remove(1);
    check!("second page", rows(&b.page(first.next.as_deref(), 2).unwrap()), vec![(3, 10), (4, 5)]);
}

#[test]
fn the_place_does_not_move_with_the_player() {
    let mut b = board(&[(1, 30), (2, 20), (3, 10), (4, 5)]);
    let first = b.page(None, 2).unwrap();
    b.set(2, 1);
    check!("player 2 fell to the bottom after page 1", rows(&b.page(first.next.as_deref(), 10).unwrap()), vec![(3, 10), (4, 5), (2, 1)]);
}

#[test]
fn ties_across_a_page_boundary() {
    let b = board(&[(5, 7), (1, 7), (4, 7), (2, 7), (3, 7)]);
    check!("five equal scores, two per page", walk(&b, 2), vec![vec![(1, 7), (2, 7)], vec![(3, 7), (4, 7)], vec![(5, 7)]]);
}

#[test]
fn extreme_values() {
    let m = u64::MAX;
    let b = board(&[(m, m), (0, m), (m - 1, 0), (0, m), (7, 0)]);
    check!("extreme ids and scores, one per page", walk(&b, 1), vec![vec![(0, m)], vec![(m, m)], vec![(7, 0)], vec![(m - 1, 0)]]);
}

#[test]
fn set_moves_and_remove_reports() {
    let mut b = board(&[(1, 10), (2, 20)]);
    b.set(1, 30);
    b.set(1, 30);
    check!("player 1 appears once, at the new score", rows(&b.page(None, 10).unwrap()), vec![(1, 30), (2, 20)]);
    check!("remove, remove again, remove a stranger", (b.remove(1), b.remove(1), b.remove(99)), (true, false, false));
    check!("what is left", rows(&b.page(None, 10).unwrap()), vec![(2, 20)]);
}

#[test]
fn limits_are_clamped() {
    let mut b = Board::new();
    for p in 0..250 {
        b.set(p, 1);
    }
    check!("limit 0", b.page(None, 0).unwrap().entries.len(), 1);
    check!("limit 100", b.page(None, 100).unwrap().entries.len(), 100);
    check!("limit 101", b.page(None, 101).unwrap().entries.len(), 100);
    check!("limit usize::MAX", b.page(None, usize::MAX).unwrap().entries.len(), 100);
}

#[test]
fn every_listed_string_is_refused() {
    let b = board(&[(1, 1), (2, 2)]);
    for bad in ["", "abc", "1 2", "!", "not-a-cursor"] {
        check!(format!("cursor {bad:?}"), b.page(Some(bad), 10), Err(BadCursor));
    }
}

#[test]
fn next_is_decided_at_the_time_of_the_call() {
    let mut b = board(&[(1, 30), (2, 20), (3, 10)]);
    let first = b.page(None, 2).unwrap();
    b.remove(3);
    let second = b.page(first.next.as_deref(), 2).unwrap();
    check!("everything after page 1 was removed", (rows(&second), second.next), (vec![], None));
}

#[test]
fn random_walks_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(21302);
    for _ in 0..300 {
        let mut b = Board::new();
        let mut model: std::collections::HashMap<u64, u64> = Default::default();
        let mut log = Vec::new();
        let mut cursor: Option<String> = None;
        // The place where the previous page ended, as (score, player).
        let mut place: Option<(u64, u64)> = None;
        for _ in 0..40 {
            match rng.below(4) {
                0 | 1 => {
                    let (p, s) = (rng.below(8) as u64, rng.below(4) as u64);
                    log.push(format!("set({p}, {s})"));
                    b.set(p, s);
                    model.insert(p, s);
                }
                2 => {
                    let p = rng.below(8) as u64;
                    log.push(format!("remove({p})"));
                    check!(format!("{log:?}"), b.remove(p), model.remove(&p).is_some());
                }
                _ => {
                    let limit = rng.below(4) + 1;
                    log.push(format!("next page, limit {limit}"));
                    let mut all: Vec<(u64, u64)> = model.iter().map(|(p, s)| (*p, *s)).collect();
                    all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                    if let Some((s, p)) = place {
                        all.retain(|e| e.1 < s || (e.1 == s && e.0 > p));
                    }
                    let want: Vec<(u64, u64)> = all.iter().take(limit).copied().collect();
                    let want_more = all.len() > limit;
                    let page = b.page(cursor.as_deref(), limit).expect("a cursor the board made");
                    check!(format!("{log:?}: (rows, has next)"), (rows(&page), page.next.is_some()), (want.clone(), want_more));
                    cursor = page.next;
                    place = match cursor {
                        Some(_) => want.last().map(|&(p, s)| (s, p)),
                        None => None,
                    };
                }
            }
        }
    }
}

#[test]
fn walking_a_large_board_is_fast() {
    let n: u64 = 200_000;
    let mut b = Board::new();
    for p in 0..n {
        b.set(p, (p * 7919) % 1000);
    }
    for p in 0..n {
        b.set(p, (p * 104_729) % 1000);
    }
    let pages = walk(&b, 10);
    check!("200,000 players, ten per page: pages", pages.len(), 20_000);
    check!("rows seen", pages.iter().map(Vec::len).sum::<usize>(), n as usize);
}
