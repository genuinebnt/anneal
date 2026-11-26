use solution::*;

use std::collections::{BTreeMap, BTreeSet};

#[allow(dead_code)]
fn with(pairs: &[(&str, i64)]) -> Db {
    let mut db = Db::new();
    let t = db.begin();
    for &(k, v) in pairs {
        db.put(t, k, v).unwrap();
    }
    db.commit(t).unwrap();
    db
}

#[allow(dead_code)]
fn commit_one(db: &mut Db, key: &str, value: Option<i64>) {
    let t = db.begin();
    match value {
        Some(v) => db.put(t, key, v).unwrap(),
        None => db.delete(t, key).unwrap(),
    }
    db.commit(t).unwrap();
}

#[test]
fn finished_transactions_reject_everything() {
    let mut db = Db::new();
    let t = db.begin();
    db.abort(t).unwrap();
    use TxError::NotActive;
    check!("get, put, delete, commit, abort after abort", (db.get(t, "x"), db.put(t, "x", 1), db.delete(t, "x"), db.commit(t), db.abort(t)), (Err(NotActive), Err(NotActive), Err(NotActive), Err(NotActive), Err(NotActive)));
    check!("an id never begun", db.get(TxId(999_999), "x"), Err(NotActive));
}

#[test]
fn ids_are_not_reused() {
    let mut db = Db::new();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..20 {
        let t = db.begin();
        check!(format!("{t:?} is new"), seen.insert(t), true);
        db.commit(t).unwrap();
    }
}

#[test]
fn abort_leaves_no_trace() {
    let mut db = with(&[("x", 1)]);
    let (a, b) = (db.begin(), db.begin());
    db.put(a, "x", 2).unwrap();
    db.put(b, "x", 3).unwrap();
    db.abort(a).unwrap();
    check!("b commits after a aborts", (db.commit(b), db.versions()), (Ok(()), 2));
}

#[test]
fn conflict_even_if_the_other_began_later() {
    let mut db = Db::new();
    let early = db.begin();
    db.put(early, "x", 1).unwrap();
    commit_one(&mut db, "x", Some(2));
    check!("an earlier transaction loses to a later, faster one", db.commit(early), Err(TxError::Conflict));
}

#[test]
fn no_conflict_with_commits_before_the_start() {
    let mut db = with(&[("x", 1)]);
    let t = db.begin();
    db.put(t, "x", 2).unwrap();
    check!("the only other commit happened before begin", db.commit(t), Ok(()));
}

#[test]
fn read_only_transactions_always_commit() {
    let mut db = with(&[("x", 1)]);
    let t = db.begin();
    db.get(t, "x").unwrap();
    commit_one(&mut db, "x", Some(9));
    check!("read, then another commit", db.commit(t), Ok(()));
}

#[test]
fn deletes_are_writes() {
    let mut db = with(&[("x", 1)]);
    let (a, b) = (db.begin(), db.begin());
    db.delete(a, "x").unwrap();
    check!("own delete is visible", db.get(a, "x"), Ok(None));
    db.put(b, "x", 2).unwrap();
    db.commit(a).unwrap();
    check!("put loses to a committed delete", db.commit(b), Err(TxError::Conflict));
}

#[test]
fn deleting_a_missing_key_conflicts_too() {
    let mut db = Db::new();
    let t = db.begin();
    db.put(t, "ghost", 1).unwrap();
    commit_one(&mut db, "ghost", None);
    check!("against a delete of a key that never existed", db.commit(t), Err(TxError::Conflict));
}

#[test]
fn vacuum_respects_old_snapshots() {
    let mut db = with(&[("x", 1)]);
    commit_one(&mut db, "x", Some(2));
    let old = db.begin();
    commit_one(&mut db, "x", Some(3));
    commit_one(&mut db, "x", Some(4));
    check!("removed while a snapshot sees 2", db.vacuum(), 1);
    check!("it still sees 2", db.get(old, "x"), Ok(Some(2)));
    db.abort(old).unwrap();
    check!("removed afterwards", (db.vacuum(), db.versions()), (2, 1));
}

#[test]
fn vacuum_removes_a_dead_key_entirely() {
    let mut db = with(&[("x", 1)]);
    commit_one(&mut db, "x", None);
    check!("a deleted key leaves nothing", (db.vacuum(), db.versions()), (2, 0));
}

