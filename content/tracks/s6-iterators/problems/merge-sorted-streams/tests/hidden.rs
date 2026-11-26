use solution::*;

use std::cell::Cell;
use std::rc::Rc;

/// Counts pulls, panics if asked again after `None`, and panics rather than run for ever.
struct Spy<I> {
    inner: I,
    pulls: Rc<Cell<usize>>,
    done: bool,
}

impl<I: Iterator> Iterator for Spy<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        assert!(!self.done, "a finished source was asked again");
        self.pulls.set(self.pulls.get() + 1);
        assert!(self.pulls.get() <= 10_000, "a source was pulled more than 10 000 times: the merge is not lazy");
        let item = self.inner.next();
        self.done = item.is_none();
        item
    }
}

#[allow(dead_code)]
fn spy<I: Iterator>(inner: I) -> (Spy<I>, Rc<Cell<usize>>) {
    let pulls = Rc::new(Cell::new(0));
    (Spy { inner, pulls: pulls.clone(), done: false }, pulls)
}

#[test]
fn building_pulls_nothing() {
    let (a, pa) = spy(0..5);
    let (b, pb) = spy(0..5);
    let _m = merge([a, b]);
    check!("pulls after merge() alone", (pa.get(), pb.get()), (0, 0));
}

#[test]
fn at_most_one_item_ahead() {
    let (a, pa) = spy(0u64..);
    let (b, pb) = spy(1000u64..);
    let mut m = merge([a, b]);
    for _ in 0..50 {
        m.next();
    }
    check!("after 50 items, all from the first source", (pa.get() <= 51, pb.get() <= 1), (true, true));
}

#[test]
fn finished_sources_are_left_alone() {
    let (a, _) = spy(0..1);
    let (b, _) = spy(0..4);
    let mut m = merge([a, b]);
    let got: Vec<i32> = m.by_ref().collect();
    check!("a short and a long source", got, vec![0, 0, 1, 2, 3]);
    check!("next after the end", (m.next(), m.next()), (None, None));
}

#[test]
fn items_need_no_clone() {
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Token(u32);
    let got: Vec<Token> = merge([vec![Token(2)], vec![Token(1), Token(3)]]).collect();
    check!("a type with no Clone", got, vec![Token(1), Token(2), Token(3)]);
}

#[test]
fn accepts_any_into_iterator() {
    let a = std::collections::BTreeSet::from([5, 1]);
    let b = std::collections::BTreeSet::from([3]);
    check!("two BTreeSets", merge([a, b]).collect::<Vec<_>>(), vec![1, 3, 5]);
}

#[test]
fn size_hint_without_an_upper_bound() {
    let sources: [Box<dyn Iterator<Item = u32>>; 2] = [Box::new(0u32..10), Box::new(std::iter::repeat(7))];
    let m = merge(sources);
    check!("10 items plus an endless source", m.size_hint(), (usize::MAX, None));
}

#[test]
fn size_hint_with_unknown_lower_bounds() {
    fn even(x: &i32) -> bool {
        x % 2 == 0
    }
    let sources = [(0..10).filter(even as fn(&i32) -> bool), (0..4).filter(even as fn(&i32) -> bool)];
    let mut m = merge(sources);
    check!("two filters before the first next", m.size_hint(), (0, Some(14)));
    m.next();
    let (low, high) = m.size_hint();
    check!("bounds stay true after one item (7 remain)", (low <= 7, high.is_none_or(|h| h >= 7)), (true, true));
}

/// Counts every comparison made between items.
#[derive(Debug, Clone)]
struct Counted(u32, Rc<Cell<u64>>);
impl PartialEq for Counted {
    fn eq(&self, o: &Self) -> bool { self.cmp(o) == std::cmp::Ordering::Equal }
}
impl Eq for Counted {}
impl PartialOrd for Counted {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) }
}
impl Ord for Counted {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.1.set(self.1.get() + 1);
        self.0.cmp(&o.0)
    }
}

#[test]
fn comparisons_grow_with_log_k() {
    let comparisons = Rc::new(Cell::new(0));
    let k = 512u32;
    let sources: Vec<Vec<Counted>> = (0..k).map(|i| (0..40).map(|j| Counted(j * k + (i * 7919) % k, comparisons.clone())).collect()).collect();
    let n = u64::from(k) * 40;
    let count = merge(sources).count() as u64;
    check!("items out", count, n);
    let per_item = comparisons.get() / n;
    check!(format!("512 sources: {per_item} comparisons per item, at most 45"), per_item <= 45, true);
}

#[test]
fn random_lists_match_a_stable_sort() {
    let mut rng = anneal_prelude::Rng::new(8112);
    for _ in 0..400 {
        let k = rng.below(7);
        let lists: Vec<Vec<(u8, usize)>> = (0..k)
            .map(|i| {
                let len = rng.below(8);
                let mut v: Vec<u8> = rng.vec(len, 0, 5);
                v.sort();
                v.into_iter().map(|x| (x, i)).collect()
            })
            .collect();
        // Sorting by (value, source) is exactly the stable merge.
        let mut want: Vec<(u8, usize)> = lists.concat();
        want.sort();
        let got: Vec<(u8, usize)> = merge(lists.clone()).collect();
        check!(format!("{lists:?}"), got, want);
    }
}
