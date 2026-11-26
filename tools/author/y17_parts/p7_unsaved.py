from y17_parts.common import broken

TRAIT = r'''/// A large, sorted, read-only set of entries kept elsewhere. Every call is a round trip.
pub trait Catalogue {
    fn get(&self, key: &str) -> Option<String>;
    /// The entry with the smallest key that is `key` or later.
    fn at_or_after(&self, key: &str) -> Option<(String, String)>;
    /// The entry with the smallest key later than `key`.
    fn after(&self, key: &str) -> Option<(String, String)>;
}
'''

STARTER = TRAIT + r'''
/// Changes to a catalogue that have not been sent to it yet.
pub struct Draft<C: Catalogue> {
    base: C,
}

impl<C: Catalogue> Draft<C> {
    pub fn new(base: C) -> Draft<C> {
        todo!()
    }

    pub fn set(&mut self, key: &str, value: &str) {
        todo!()
    }

    pub fn delete(&mut self, key: &str) {
        todo!()
    }

    /// Forgets the unsaved change to `key`, if there is one.
    pub fn revert(&mut self, key: &str) {
        todo!()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        todo!()
    }

    /// Up to `limit` entries as the catalogue would look after saving, from key `from` on, in key order.
    pub fn page(&self, from: &str, limit: usize) -> Vec<(String, String)> {
        todo!()
    }

    /// What saving would send, in key order: a new value, or `None` for a delete.
    pub fn changes(&self) -> Vec<(String, Option<String>)> {
        todo!()
    }
}
'''

SOLUTION = r'''use std::collections::BTreeMap;

''' + TRAIT + r'''
/// Changes to a catalogue that have not been sent to it yet.
pub struct Draft<C: Catalogue> {
    base: C,
    /// `None` is a delete. It has to be kept: it hides what the catalogue still holds.
    changes: BTreeMap<String, Option<String>>,
}

impl<C: Catalogue> Draft<C> {
    pub fn new(base: C) -> Draft<C> {
        Draft {
            base,
            changes: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.changes.insert(key.to_string(), Some(value.to_string()));
    }

    pub fn delete(&mut self, key: &str) {
        self.changes.insert(key.to_string(), None);
    }

    /// Forgets the unsaved change to `key`, if there is one.
    pub fn revert(&mut self, key: &str) {
        self.changes.remove(key);
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match self.changes.get(key) {
            Some(change) => change.clone(),
            None => self.base.get(key),
        }
    }

    /// Up to `limit` entries as the catalogue would look after saving, from key `from` on, in key order.
    pub fn page(&self, from: &str, limit: usize) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        let mut local = self
            .changes
            .range::<str, _>((std::ops::Bound::Included(from), std::ops::Bound::Unbounded))
            .peekable();
        let mut theirs = self.base.at_or_after(from);
        loop {
            // take whichever side has the smaller key; on a tie the local change wins and both move on
            let take_local = match (local.peek(), &theirs) {
                (Some((mine, _)), Some((their, _))) => *mine <= their,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => return out,
            };
            let mut used_base: Option<String> = None;
            if take_local {
                let Some((key, change)) = local.next() else {
                    return out;
                };
                if theirs.as_ref().is_some_and(|t| t.0 == *key) {
                    used_base = theirs.take().map(|t| t.0);
                }
                if let Some(value) = change {
                    out.push((key.clone(), value.clone()));
                }
            } else if let Some((key, value)) = theirs.take() {
                used_base = Some(key.clone());
                out.push((key, value));
            }
            if out.len() == limit {
                return out;
            }
            // ask for the next catalogue entry only once it is known to be needed
            if let Some(key) = used_base {
                theirs = self.base.after(&key);
            }
        }
    }

    /// What saving would send, in key order: a new value, or `None` for a delete.
    pub fn changes(&self) -> Vec<(String, Option<String>)> {
        self.changes
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + r'''
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
'''

HIDDEN = HARNESS + r'''
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
'''

PROBLEM = dict(
    slug="unsaved-changes", title="Unsaved changes over a read-only catalogue", level="medium", stage="many-layers",
    tags=["layers", "delete markers", "BTreeMap range", "peekable", "ordered merge"],
    statement=r'''
