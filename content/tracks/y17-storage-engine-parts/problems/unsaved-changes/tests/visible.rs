use solution::*;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::rc::Rc;

struct Remote {
    rows: BTreeMap<String, String>,
    calls: Rc<Cell<usize>>,
}

impl Remote {
    fn hit(&self) {
        self.calls.set(self.calls.get() + 1);
    }
    fn first(&self, bound: Bound<&str>) -> Option<(String, String)> {
        self.hit();
        self.rows.range::<str, _>((bound, Bound::Unbounded)).next().map(|(k, v)| (k.clone(), v.clone()))
    }
}

impl Catalogue for Remote {
    fn get(&self, key: &str) -> Option<String> {
        self.hit();
        self.rows.get(key).cloned()
    }
    fn at_or_after(&self, key: &str) -> Option<(String, String)> {
        self.first(Bound::Included(key))
    }
    fn after(&self, key: &str) -> Option<(String, String)> {
        self.first(Bound::Excluded(key))
    }
}

#[allow(dead_code)]
fn draft(rows: &[(&str, &str)]) -> (Draft<Remote>, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let rows = rows.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    (Draft::new(Remote { rows, calls: calls.clone() }), calls)
}

#[allow(dead_code)]
fn rows(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn a_page_shows_the_changes() {
    let (mut d, _) = draft(&[("apple", "1"), ("cherry", "3"), ("plum", "4")]);
    d.set("banana", "2");
    d.set("plum", "40");
    d.delete("cherry");
    check!("set banana and plum, delete cherry", d.page("", 10), rows(&[("apple", "1"), ("banana", "2"), ("plum", "40")]));
}

#[test]
fn a_delete_hides_the_catalogue_entry() {
    let (mut d, _) = draft(&[("a", "1")]);
    d.delete("a");
    check!("delete a", (d.get("a"), d.changes()), (None, vec![("a".to_string(), None)]));
}

#[test]
fn revert_is_not_delete() {
    let (mut d, _) = draft(&[("a", "1")]);
    d.set("a", "2");
    d.revert("a");
    check!("set a, revert a", (d.get("a"), d.changes().len()), (Some("1".to_string()), 0));
}

#[test]
fn a_page_is_full_even_when_rows_are_deleted() {
    let (mut d, _) = draft(&[("a", "1"), ("b", "2"), ("c", "3"), ("d", "4")]);
    d.delete("a");
    d.delete("b");
    check!("delete a and b, two rows from the start", d.page("a", 2), rows(&[("c", "3"), ("d", "4")]));
}

#[test]
fn a_page_does_not_read_ahead() {
    let (d, calls) = draft(&[("a", "1"), ("b", "2"), ("c", "3"), ("d", "4"), ("e", "5")]);
    let got = d.page("b", 2);
    check!("two rows from b: at most 3 calls", (got, calls.get() <= 3), (rows(&[("b", "2"), ("c", "3")]), true));
}