#[test]
fn vacuum_keeps_a_newer_delete_and_the_value_under_it() {
    let mut db = with(&[("x", 1)]);
    let old = db.begin();
    commit_one(&mut db, "x", None);
    check!("nothing can go while the old snapshot sees 1", (db.vacuum(), db.get(old, "x")), (0, Ok(Some(1))));
}

#[test]
fn vacuum_does_not_hide_conflicts() {
    let mut db = Db::new();
    let t = db.begin();
    db.put(t, "k", 1).unwrap();
    commit_one(&mut db, "k", None);
    db.vacuum();
    check!("a delete newer than an active snapshot must survive vacuum", db.commit(t), Err(TxError::Conflict));
}

/// The obvious model: every transaction clones the whole database when it begins.
#[derive(Default)]
struct Model {
    data: BTreeMap<String, i64>,
    /// `(commit number, key, value)` of every committed write.
    log: Vec<(u64, String, Option<i64>)>,
    commits: u64,
}

struct ModelTx {
    id: TxId,
    start: u64,
    view: BTreeMap<String, i64>,
    wrote: BTreeMap<String, Option<i64>>,
}

impl Model {
    /// How many versions must remain after a vacuum, by the rule in the statement.
    fn expected_versions(&self, horizon: u64) -> usize {
        let keys: BTreeSet<&String> = self.log.iter().map(|e| &e.1).collect();
        keys.into_iter()
            .map(|k| {
                let newer = self.log.iter().filter(|e| &e.1 == k && e.0 > horizon).count();
                let under = self.log.iter().rfind(|e| &e.1 == k && e.0 <= horizon);
                newer + usize::from(under.is_some_and(|e| e.2.is_some()))
            })
            .sum()
    }
}

#[test]
fn random_histories_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(8120);
    for _ in 0..400 {
        let mut db = Db::new();
        let mut model = Model::default();
        let mut open: Vec<ModelTx> = Vec::new();
        let mut trace = Vec::new();
        for _ in 0..40 {
            let key = *rng.pick(&["a", "b", "c"]);
            let action = rng.below(8);
            if open.is_empty() || (action == 0 && open.len() < 4) {
                let id = db.begin();
                trace.push(format!("begin {}", id.0));
                open.push(ModelTx { id, start: model.commits, view: model.data.clone(), wrote: BTreeMap::new() });
                continue;
            }
            let i = rng.below(open.len());
            let id = open[i].id;
            match action {
                0..=2 => {
                    trace.push(format!("{} get {key}", id.0));
                    let want = match open[i].wrote.get(key) {
                        Some(v) => *v,
                        None => open[i].view.get(key).copied(),
                    };
                    check!(format!("{trace:?}"), db.get(id, key), Ok(want));
                }
                3 | 4 => {
                    let value = (action == 3).then(|| rng.int(0, 9));
                    trace.push(format!("{} write {key} {value:?}", id.0));
                    let got = match value {
                        Some(v) => db.put(id, key, v),
                        None => db.delete(id, key),
                    };
                    check!(format!("{trace:?}"), got, Ok(()));
                    open[i].wrote.insert(key.to_string(), value);
                }
                5 | 6 => {
                    let t = open.remove(i);
                    trace.push(format!("{} commit", id.0));
                    let lost = model.log.iter().any(|(at, k, _)| *at > t.start && t.wrote.contains_key(k));
                    check!(format!("{trace:?}"), db.commit(id), if lost { Err(TxError::Conflict) } else { Ok(()) });
                    if !lost && !t.wrote.is_empty() {
                        model.commits += 1;
                        for (k, v) in t.wrote {
                            match v {
                                Some(v) => model.data.insert(k.clone(), v),
                                None => model.data.remove(&k),
                            };
                            model.log.push((model.commits, k, v));
                        }
                    }
                }
                _ => {
                    trace.push("vacuum".to_string());
                    let horizon = open.iter().map(|t| t.start).min().unwrap_or(model.commits);
                    let before = db.versions();
                    let removed = db.vacuum();
                    let want = model.expected_versions(horizon);
                    check!(format!("{trace:?}: (versions after vacuum, removed)"), (db.versions(), removed), (want, before - want));
                }
            }
        }
    }
}
