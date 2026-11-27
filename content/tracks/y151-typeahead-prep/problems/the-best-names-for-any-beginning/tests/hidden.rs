use solution::*;
use std::collections::{BTreeMap, HashMap};

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

/// The suggested names, joined by commas.
fn joined(list: Vec<String>) -> String {
    list.join(", ")
}

/// Every player in a map; each question sorts the ones that fit.
#[derive(Default)]
struct Slow(BTreeMap<String, u64>);

impl Slow {
    fn raise(&mut self, name: &str, score: u64) {
        let held = self.0.entry(name.to_string()).or_insert(score);
        *held = (*held).max(score);
    }
    fn best(&self, beginning: &str, k: usize) -> Vec<String> {
        let mut fit: Vec<(&String, u64)> =
            self.0.iter().filter(|(n, _)| n.starts_with(beginning)).map(|(n, &s)| (n, s)).collect();
        fit.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        fit.into_iter().take(k).map(|(n, _)| n.clone()).collect()
    }
}

fn word(rng: &mut anneal_prelude::Rng, letters: &[char], max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(letters)).collect()
}

#[test]
fn one_list_that_moves_several_players() {
    let mut r = Remote::new(10);
    r.raise(&[("za", 100), ("zb", 90), ("zc", 80), ("zd", 70)]);
    r.raise(&[("ze", 95), ("zc", 1000)]);
    check!("z", joined(r.best("z")), "zc, za, ze, zb, zd".to_string());
}

#[test]
fn one_player_many_times_in_one_list() {
    let mut r = Remote::new(3);
    r.raise(&[("a", 1), ("b", 5), ("a", 9), ("a", 2), ("c", 7), ("b", 8)]);
    check!("all", joined(r.best("")), "a, b, c".to_string());
}

#[test]
fn someone_below_the_list_climbs_back_in() {
    let mut r = Remote::new(2);
    r.raise(&[("p1", 10), ("p2", 20), ("p3", 30)]);
    check!("p1 is out", joined(r.best("p")), "p3, p2".to_string());
    r.raise(&[("p1", 5)]);
    check!("a lower score does not bring it back", joined(r.best("p")), "p3, p2".to_string());
    r.raise(&[("p1", 40)]);
    check!("p1 is back", joined(r.best("p")), "p1, p3".to_string());
}

#[test]
fn a_huge_k_reserves_nothing() {
    let mut r = Remote::new(usize::MAX);
    r.raise(&[("b", 1), ("a", 2)]);
    check!("all", joined(r.best("")), "a, b".to_string());
}

#[test]
fn names_in_other_scripts() {
    let mut r = Remote::new(4);
    r.raise(&[("東京", 2), ("東", 1), ("éa", 3), ("e\u{301}a", 4)]);
    check!("東", joined(r.best("東")), "東京, 東".to_string());
    check!("é is not e with a mark", joined(r.best("é")), "éa".to_string());
}

#[test]
fn two_stores_do_not_mix() {
    let (mut a, mut b) = (Remote::new(3), Remote::new(3));
    a.raise(&[("ana", 5)]);
    b.raise(&[("bo", 7)]);
    a.raise(&[("al", 1)]);
    check!("a", joined(a.best("")), "ana, al".to_string());
    check!("b", joined(b.best("")), "bo".to_string());
}

#[test]
fn an_equal_score_changes_nothing_and_asking_changes_nothing() {
    let mut r = Remote::new(3);
    check!("empty store", r.best(""), Vec::<String>::new());
    r.raise(&[]);
    r.raise(&[("b", 4), ("a", 4)]);
    r.raise(&[("b", 4)]);
    check!("first", joined(r.best("")), "a, b".to_string());
    check!("again", joined(r.best("")), "a, b".to_string());
}

