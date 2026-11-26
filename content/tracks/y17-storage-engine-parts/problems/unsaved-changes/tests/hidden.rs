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
fn nothing_anywhere() {
    let (d, _) = draft(&[]);
    check!("empty", (d.get("x"), d.page("", 5), d.changes()), (None, vec![], vec![]));
}

#[test]
fn a_page_of_nothing_asks_nothing() {
    let (d, calls) = draft(&[("a", "1")]);
    check!("limit 0", (d.page("", 0), calls.get()), (vec![], 0));
}

#[test]
fn a_changed_key_needs_no_call() {
    let (mut d, calls) = draft(&[("a", "1")]);
    d.set("b", "2");
    d.delete("a");
    let got = (d.get("a"), d.get("b"));
    check!("get of two changed keys", (got, calls.get()), ((None, Some("2".to_string())), 0));
    let got = d.get("zz");
    check!("an unchanged key: one call", (got, calls.get()), (None, 1));
}

#[test]
fn deleting_what_is_not_there() {
    let (mut d, _) = draft(&[("a", "1")]);
    d.delete("ghost");
    d.revert("never-touched");
    check!("page", d.page("", 9), rows(&[("a", "1")]));
    check!("the delete is still to be sent", d.changes(), vec![("ghost".to_string(), None)]);
}

#[test]
fn later_changes_replace_earlier_ones() {
    let (mut d, _) = draft(&[("k", "base")]);
    d.delete("k");
    d.set("k", "again");
    check!("delete then set", d.get("k"), Some("again".to_string()));
    d.delete("k");
    check!("then delete", (d.page("", 9), d.changes()), (vec![], vec![("k".to_string(), None)]));
    d.revert("k");
    check!("then revert", d.page("", 9), rows(&[("k", "base")]));
}

#[test]
fn from_is_included_and_need_not_exist() {
    let (mut d, _) = draft(&[("b", "1"), ("d", "2")]);
    d.set("c", "new");
    check!("from b", d.page("b", 1), rows(&[("b", "1")]));
    check!("from bb", d.page("bb", 9), rows(&[("c", "new"), ("d", "2")]));
    check!("from e", d.page("e", 9), vec![]);
}

#[test]
fn only_the_catalogue_rows_on_the_page_are_fetched() {
    let names: Vec<String> = (0..10_000).map(|i| format!("k{i:05}")).collect();
    let base: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "v")).collect();
    let (mut d, calls) = draft(&base);
    for i in (101..130).step_by(2) {
        d.delete(&names[i]);
    }
    d.set("k00100a", "x");
    d.set("zzz", "far away");
    let got = d.page("k00100", 20);
    let last = got.last().map(|r| r.0.clone()).unwrap_or_default();
    let walked = names.iter().filter(|n| n.as_str() >= "k00100" && **n <= last).count();
    check!(format!("20 rows from k00100 end at {last}; {walked} catalogue rows lie in between; calls {}", calls.get()), (got.len(), calls.get() <= walked + 1), (20, true));
}

#[test]
fn the_end_of_the_catalogue() {
    let (mut d, calls) = draft(&[("a", "1"), ("b", "2")]);
    d.set("c", "3");
    let got = d.page("a", 100);
    check!(format!("all rows; calls {}", calls.get()), (got.len(), calls.get() <= 3), (3, true));
}

#[test]
fn random_drafts_match_a_saved_copy() {
    let mut rng = anneal_prelude::Rng::new(170701);
    for case in 0..500 {
        let mut base = BTreeMap::new();
        let n = rng.below(12);
        for _ in 0..n {
            let len = rng.below(3);
            base.insert(rng.string(len, "abc"), "base".to_string());
        }
        let calls = Rc::new(Cell::new(0));
        let mut d = Draft::new(Remote { rows: base.clone(), calls: calls.clone() });
        let mut changes: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut story = format!("case {case}: base {:?}", base.keys().collect::<Vec<_>>());
        for step in 0..14 {
            let len = rng.below(3);
            let key = rng.string(len, "abc");
            match rng.below(5) {
                0 => {
                    d.set(&key, &step.to_string());
                    changes.insert(key.clone(), Some(step.to_string()));
                    story.push_str(&format!(", set {key:?}"));
                }
                1 => {
                    d.delete(&key);
                    changes.insert(key.clone(), None);
                    story.push_str(&format!(", delete {key:?}"));
                }
                2 => {
                    d.revert(&key);
                    changes.remove(&key);
                    story.push_str(&format!(", revert {key:?}"));
                }
                _ => {}
            }
            let mut saved = base.clone();
            for (k, v) in &changes {
                match v {
                    Some(v) => saved.insert(k.clone(), v.clone()),
                    None => saved.remove(k),
                };
            }
            let flen = rng.below(3);
            let from = rng.string(flen, "abc");
            let limit = rng.below(6);
            check!(format!("{story}; get {from:?}"), d.get(&from), saved.get(&from).cloned());
            calls.set(0);
            let want: Vec<(String, String)> = saved.iter().filter(|(k, _)| **k >= from).take(limit).map(|(k, v)| (k.clone(), v.clone())).collect();
            let got = d.page(&from, limit);
            let end = if got.len() == limit { got.last().map(|r| r.0.clone()) } else { None };
            let between = base.keys().filter(|k| **k >= from && end.as_ref().is_none_or(|e| *k <= e)).count();
            let allowed = if limit == 0 { 0 } else { between + 1 };
            check!(format!("{story}; page({from:?}, {limit}) with {} calls, {allowed} allowed", calls.get()), (got, calls.get() <= allowed), (want, true));
            check!(format!("{story}; changes"), d.changes(), changes.clone().into_iter().collect::<Vec<_>>());
        }
    }
}
