use solution::*;

fn s(m: &str) -> String {
    m.to_string()
}

#[test]
fn undo_is_judged_by_todays_stock() {
    let mut book = StockBook::new();
    book.receive("nut", 10).unwrap();
    book.ship("nut", 10).unwrap();
    check!("all gone", book.undo(0), Err(Refused::Short { have: 0, need: 10 }));
    book.receive("nut", 10).unwrap();
    check!("new goods cover it", book.undo(0), Ok(3));
    check!("stock", book.stock("nut"), 0);
}

#[test]
fn items_are_separate() {
    let mut book = StockBook::new();
    book.receive("nut", 3).unwrap();
    book.receive("bolt", 8).unwrap();
    check!("ship nuts", book.ship("nut", 4), Err(Refused::Short { have: 3, need: 4 }));
    book.ship("nut", 3).unwrap();
    check!("bolts cannot cover nuts", book.undo(0), Err(Refused::Short { have: 0, need: 3 }));
    check!("bolts untouched", book.stock("bolt"), 8);
}

#[test]
fn zero_comes_before_short() {
    let mut book = StockBook::new();
    check!("ship 0 of nothing", book.ship("nut", 0), Err(Refused::Zero));
}

#[test]
fn already_undone_comes_before_short() {
    let mut book = StockBook::new();
    book.receive("nut", 2).unwrap();
    book.undo(0).unwrap();
    check!("again, with no stock", book.undo(0), Err(Refused::AlreadyUndone));
}

#[test]
fn big_quantities() {
    let mut book = StockBook::new();
    for _ in 0..5 {
        book.receive("nut", u32::MAX).unwrap();
    }
    check!("stock", book.stock("nut"), 5 * u64::from(u32::MAX));
    book.ship("nut", u32::MAX).unwrap();
    book.undo(0).unwrap();
    check!("after", book.stock("nut"), 3 * u64::from(u32::MAX));
    check!("last line", book.entries()[6].clone(), Entry::Undo { of: 0, item: s("nut"), change: -i64::from(u32::MAX) });
}

#[test]
fn an_entry_number_far_out_of_range() {
    let mut book = StockBook::new();
    check!("empty book", book.undo(usize::MAX), Err(Refused::NoSuchEntry));
    book.receive("nut", 1).unwrap();
    check!("one past", book.undo(1), Err(Refused::NoSuchEntry));
}

#[test]
fn refusals_write_nothing() {
    let mut book = StockBook::new();
    book.receive("nut", 1).unwrap();
    let before = book.entries().to_vec();
    let _ = book.ship("nut", 2);
    let _ = book.undo(9);
    let _ = book.receive("nut", 0);
    check!("book", book.entries().to_vec(), before);
    check!("stock", book.stock("nut"), 1);
}

/// The slow way: every answer is worked out by walking the lines.
#[allow(dead_code)]
#[derive(Default)]
struct Slow(Vec<Entry>);

#[allow(dead_code)]
impl Slow {
    fn stock(&self, want: &str) -> i64 {
        self.0
            .iter()
            .map(|e| match e {
                Entry::In { item, qty } if item == want => i64::from(*qty),
                Entry::Out { item, qty } if item == want => -i64::from(*qty),
                Entry::Undo { item, change, .. } if item == want => *change,
                _ => 0,
            })
            .sum()
    }
    fn push(&mut self, e: Entry) -> Result<usize, Refused> {
        self.0.push(e);
        Ok(self.0.len() - 1)
    }
    fn short(&self, item: &str, need: i64) -> Result<(), Refused> {
        let have = self.stock(item);
        if have < need {
            return Err(Refused::Short { have: have as u64, need: need as u64 });
        }
        Ok(())
    }
    fn receive(&mut self, item: &str, qty: u32) -> Result<usize, Refused> {
        if qty == 0 {
            return Err(Refused::Zero);
        }
        self.push(Entry::In { item: s(item), qty })
    }
    fn ship(&mut self, item: &str, qty: u32) -> Result<usize, Refused> {
        if qty == 0 {
            return Err(Refused::Zero);
        }
        self.short(item, qty.into())?;
        self.push(Entry::Out { item: s(item), qty })
    }
    fn undo(&mut self, n: usize) -> Result<usize, Refused> {
        let (item, change) = match self.0.get(n) {
            None => return Err(Refused::NoSuchEntry),
            Some(Entry::Undo { .. }) => return Err(Refused::NotUndoable),
            Some(Entry::In { item, qty }) => (item.clone(), -i64::from(*qty)),
            Some(Entry::Out { item, qty }) => (item.clone(), i64::from(*qty)),
        };
        if self.0.iter().any(|e| matches!(e, Entry::Undo { of, .. } if *of == n)) {
            return Err(Refused::AlreadyUndone);
        }
        self.short(&item, -change)?;
        self.push(Entry::Undo { of: n, item, change })
    }
}

#[test]
fn random_books_match_the_slow_one() {
    let mut rng = anneal_prelude::Rng::new(7103);
    for _ in 0..300 {
        let (mut fast, mut slow) = (StockBook::new(), Slow::default());
        let mut log = Vec::new();
        for step in 0..40 {
            let item = *rng.pick(&["a", "b"]);
            let qty = rng.below(6) as u32;
            let (got, want) = match rng.below(10) {
                0..=3 => {
                    log.push(format!("receive {item} {qty}"));
                    (fast.receive(item, qty), slow.receive(item, qty))
                }
                4..=6 => {
                    log.push(format!("ship {item} {qty}"));
                    (fast.ship(item, qty), slow.ship(item, qty))
                }
                _ => {
                    let n = rng.below(slow.0.len() + 2);
                    log.push(format!("undo {n}"));
                    (fast.undo(n), slow.undo(n))
                }
            };
            check!(log.join(", "), got, want);
            check!(log.join(", "), fast.stock(item) as i64, slow.stock(item));
            if step % 11 == 10 {
                fast = StockBook::from_entries(fast.entries());
                log.push("rebuild".to_string());
            }
        }
        check!(log.join(", "), fast.entries().to_vec(), slow.0.clone());
    }
}

#[test]
fn a_long_book_stays_quick() {
    let mut book = StockBook::new();
    let n = 150_000;
    for i in 0..n {
        book.receive("nut", 1 + (i % 3) as u32).unwrap();
    }
    for i in 0..n {
        check!("undo", book.undo(i), Ok(n + i));
        assert_eq!(book.undo(i), Err(Refused::AlreadyUndone));
    }
    check!("stock", book.stock("nut"), 0);
    let copy = StockBook::from_entries(book.entries());
    check!("rebuilt", copy.entries().len(), 2 * n);
}