Editors change products in a shared catalogue and save at the end of the day. Until then, their screens must show the catalogue **as it will look after saving**. The catalogue is large and far away: you can only ask it for one entry at a time.

Write `Draft`, which holds the unsaved changes. It never writes to the catalogue.

- `set(key, value)` and `delete(key)` record a change. A later change to the same key replaces it. Deleting a key the catalogue does not have is still a change.
- `revert(key)` forgets the change to `key`. The catalogue's own entry shows again.
- `get(key)` is the value after saving. For a changed key it makes **no** catalogue call, and otherwise one.
- `page(from, limit)` is the first `limit` entries after saving with `key >= from`, in key order. The page is short only when there is nothing more.
- `changes()` lists every recorded change in key order, with `None` for a delete. A change is listed even if it matches the catalogue.

**Cost of a page.** Let `n` be the number of catalogue entries from `from` up to the last key you return, or all of them from `from` on if the page is short. `page` may make at most `n + 1` calls, and none when `limit` is 0. Unsaved changes beyond the page must not be walked through one by one.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    hints=[
        ("approach", "Decide what has to be remembered about a deleted key. Then picture two sorted lists, yours and the catalogue's, and how you would read the combined list from the top."),
        ("rust", "`BTreeMap::range` accepts a `(Bound, Bound)` pair; for `String` keys searched by `&str` write `range::<str, _>`. `Iterator::peekable` lets you look at your next change before taking it."),
        ("edge", "When both sides have the same key, only one row may come out, and both sides must move on. Ask the catalogue for its next entry only after you know the page is not full."),
    ],
    notes=("A delete is stored as a marker, because removing the local record would let the catalogue's entry show through. A page walks both sorted sides at once, taking the smaller key; the local change wins a tie. The next catalogue entry is requested only when another row is still needed.", "O(rows walked x log changes)", "O(changes + page)"),
    follow_up="1) Add a second draft on top of the first, such as a personal one over a team one. What has to change in your types? 2) If the catalogue changes between two pages, what can the editor see twice or miss? 3) Saving sends `changes()`. Which of them could be left out safely, and what would you need to know?",
    teaches=["When a newer layer sits over older data, a delete must be stored, not just applied.", "Two sorted layers are read together by always taking the smaller key, with the newer layer winning ties.", "Fetch lazily: ask for the next item only when it is needed."],
    wrong={
        "delete_just_drops_the_change": broken(SOLUTION, ("    pub fn delete(&mut self, key: &str) {\n        self.changes.insert(key.to_string(), None);", "    pub fn delete(&mut self, key: &str) {\n        self.changes.remove(key);")),
        "revert_deletes": broken(SOLUTION, ("    pub fn revert(&mut self, key: &str) {\n        self.changes.remove(key);", "    pub fn revert(&mut self, key: &str) {\n        self.changes.insert(key.to_string(), None);")),
        "deleted_rows_shorten_the_page": broken(SOLUTION, ("        let mut out = Vec::new();\n        if limit == 0 {", "        let mut out = Vec::new();\n        let mut walked = 0;\n        if limit == 0 {"), ("            if out.len() == limit {\n                return out;\n            }\n", "            walked += 1;\n            if walked == limit {\n                return out;\n            }\n")),
        "reads_one_ahead": broken(SOLUTION, ("            if out.len() == limit {\n                return out;\n            }\n            // ask for the next catalogue entry only once it is known to be needed\n            if let Some(key) = used_base {\n                theirs = self.base.after(&key);\n            }\n", "            if let Some(key) = used_base {\n                theirs = self.base.after(&key);\n                if theirs.is_some() && out.len() == limit {\n                    let _ = self.base.after(&key);\n                }\n            }\n            if out.len() == limit {\n                return out;\n            }\n")),
        "asks_the_catalogue_first": broken(SOLUTION, ("        match self.changes.get(key) {\n            Some(change) => change.clone(),\n            None => self.base.get(key),\n        }", "        let saved = self.base.get(key);\n        match self.changes.get(key) {\n            Some(change) => change.clone(),\n            None => saved,\n        }")),
        "a_tie_shows_both": broken(SOLUTION, ("                    used_base = theirs.take().map(|t| t.0);\n", "")),
    },
)
