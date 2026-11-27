use solution::*;

fn service(capacity: usize) -> Service {
    Service::new(capacity, Box::new(classify))
}

fn terms(a: &Answer) -> String {
    a.items.iter().map(|s| s.term.as_str()).collect::<Vec<_>>().join(", ")
}

#[test]
fn the_second_keystroke_is_cached() {
    let s = service(8);
    s.add(&[("rust", 5), ("ruby", 3)], 0);
    let first = s.suggest("ru", 5, 0);
    check!("first", (first.cached, terms(&first)), (false, "rust, ruby".to_string()));
    let again = s.suggest(" RU", 5, 0);
    check!("same key", (again.cached, terms(&again)), (true, "rust, ruby".to_string()));
    check!("another limit", s.suggest("ru", 1, 0).cached, false);
}

#[test]
fn a_new_term_appears_at_once() {
    let s = service(8);
    s.add(&[("javascript", 5)], 0);
    s.suggest("script", 5, 0);
    s.add(&[("typescript", 9)], 0);
    check!("both", terms(&s.suggest("script", 5, 0)), "typescript, javascript".to_string());
}

#[test]
fn an_unrelated_write_keeps_the_answer() {
    let s = service(8);
    s.add(&[("rust", 5), ("python", 3)], 0);
    s.suggest("ru", 5, 0);
    s.choose("python", 0);
    check!("still cached", s.suggest("ru", 5, 0).cached, true);
}

#[test]
fn a_cached_answer_still_fades() {
    let s = service(8);
    s.add(&[("rust", 8)], 0);
    s.suggest("ru", 5, 0);
    let later = s.suggest("ru", 5, 7200);
    check!("cached, a quarter", (later.cached, (later.items[0].score - 2.0).abs() < 1e-4), (true, true));
}

#[test]
fn capacity_zero_caches_nothing() {
    let s = service(0);
    s.add(&[("rust", 8)], 0);
    s.suggest("ru", 5, 0);
    check!("again", s.suggest("ru", 5, 0).cached, false);
}
