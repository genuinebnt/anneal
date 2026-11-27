use solution::*;
use std::collections::HashMap;

/// The remote store. It checks every saved list and counts the calls.
struct Remote {
    k: usize,
    lists: HashMap<String, Vec<(String, u64)>>,
    /// every score each name has been raised with
    raised: HashMap<String, Vec<u64>>,
    loads: u64,
    saves: u64,
    load_limit: u64,
    save_limit: u64,
}

impl Remote {
    fn new(k: usize) -> Remote {
        Remote { k, lists: HashMap::new(), raised: HashMap::new(), loads: 0, saves: 0, load_limit: u64::MAX, save_limit: u64::MAX }
    }

    #[allow(dead_code)]
    fn allow(&mut self, loads: u64, saves: u64) {
        (self.loads, self.saves) = (0, 0);
        (self.load_limit, self.save_limit) = (loads, saves);
    }

    fn raise(&mut self, changes: &[(&str, u64)]) {
        for &(name, score) in changes {
            self.raised.entry(name.to_string()).or_default().push(score);
        }
        let k = self.k;
        raise(self, k, changes);
    }

    fn best(&mut self, beginning: &str) -> Vec<String> {
        let k = self.k;
        best(self, k, beginning)
    }
}

impl Store for Remote {
    fn load(&mut self, key: &str) -> Vec<(String, u64)> {
        self.loads += 1;
        assert!(self.loads <= self.load_limit, "more than {} calls of load, which is over the budget", self.load_limit);
        self.lists.get(key).cloned().unwrap_or_default()
    }

    fn save(&mut self, key: &str, list: Vec<(String, u64)>) {
        self.saves += 1;
        assert!(self.saves <= self.save_limit, "more than {} calls of save, which is over the budget", self.save_limit);
        assert!(list.len() <= self.k, "a list of {} pairs was saved, but k is {}", list.len(), self.k);
        let mut names: Vec<&String> = list.iter().map(|(n, _)| n).collect();
        names.sort();
        names.dedup();
        assert!(names.len() == list.len(), "a saved list names a player twice: {list:?}");
        for (name, score) in &list {
            let real = self.raised.get(name).is_some_and(|scores| scores.contains(score));
            assert!(real, "({name:?}, {score}) is not a player with a score it was raised with");
        }
        self.lists.insert(key.to_string(), list);
    }
}

#[test]
fn best_first_for_each_beginning() {
    let mut r = Remote::new(2);
    r.raise(&[("ana", 5), ("andy", 9), ("bo", 7), ("anton", 6)]);
    check!("an", r.best("an"), vec!["andy".to_string(), "anton".to_string()]);
    check!("everyone", r.best(""), vec!["andy".to_string(), "bo".to_string()]);
    check!("nobody", r.best("anz"), Vec::<String>::new());
}

#[test]
fn a_score_only_goes_up() {
    let mut r = Remote::new(3);
    r.raise(&[("ana", 5), ("bo", 4)]);
    r.raise(&[("ana", 1), ("bo", 6)]);
    check!("order", r.best(""), vec!["bo".to_string(), "ana".to_string()]);
}

#[test]
fn equal_scores_by_name() {
    let mut r = Remote::new(5);
    r.raise(&[("bo", 1), ("al", 1), ("ab", 1)]);
    check!("by name", r.best(""), vec!["ab".to_string(), "al".to_string(), "bo".to_string()]);
}

#[test]
fn a_name_may_be_a_beginning_of_another() {
    let mut r = Remote::new(5);
    r.raise(&[("an", 1), ("ann", 2), ("", 0)]);
    check!("an", r.best("an"), vec!["ann".to_string(), "an".to_string()]);
    check!("all", r.best(""), vec!["ann".to_string(), "an".to_string(), String::new()]);
}

#[test]
fn room_for_nobody() {
    let mut r = Remote::new(0);
    r.raise(&[("ana", 5)]);
    check!("k = 0", r.best(""), Vec::<String>::new());
}
