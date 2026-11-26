use std::collections::HashSet;

pub fn compare_tags(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let a: HashSet<&str> = a.iter().copied().collect();
    let b: HashSet<&str> = b.iter().copied().collect();
    let sorted = |it: &mut dyn Iterator<Item = &&str>| {
        let mut v: Vec<String> = it.map(|s| s.to_string()).collect();
        v.sort();
        v
    };
    (
        sorted(&mut a.intersection(&b)),
        sorted(&mut a.difference(&b)),
        sorted(&mut b.difference(&a)),
    )
}
