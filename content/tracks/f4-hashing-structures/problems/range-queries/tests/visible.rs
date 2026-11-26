use solution::*;

#[test]
fn core_sums_follow_updates() {
    let mut s = PrefixSums::new(&[3, 1, 4, 1, 5]);
    check!("(prefix 0, prefix 3, sum 1..4)", (s.prefix(0), s.prefix(3), s.sum(1, 4)), (0, 8, 6));
    s.add(2, -4);
    check!("after add(2, -4)", (s.prefix(5), s.sum(2, 3)), (10, 0));
}

#[test]
fn core_no_slots() {
    let s = PrefixSums::new(&[]);
    check!("prefix(0)", s.prefix(0), 0);
}

#[test]
fn part2_first_slot_to_reach() {
    let s = PrefixSums::new(&[2, 0, 3, 0, 1]);
    check!("targets 1..=7", (1..=7).map(|t| s.first_reaching(t)).collect::<Vec<_>>(), vec![Some(0), Some(0), Some(2), Some(2), Some(2), Some(4), None]);
}

#[derive(Clone, Debug, PartialEq)]
struct Text(String);
impl Summary for Text {
    fn empty() -> Self {
        Text(String::new())
    }
    fn join(a: &Self, b: &Self) -> Self {
        Text(format!("{}{}", a.0, b.0))
    }
}

#[test]
fn part3_order_is_kept() {
    let letters: Vec<Text> = "abcdefg".chars().map(|c| Text(c.to_string())).collect();
    let mut t = Combine::new(&letters);
    check!("query(1, 6)", t.query(1, 6).0, "bcdef".to_string());
    t.set(3, Text("D".into()));
    check!("all after set", t.query(0, 7).0, "abcDefg".to_string());
    check!("empty range", t.query(4, 4).0, String::new());
}

#[test]
fn part4_add_to_a_range() {
    let mut a = RangeAdder::new(&[1, 2, 3, 4, 5]);
    a.add_range(1, 4, 10);
    check!("(all, 0..2, 3..5)", (a.sum(0, 5), a.sum(0, 2), a.sum(3, 5)), (45, 13, 19));
}
