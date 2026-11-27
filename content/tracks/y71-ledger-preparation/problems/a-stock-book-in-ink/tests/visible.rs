use solution::*;

fn s(m: &str) -> String {
    m.to_string()
}

#[test]
fn receiving_and_shipping() {
    let mut book = StockBook::new();
    check!("receive 10", book.receive("nut", 10), Ok(0));
    check!("ship 4", book.ship("nut", 4), Ok(1));
    check!("ship 7", book.ship("nut", 7), Err(Refused::Short { have: 6, need: 7 }));
    check!("stock", book.stock("nut"), 6);
    check!("never seen", book.stock("bolt"), 0);
    check!("lines", book.entries().len(), 2);
}

#[test]
fn an_undo_adds_a_line_and_removes_none() {
    let mut book = StockBook::new();
    book.receive("nut", 10).unwrap();
    book.ship("nut", 4).unwrap();
    check!("undo the shipment", book.undo(1), Ok(2));
    check!("stock", book.stock("nut"), 10);
    check!(
        "the book",
        book.entries().to_vec(),
        vec![
            Entry::In { item: s("nut"), qty: 10 },
            Entry::Out { item: s("nut"), qty: 4 },
            Entry::Undo { of: 1, item: s("nut"), change: 4 },
        ]
    );
}

#[test]
fn goods_that_have_left_cannot_be_unreceived() {
    let mut book = StockBook::new();
    book.receive("nut", 10).unwrap();
    book.ship("nut", 4).unwrap();
    check!("undo the receipt", book.undo(0), Err(Refused::Short { have: 6, need: 10 }));
    check!("nothing was written", book.entries().len(), 2);
}

#[test]
fn undo_refusals() {
    let mut book = StockBook::new();
    book.receive("nut", 1).unwrap();
    check!("no such line", book.undo(5), Err(Refused::NoSuchEntry));
    check!("first undo", book.undo(0), Ok(1));
    check!("again", book.undo(0), Err(Refused::AlreadyUndone));
    check!("undo the undo", book.undo(1), Err(Refused::NotUndoable));
}

#[test]
fn nothing_of_nothing() {
    let mut book = StockBook::new();
    check!("receive 0", book.receive("nut", 0), Err(Refused::Zero));
    check!("ship 0", book.ship("nut", 0), Err(Refused::Zero));
    check!("lines", book.entries().len(), 0);
}

#[test]
fn a_rebuilt_book_remembers_what_was_undone() {
    let mut book = StockBook::new();
    book.receive("nut", 5).unwrap();
    book.receive("nut", 5).unwrap();
    book.undo(0).unwrap();
    let mut copy = StockBook::from_entries(book.entries());
    check!("stock", copy.stock("nut"), 5);
    check!("undo 0 again", copy.undo(0), Err(Refused::AlreadyUndone));
    check!("undo 1", copy.undo(1), Ok(3));
}
