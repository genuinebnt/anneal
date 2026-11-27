use solution::*;

fn s(m: &str) -> String {
    m.to_string()
}

/// Decides and applies, as the desk does.
#[allow(dead_code)]
fn ask(book: &mut Book, command: Command) -> Result<Vec<Event>, Refusal> {
    let events = decide(book, &command)?;
    for e in &events {
        *book = evolve(std::mem::take(book), e);
    }
    Ok(events)
}

#[test]
fn lending_and_returning() {
    let mut book = Book::default();
    check!("lend to ann", ask(&mut book, Command::Lend { member: s("ann") }), Ok(vec![Event::Lent { member: s("ann") }]));
    check!("who has it", book.out_to.clone(), Some(s("ann")));
    check!("lend to bo", ask(&mut book, Command::Lend { member: s("bo") }), Err(Refusal::OnLoan));
    check!("bo returns it", ask(&mut book, Command::Return { member: s("bo") }), Err(Refusal::NotYourLoan));
    check!("ann returns it", ask(&mut book, Command::Return { member: s("ann") }), Ok(vec![Event::Returned]));
    check!("the card", book, Book { lends: 1, ..Book::default() });
}

#[test]
fn a_return_sets_the_book_aside_for_the_next_in_line() {
    let mut book = Book::default();
    ask(&mut book, Command::Lend { member: s("ann") }).unwrap();
    ask(&mut book, Command::Reserve { member: s("bo") }).unwrap();
    ask(&mut book, Command::Reserve { member: s("cy") }).unwrap();
    check!(
        "ann returns it",
        ask(&mut book, Command::Return { member: s("ann") }),
        Ok(vec![Event::Returned, Event::SetAside { member: s("bo") }])
    );
    check!("held for", book.held_for.clone(), Some(s("bo")));
    check!("still queued", book.queue.iter().cloned().collect::<Vec<_>>(), vec![s("cy")]);
    check!("cy asks first", ask(&mut book, Command::Lend { member: s("cy") }), Err(Refusal::HeldForAnother));
}

#[test]
fn dropping_a_hold_passes_it_on() {
    let mut book = Book::default();
    check!("a free book is set aside at once", ask(&mut book, Command::Reserve { member: s("ann") }), Ok(vec![Event::SetAside { member: s("ann") }]));
    ask(&mut book, Command::Reserve { member: s("bo") }).unwrap();
    check!(
        "ann cancels",
        ask(&mut book, Command::Cancel { member: s("ann") }),
        Ok(vec![Event::HoldDropped, Event::SetAside { member: s("bo") }])
    );
    check!("ann cancels again", ask(&mut book, Command::Cancel { member: s("ann") }), Err(Refusal::NothingToCancel));
}

#[test]
fn deciding_changes_nothing() {
    let book = replay(&[Event::Lent { member: s("ann") }]);
    let before = book.clone();
    let _ = decide(&book, &Command::Return { member: s("ann") });
    check!("the card after deciding", book, before);
}

#[test]
fn retiring() {
    let mut book = Book::default();
    ask(&mut book, Command::Lend { member: s("ann") }).unwrap();
    check!("retire while out", ask(&mut book, Command::Retire), Err(Refusal::OnLoan));
    ask(&mut book, Command::Return { member: s("ann") }).unwrap();
    check!("retire", ask(&mut book, Command::Retire), Ok(vec![Event::Retired]));
    check!("retire again", ask(&mut book, Command::Retire), Err(Refusal::Retired));
    check!("reserve", ask(&mut book, Command::Reserve { member: s("bo") }), Err(Refusal::Retired));
}

#[test]
fn old_history_still_loads() {
    // today's desk would never write this, but it is what happened
    let book = replay(&[
        Event::Returned,
        Event::Retired,
        Event::Lent { member: s("ann") },
    ]);
    check!("who has it", book.out_to, Some(s("ann")));
    check!("retired", book.retired, true);
}