#[test]
fn a_long_beginning_has_its_own_best() {
    let mut r = Remote::new(3);
    let mut slow = Slow::default();
    let stem = "the same long beginning for every one of them ";
    // twenty strong players share all but the last part of the name with three weak ones
    let mut names: Vec<(String, u64)> = (0..20).map(|i| (format!("{stem}strong {i:02}"), 1000 + i)).collect();
    names.extend((0..3).map(|i| (format!("{stem}weak {i}"), i)));
    for (name, score) in &names {
        r.raise(&[(name.as_str(), *score)]);
        slow.raise(name, *score);
    }
    for end in 0..=stem.len() + 6 {
        let beginning = format!("{stem}weak 1");
        let beginning = &beginning[..end.min(beginning.len())];
        check!(format!("best({beginning:?})"), r.best(beginning), slow.best(beginning, 3));
    }
}

#[test]
fn agrees_with_sorting_everyone() {
    let mut rng = anneal_prelude::Rng::new(15107);
    let letters = ['a', 'b', 'é'];
    for round in 0..400 {
        let k = rng.below(5);
        // short names that collide often, then long ones with long shared beginnings
        let (longest, letters): (usize, &[char]) = if round % 2 == 0 { (3, &letters) } else { (9, &letters[..2]) };
        let mut r = Remote::new(k);
        let mut slow = Slow::default();
        for _ in 0..rng.below(12) {
            let n = rng.below(8);
            let batch: Vec<(String, u64)> = (0..n).map(|_| (word(&mut rng, letters, longest), rng.below(12) as u64)).collect();
            let changes: Vec<(&str, u64)> = batch.iter().map(|(n, s)| (n.as_str(), *s)).collect();
            r.raise(&changes);
            for (name, score) in &batch {
                slow.raise(name, *score);
            }
            for _ in 0..4 {
                let beginning = word(&mut rng, letters, longest);
                check!(format!("k {k}, after {batch:?}, best({beginning:?})"), r.best(&beginning), slow.best(&beginning, k));
            }
        }
    }
}

#[test]
fn five_thousand_players_within_the_budget() {
    let mut rng = anneal_prelude::Rng::new(15108);
    let letters = ['a', 'b', 'c', 'd', 'e', 'f'];
    let mut r = Remote::new(10);
    let mut slow = Slow::default();
    let mut names: Vec<String> = Vec::new();
    while names.len() < 5000 {
        let name = format!("{}{}", word(&mut rng, &letters, 6), word(&mut rng, &letters, 6));
        if !slow.0.contains_key(&name) {
            slow.raise(&name, 0);
            names.push(name);
        }
    }
    slow.0.clear();
    for round in 0..900 {
        // the first 500 calls add everyone, ten at a time; the rest raise known players
        let batch: Vec<(String, u64)> = (0..10)
            .map(|i| {
                let name = if round < 500 { names[round * 10 + i].clone() } else { names[rng.below(5000)].clone() };
                (name, rng.below(1_000_000) as u64)
            })
            .collect();
        let limit: u64 = batch.iter().map(|(n, _)| 4 * (n.chars().count() as u64 + 1)).sum();
        let changes: Vec<(&str, u64)> = batch.iter().map(|(n, s)| (n.as_str(), *s)).collect();
        r.allow(limit, limit);
        r.raise(&changes);
        for (name, score) in &batch {
            slow.raise(name, *score);
        }
        if round % 30 == 29 {
            let mut beginnings: Vec<String> = ["", "a", "ab", "fff", "zz"].iter().map(|b| b.to_string()).collect();
            // longer beginnings of players who are there, up to a whole name
            let known: Vec<&String> = slow.0.keys().collect();
            for length in 4..=12 {
                let name = known[rng.below(known.len())];
                beginnings.push(name.chars().take(length).collect());
            }
            for beginning in &beginnings {
                r.allow(8, 0);
                let got = r.best(beginning);
                check!(format!("best({beginning:?}) after {round} lists"), got, slow.best(beginning, 10));
            }
        }
    }
}
