use solution::*;


use anneal_prelude::Rng;

/// Real randomness, from a seed. It remembers every question.
#[allow(dead_code)]
struct Seeded {
    rng: Rng,
    asked: Vec<usize>,
}
#[allow(dead_code)]
impl Seeded {
    fn new(seed: u64) -> Seeded {
        Seeded { rng: Rng::new(seed), asked: Vec::new() }
    }
}
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has no answer");
        self.asked.push(n);
        self.rng.below(n)
    }
}

/// Gives the same number every time, or the largest allowed if that is smaller.
#[allow(dead_code)]
struct Loaded(usize);
impl Dice for Loaded {
    fn below(&mut self, n: usize) -> usize {
        self.0.min(n - 1)
    }
}

/// `total` sessions; those whose number is under `ended` end at 10, the rest at 1000.
#[allow(dead_code)]
fn sessions(total: usize, ended: usize) -> Sessions {
    let mut s = Sessions::new();
    for i in 0..total {
        s.set(&format!("s{i}"), if i < ended { 10 } else { 1000 });
    }
    s
}
#[allow(dead_code)]
fn number(id: &str) -> usize {
    id[1..].parse().expect("an id")
}

#[test]
fn set_ends_and_remove() {
    let mut s = Sessions::new();
    s.set("a", 5);
    s.set("b", 9);
    s.set("a", 7);
    check!("two sessions", s.len(), 2);
    check!("a moved", s.ends_at("a"), Some(7));
    check!("remove a", s.remove("a"), true);
    check!("again", s.remove("a"), false);
    check!("b is still there", (s.len(), s.ends_at("b")), (1, Some(9)));
}

#[test]
fn if_all_have_ended_they_all_go() {
    let mut s = sessions(100, 100);
    let mut gone = s.sweep(10, &mut Seeded::new(1), 100);
    gone.sort();
    check!("all 100, each once", (gone.len(), s.len()), (100, 0));
}

#[test]
fn if_none_has_ended_one_round_is_enough() {
    let mut s = sessions(100, 100);
    let mut dice = Seeded::new(2);
    check!("nothing at 9", s.sweep(9, &mut dice, 100), Vec::<String>::new());
    check!("twenty looks", dice.asked.len(), 20);
}

#[test]
fn a_small_store_needs_fewer_looks() {
    let mut dice = Seeded::new(3);
    sessions(7, 0).sweep(10, &mut dice, 5);
    check!("seven looks for seven sessions", dice.asked, vec![7; 7]);
}

#[test]
fn zero_rounds_and_an_empty_store() {
    let mut dice = Seeded::new(4);
    check!("no rounds", sessions(5, 5).sweep(10, &mut dice, 0), Vec::<String>::new());
    check!("nothing to look at", Sessions::new().sweep(10, &mut dice, 5), Vec::<String>::new());
    check!("no dice were thrown", dice.asked.len(), 0);
}
