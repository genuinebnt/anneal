use solution::*;

#[test]
fn pages_with_no_words_still_count() {
    let mut s = Shelf::new();
    check!("numbers", (s.add(""), s.add("--- !!"), s.add("x")), (0, 1, 2));
    check!("every page", s.find(&[], &[], &[]), vec![0, 1, 2]);
    check!("all but x", s.find(&[], &[], &["x"]), vec![0, 1]);
}

#[test]
fn letters_beyond_ascii_and_digits() {
    let mut s = Shelf::new();
    s.add("Café ÉCOLE 3D");
    s.add("cafe");
    check!("CAFÉ", s.pages_with("CAFÉ"), vec![0]);
    check!("école", s.pages_with("école"), vec![0]);
    check!("3d", s.find(&["3D"], &[], &[]), vec![0]);
    check!("cafe is another word", s.pages_with("cafe"), vec![1]);
}

#[test]
fn the_same_word_twice_and_in_two_places() {
    let mut s = Shelf::new();
    s.add("a b");
    s.add("a");
    check!("all: a, a", s.find(&["a", "A"], &[], &[]), vec![0, 1]);
    check!("any: b, b", s.find(&[], &["b", "b"], &[]), vec![0]);
    check!("all and none", s.find(&["a"], &[], &["a"]), Vec::<u32>::new());
}

#[test]
fn adding_after_asking() {
    let mut s = Shelf::new();
    s.add("red");
    check!("before", s.pages_with("red"), vec![0]);
    s.add("red green");
    check!("after", s.pages_with("red"), vec![0, 1]);
}

#[test]
fn matches_reading_every_page() {
    let mut rng = anneal_prelude::Rng::new(5101);
    let vocab = ["a", "b", "c", "d", "e", "f"];
    for _ in 0..150 {
        let mut s = Shelf::new();
        let mut pages: Vec<Vec<&str>> = Vec::new();
        for _ in 0..rng.below(12) {
            let words: Vec<&str> = (0..rng.below(7)).map(|_| *rng.pick(&vocab)).collect();
            s.add(&words.join(", "));
            pages.push(words);
        }
        for _ in 0..12 {
            let mut ask = || -> Vec<&str> { (0..rng.below(3)).map(|_| *rng.pick(&vocab)).collect() };
            let (all, any, none) = (ask(), ask(), ask());
            let want: Vec<u32> = (0..pages.len())
                .filter(|&i| {
                    let p = &pages[i];
                    all.iter().all(|w| p.contains(w))
                        && (any.is_empty() || any.iter().any(|w| p.contains(w)))
                        && !none.iter().any(|w| p.contains(w))
                })
                .map(|i| i as u32)
                .collect();
            check!(format!("pages {pages:?}, all {all:?}, any {any:?}, none {none:?}"), s.find(&all, &any, &none), want);
        }
    }
}

#[test]
fn many_pages_with_one_rare_word() {
    let mut s = Shelf::new();
    for i in 0..2_000u32 {
        let mut text = String::from("the guide to setup");
        if i % 500 == 7 {
            text.push_str(" zebra");
        }
        s.add(&text);
    }
    check!("with the rare word", s.find(&["the", "guide", "zebra"], &["setup", "to"], &["lion"]), vec![7, 507, 1007, 1507]);
    check!("and excluding a common one", s.find(&["setup", "zebra"], &[], &["the"]), Vec::<u32>::new());
}

#[test]
fn any_alone_gives_each_page_once_in_order() {
    let mut s = Shelf::new();
    for text in ["b c", "a", "a b", "c", "a b c"] {
        s.add(text);
    }
    check!("any: c, a, b", s.find(&[], &["c", "a", "b"], &[]), vec![0, 1, 2, 3, 4]);
    check!("any: b, a; none: c", s.find(&[], &["b", "a"], &["c"]), vec![1, 2]);
}

#[test]
fn none_alone() {
    let mut s = Shelf::new();
    for text in ["a", "b", "a b", ""] {
        s.add(text);
    }
    check!("none: a, b", s.find(&[], &[], &["a", "b"]), vec![3]);
}

