use solution::*;

fn shelf(pages: &[&str]) -> Shelf {
    let mut s = Shelf::new();
    for p in pages {
        s.add(p);
    }
    s
}

const HELP: [&str; 5] = [
    "Reset your password",
    "Change the Wi-Fi password",
    "Wi-Fi keeps dropping",
    "Reset the router",
    "Billing questions",
];

#[test]
fn pages_are_numbered_as_they_arrive() {
    let mut s = Shelf::new();
    check!("first and second add", (s.add("a"), s.add("b")), (0, 1));
}

#[test]
fn one_word() {
    let s = shelf(&HELP);
    check!("password", s.pages_with("password"), vec![0, 1]);
    check!("RESET, in any case", s.pages_with("RESET"), vec![0, 3]);
    check!("a word on no page", s.pages_with("printer"), Vec::<u32>::new());
}

#[test]
fn punctuation_splits_words_and_a_page_is_listed_once() {
    let s = shelf(&["Wi-Fi isn't on, on, on!"]);
    for w in ["wi", "fi", "isn", "t", "on"] {
        check!(w, s.pages_with(w), vec![0]);
    }
}

#[test]
fn all_any_and_none() {
    let s = shelf(&HELP);
    check!("all: wi, password", s.find(&["wi", "password"], &[], &[]), vec![1]);
    check!("any: router, billing", s.find(&[], &["router", "billing"], &[]), vec![3, 4]);
    check!("none: wi", s.find(&[], &[], &["wi"]), vec![0, 3, 4]);
    check!(
        "all: reset; any: router, password; none: your",
        s.find(&["reset"], &["router", "password"], &["your"]),
        vec![3]
    );
}

#[test]
fn nothing_asked_means_every_page() {
    check!("three empty lists", shelf(&HELP).find(&[], &[], &[]), vec![0, 1, 2, 3, 4]);
    check!("an empty shelf", Shelf::new().find(&[], &[], &[]), Vec::<u32>::new());
}

#[test]
fn a_missing_word_in_each_place() {
    let s = shelf(&HELP);
    check!("in all", s.find(&["reset", "printer"], &[], &[]), Vec::<u32>::new());
    check!("alone in any", s.find(&[], &["printer"], &[]), Vec::<u32>::new());
    check!("in none", s.find(&["reset"], &[], &["printer"]), vec![0, 3]);
}
