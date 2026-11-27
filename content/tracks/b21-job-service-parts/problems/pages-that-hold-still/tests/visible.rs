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
fn best_first_with_ties_by_player() {
    let b = board(&[(3, 50), (1, 70), (2, 50), (4, 90)]);
    let page = b.page(None, 10).unwrap();
    check!("one page of everything: ((player, score) rows, next)", (rows(&page), page.next), (vec![(4, 90), (1, 70), (2, 50), (3, 50)], None));
}

#[test]
fn walking_in_pages() {
    let b = board(&[(1, 5), (2, 4), (3, 3), (4, 2), (5, 1)]);
    check!("five players, two per page", walk(&b, 2), vec![vec![(1, 5), (2, 4)], vec![(3, 3), (4, 2)], vec![(5, 1)]]);
}

#[test]
fn a_full_last_page_has_no_next() {
    let b = board(&[(1, 5), (2, 4), (3, 3), (4, 2)]);
    check!("four players, two per page", walk(&b, 2), vec![vec![(1, 5), (2, 4)], vec![(3, 3), (4, 2)]]);
}

#[test]
fn a_new_leader_does_not_repeat_a_row() {
    let mut b = board(&[(1, 30), (2, 20), (3, 10)]);
    let first = b.page(None, 2).unwrap();
    b.set(9, 100);
    let second = b.page(first.next.as_deref(), 2).unwrap();
    check!("second page after a new leader arrived", rows(&second), vec![(3, 10)]);
}

#[test]
fn empty_board() {
    let page = Board::new().page(None, 10).unwrap();
    check!("empty board", (rows(&page), page.next), (vec![], None));
}

#[test]
fn garbage_is_refused() {
    let b = board(&[(1, 1)]);
    check!("cursor \"abc\"", b.page(Some("abc"), 10), Err(BadCursor));
}