/// A list whose number at place i is `i * step + shift`, or one given in full. It counts reads and stops at the budget.
struct Counted {
    len: usize,
    step: u32,
    shift: u32,
    given: Option<Vec<u32>>,
    reads: std::rc::Rc<std::cell::Cell<u64>>,
    budget: u64,
}

impl PageList for Counted {
    fn len(&self) -> usize {
        self.len
    }
    fn get(&self, i: usize) -> u32 {
        assert!(i < self.len, "get({i}) on a list of {}", self.len);
        self.reads.set(self.reads.get() + 1);
        assert!(self.reads.get() <= self.budget, "more than the budget of {} reads", self.budget);
        match &self.given {
            Some(numbers) => numbers[i],
            None => i as u32 * self.step + self.shift,
        }
    }
}

enum Spec {
    Steps(usize, u32, u32),
    Given(Vec<u32>),
}

fn run(specs: Vec<Spec>) -> Vec<u32> {
    let lengths: Vec<usize> = specs.iter().map(|s| match s { Spec::Steps(n, _, _) => *n, Spec::Given(v) => v.len() }).collect();
    let shortest = lengths.iter().copied().min().unwrap_or(0);
    let longest = lengths.iter().copied().max().unwrap_or(0);
    let bits = (usize::BITS - longest.leading_zeros()) as u64;
    let budget = lengths.len() as u64 * (shortest as u64 + 1) * (2 * bits + 4);
    let reads = std::rc::Rc::new(std::cell::Cell::new(0));
    let lists: Vec<Counted> = specs
        .into_iter()
        .zip(lengths)
        .map(|(spec, len)| {
            let (step, shift, given) = match spec {
                Spec::Steps(_, step, shift) => (step, shift, None),
                Spec::Given(v) => (0, 0, Some(v)),
            };
            Counted { len, step, shift, given, reads: reads.clone(), budget }
        })
        .collect();
    let refs: Vec<&dyn PageList> = lists.iter().map(|l| l as &dyn PageList).collect();
    in_every_list(&refs)
}

#[test]
fn counted_lists_of_every_small_shape() {
    let mut rng = anneal_prelude::Rng::new(5111);
    for _ in 0..2_000 {
        let w = rng.below(5);
        let lists: Vec<Vec<u32>> = (0..w)
            .map(|_| {
                let keep = 1 + rng.below(4);
                let top = rng.below(60) as u32;
                (0..top).filter(|_| rng.below(4) < keep).collect()
            })
            .collect();
        let want: Vec<u32> = match lists.first() {
            Some(first) => first.iter().copied().filter(|n| lists.iter().all(|l| l.contains(n))).collect(),
            None => Vec::new(),
        };
        let got = run(lists.iter().cloned().map(Spec::Given).collect());
        check!(format!("lists {lists:?}"), got, want);
    }
}

#[test]
fn a_short_list_in_any_place_among_enormous_ones() {
    let big = 1_000_000_000;
    for place in 0..4 {
        let mut specs = vec![Spec::Steps(big, 1, 0), Spec::Steps(big, 2, 0), Spec::Steps(big, 1, 0)];
        specs.insert(place, Spec::Given(vec![0, 7, 500_000_000, 999_999_998, 1_999_999_999]));
        check!(format!("the short list in place {place}"), run(specs), vec![0, 500_000_000, 999_999_998]);
    }
}

#[test]
fn an_empty_list_and_a_list_that_ends_early() {
    check!("one empty", run(vec![Spec::Steps(1_000_000_000, 1, 0), Spec::Given(vec![])]), Vec::<u32>::new());
    check!("the long one ends first", run(vec![Spec::Steps(1_000, 1, 0), Spec::Given(vec![5, 999, 1_000, 5_000])]), vec![5, 999]);
    check!("no lists", run(vec![]), Vec::<u32>::new());
}

#[test]
fn many_lists_and_equal_lengths() {
    let specs: Vec<Spec> = (0..12).map(|_| Spec::Steps(3_000, 3, 0)).collect();
    check!("twelve equal lists", run(specs).len(), 3_000);
    let got = run(vec![Spec::Steps(2_000, 2, 0), Spec::Steps(2_000, 3, 0), Spec::Steps(100_000, 1, 0)]);
    check!("multiples of 6", got, (0..4_000).step_by(6).collect::<Vec<u32>>());
}
