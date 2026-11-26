use solution::*;

#[test]
fn merges_three_lists() {
    let got: Vec<i32> = merge([vec![1, 4, 9], vec![2, 3], vec![0, 10]]).collect();
    check!("[1,4,9] [2,3] [0,10]", got, vec![0, 1, 2, 3, 4, 9, 10]);
}

#[test]
fn nothing_to_merge() {
    check!("no sources", merge(Vec::<Vec<u8>>::new()).next(), None);
    check!("two empty sources", merge([Vec::<u8>::new(), Vec::new()]).next(), None);
}

#[test]
fn infinite_sources() {
    let evens = (0u64..).step_by(2);
    let odds = (1u64..).step_by(2);
    let got: Vec<u64> = merge([evens, odds]).take(6).collect();
    check!("evens and odds, first 6", got, vec![0, 1, 2, 3, 4, 5]);
}

/// Equal by `key` only, so the test can see which source an item came from.
#[derive(Debug)]
struct Tagged(u8, &'static str);
impl PartialEq for Tagged {
    fn eq(&self, o: &Self) -> bool { self.0 == o.0 }
}
impl Eq for Tagged {}
impl PartialOrd for Tagged {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) }
}
impl Ord for Tagged {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering { self.0.cmp(&o.0) }
}

#[test]
fn ties_come_out_in_source_order() {
    let got: Vec<&str> = merge([vec![Tagged(1, "a1"), Tagged(2, "a2")], vec![Tagged(1, "b1"), Tagged(1, "b2")], vec![Tagged(1, "c1")]]).map(|t| t.1).collect();
    check!("equal keys across three sources", got, vec!["a1", "b1", "b2", "c1", "a2"]);
}

#[test]
fn size_hint_is_exact_for_vectors() {
    let mut m = merge([vec![1, 2, 3], vec![4, 5]]);
    check!("before the first next", m.size_hint(), (5, Some(5)));
    m.next();
    check!("after one next", m.size_hint(), (4, Some(4)));
}
