use anneal_prelude::Rng;
use solution::*;

fn bad(wanted: Wanted, start: usize, end: usize) -> Result<Order, Mistake> {
    Err(Mistake { wanted, start, end })
}

#[test]
fn fixed_words_are_not_names() {
    check!("a thing called from", read("move 1 from from a to b"), bad(Wanted::Thing, 7, 11));
    check!("a place called to", read("move 1 desk from To to b"), bad(Wanted::Place, 17, 19));
    check!("a place called urgent", read("move 1 desk from a to urgent"), bad(Wanted::Place, 22, 28));
    check!("digits in a name", read("move 1 desk2 from a to b"), bad(Wanted::Thing, 7, 12));
    check!("letters beyond ASCII", read("move 1 café from a to b"), bad(Wanted::Thing, 7, 12));
}

#[test]
fn counts() {
    for (word, ok) in [("1", true), ("4294967295", true), ("007", true), ("0", false), ("4294967296", false), ("+5", false), ("-1", false), ("two", false)] {
        let got = read(&format!("move {word} desks from a to b"));
        if ok {
            check!(word.to_string(), got.map(|o| o.items[0].0), Ok(word.parse::<u32>().unwrap()));
        } else {
            check!(word.to_string(), got, bad(Wanted::Count, 5, 5 + word.len()));
        }
    }
}

#[test]
fn commas() {
    check!("a comma before from", read("move 1 desk, from a to b"), bad(Wanted::Count, 13, 17));
    check!("two commas", read("move 1 desk,,2 pans from a to b"), bad(Wanted::Count, 12, 13));
    check!("no comma", read("move 1 desk 2 pans from a to b"), bad(Wanted::From, 12, 13));
    check!("commas without spaces", read("move 1 desk,2 pans from a to b").map(|o| o.items.len()), Ok(2));
    check!("a comma after the end", read("move 1 desk from a to b,"), bad(Wanted::End, 23, 24));
}

#[test]
fn positions_are_bytes() {
    check!("after a two-byte word", read("möve 1 desk"), bad(Wanted::Move, 0, 5));
    check!("the end after wide letters", read("move 1 desk from a to é"), bad(Wanted::Place, 22, 24));
}

const NAMES: [&str; 6] = ["desk", "bed", "mon", "shed", "x", "loft"];
const DAYS: [(&str, Day); 7] = [("mon", Day::Mon), ("tue", Day::Tue), ("wed", Day::Wed), ("thu", Day::Thu), ("fri", Day::Fri), ("sat", Day::Sat), ("sun", Day::Sun)];

/// A correct order as its pieces. Each piece comes with what a reader wants in that place.
fn order(rng: &mut Rng) -> (Vec<(String, Wanted)>, Order) {
    let mut words = vec![("move".to_string(), Wanted::Move)];
    let mut items = Vec::new();
    for i in 0..rng.int(1, 4) {
        if i > 0 {
            words.push((",".to_string(), Wanted::From));
        }
        let count = rng.int(1, 99) as u32;
        let thing = rng.pick(&NAMES).to_string();
        words.push((count.to_string(), Wanted::Count));
        words.push((thing.clone(), Wanted::Thing));
        items.push((count, thing));
    }
    let from = rng.pick(&NAMES).to_string();
    let to = rng.pick(&NAMES).to_string();
    words.push(("from".to_string(), Wanted::From));
    words.push((from.clone(), Wanted::Place));
    words.push(("to".to_string(), Wanted::To));
    words.push((to.clone(), Wanted::Place));
    let by = rng.bool().then(|| *rng.pick(&DAYS));
    if let Some((name, _)) = by {
        words.push(("by".to_string(), Wanted::End));
        words.push((name.to_string(), Wanted::Day));
    }
    let urgent = rng.bool();
    if urgent {
        words.push(("urgent".to_string(), Wanted::End));
    }
    (words, Order { items, from, to, by: by.map(|d| d.1), urgent })
}

/// Joins pieces with random space and random capitals. Returns the text and where each piece lies.
fn write(rng: &mut Rng, words: &[(String, Wanted)]) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut spans = Vec::new();
    for (i, (word, _)) in words.iter().enumerate() {
        if i > 0 && (word != "," || rng.bool()) {
            for _ in 0..rng.int(1, 2) {
                text.push(*rng.pick(&[' ', '\n', '\t']));
            }
        }
        let start = text.len();
        for c in word.chars() {
            text.push(if rng.bool() { c.to_ascii_uppercase() } else { c });
        }
        spans.push((start, text.len()));
    }
    (text, spans)
}

#[test]
fn random_orders_read_back() {
    let mut rng = Rng::new(18101);
    for _ in 0..400 {
        let (words, want) = order(&mut rng);
        let (text, _) = write(&mut rng, &words);
        check!(format!("{text:?}"), read(&text), Ok(want));
    }
}

#[test]
fn random_orders_cut_short() {
    let mut rng = Rng::new(18102);
    for _ in 0..400 {
        let (words, _) = order(&mut rng);
        let keep = rng.below(words.len());
        let (text, _) = write(&mut rng, &words[..keep]);
        let got = read(&text);
        // the order is complete after the second place, after the day and after `urgent`
        let to = words.iter().position(|(w, _)| w == "to").unwrap();
        let complete = keep == to + 2 || (keep > to + 2 && words[keep - 1].0 != "by");
        if complete {
            check!(format!("{text:?} is complete"), got.is_ok(), true);
        } else {
            // a cut before a comma leaves a finished item: what is wanted next is `from`
            check!(format!("{text:?}"), got, bad(words[keep].1, text.len(), text.len()));
        }
    }
}

#[test]
fn random_orders_with_one_spoiled_piece() {
    let mut rng = Rng::new(18103);
    for _ in 0..400 {
        let (mut words, _) = order(&mut rng);
        let at = rng.below(words.len());
        words[at].0 = "??".to_string();
        let (text, spans) = write(&mut rng, &words);
        check!(format!("{text:?}"), read(&text), bad(words[at].1, spans[at].0, spans[at].1));
    }
}

#[test]
fn any_pieces_at_all() {
    let mut rng = Rng::new(18104);
    let bits = ["move", "from", "to", "by", "urgent", ",", "0", "1", "desk", "é", "\u{3000}", "mon"];
    for _ in 0..2000 {
        let len = rng.below(10);
        let text: Vec<&str> = (0..len).map(|_| *rng.pick(&bits)).collect();
        let text = text.join(" ");
        if let Err(m) = read(&text) {
            let inside = m.start <= m.end && m.end <= text.len() && text.is_char_boundary(m.start);
            check!(format!("{text:?} points inside the message"), inside, true);
        }
    }
}

#[test]
fn a_very_long_list() {
    let text = format!("move {}1 desk from a to b", "1 desk, ".repeat(70_000));
    check!("70 001 items", read(&text).map(|o| o.items.len()), Ok(70_001));
}
