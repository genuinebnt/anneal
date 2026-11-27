use solution::*;

#[allow(unused_imports)]
use serde_json::{Value, json};
#[allow(unused_imports)]
use std::sync::{Arc, Barrier};

#[allow(dead_code)]
pub fn spec(source: &str) -> Spec {
    Spec { source: source.to_string(), width: 10, height: 20, format: "png".to_string(), work_ms: 20 }
}

#[allow(dead_code)]
/// Submits a job that must be accepted.
pub fn add(store: &JobStore, key: &str) -> u64 {
    store.submit(key, spec(key)).expect("accepted").0.id
}

#[allow(dead_code)]
pub fn view(store: &JobStore, id: u64) -> JobView {
    store.get(id).expect("the job exists")
}

#[allow(dead_code)]
/// (status, created_at, updated_at)
pub fn times(store: &JobStore, id: u64) -> (Status, u64, u64) {
    let v = view(store, id);
    (v.status, v.created_at, v.updated_at)
}

#[allow(dead_code)]
/// (id, name, data) of every history entry after `after`.
pub fn entries(store: &JobStore, id: u64, after: u64) -> Vec<(u64, &'static str, Value)> {
    store.events(id, after).expect("the job exists").into_iter().map(|e| (e.id, e.name, serde_json::from_str(&e.data).unwrap_or(Value::Null))).collect()
}

#[allow(dead_code)]
pub fn names(store: &JobStore, id: u64) -> Vec<&'static str> {
    entries(store, id, 0).into_iter().map(|e| e.1).collect()
}

#[test]
fn failing_and_cancelling() {
    let store = JobStore::new();
    let a = add(&store, "a");
    store.start(a);
    check!("running to failed", store.finish(a, Outcome::Failed("corrupt".into())), true);
    let v = view(&store, a);
    check!("(status, output, error)", (v.status, v.output, v.error), (Status::Failed, None, Some("corrupt".to_string())));
    let b = add(&store, "b");
    check!("queued to cancelled", store.finish(b, Outcome::Cancelled), true);
    let c = add(&store, "c");
    store.start(c);
    check!("running to cancelled", store.finish(c, Outcome::Cancelled), true);
    check!("histories", (names(&store, a), names(&store, b), names(&store, c)), (vec!["job.queued", "job.started", "job.failed"], vec!["job.queued", "job.cancelled"], vec!["job.queued", "job.started", "job.cancelled"]));
    check!("data of the last entry", entries(&store, c, 2)[0].2.clone(), json!({"job_id":3,"status":"cancelled"}));
    check!("cancelled jobs have no output or error", (view(&store, c).output, view(&store, c).error), (None, None));
}

#[test]
fn a_queued_job_cannot_succeed_or_fail() {
    let store = JobStore::new();
    let id = add(&store, "a");
    let before = view(&store, id);
    check!("queued to succeeded", store.finish(id, Outcome::Succeeded("x".into())), false);
    check!("queued to failed", store.finish(id, Outcome::Failed("x".into())), false);
    check!("nothing changed", (view(&store, id), names(&store, id)), (before, vec!["job.queued"]));
}

#[test]
fn a_final_job_never_changes() {
    for end in [Outcome::Succeeded("first".into()), Outcome::Failed("first".into()), Outcome::Cancelled] {
        let store = JobStore::new();
        let id = add(&store, "a");
        store.start(id);
        store.finish(id, end.clone());
        let before = (view(&store, id), names(&store, id));
        let tries = [store.start(id), store.finish(id, Outcome::Succeeded("second".into())), store.finish(id, Outcome::Failed("second".into())), store.finish(id, Outcome::Cancelled)];
        check!(format!("after {end:?}: start and finish again"), tries, [false; 4]);
        check!(format!("after {end:?}: job and history"), (view(&store, id), names(&store, id)), before);
    }
}

#[test]
fn start_only_once() {
    let store = JobStore::new();
    let id = add(&store, "a");
    check!("start twice", (store.start(id), store.start(id)), (true, false));
    check!("history", names(&store, id), vec!["job.queued", "job.started"]);
}

#[test]
fn unknown_ids() {
    let store = JobStore::new();
    add(&store, "a");
    for id in [0, 2, u64::MAX] {
        check!(format!("job {id}: (get, start, finish, events)"), (store.get(id), store.start(id), store.finish(id, Outcome::Cancelled), store.events(id, 0)), (None, false, false, None));
    }
    check!("a known job with nothing new", store.events(1, u64::MAX), Some(vec![]));
}

#[test]
fn only_real_changes_take_a_tick_or_an_id() {
    let store = JobStore::new();
    add(&store, "a");
    let _ = store.submit("a", spec("a"));
    let _ = store.submit("a", spec("other"));
    store.finish(1, Outcome::Failed("no".into()));
    store.start(99);
    store.get(1);
    let (b, _) = store.submit("b", spec("b")).unwrap();
    check!("after a replay, a refused key and two refused changes: (id, created_at)", (b.id, b.created_at), (2, 2));
    store.start(1);
    check!("job 1", times(&store, 1), (Status::Running, 1, 3));
}

#[test]
fn a_replay_shows_the_job_as_it_is_now() {
    let store = JobStore::new();
    add(&store, "a");
    store.start(1);
    store.finish(1, Outcome::Succeeded("done".into()));
    let (job, created) = store.submit("a", spec("a")).unwrap();
    check!("replay after the job ended", (job.status, job.output, created), (Status::Succeeded, Some("done".to_string()), false));
}

#[test]
fn every_field_of_the_spec_counts() {
    let store = JobStore::new();
    add(&store, "k");
    let base = spec("k");
    for other in [Spec { source: "K".into(), ..base.clone() }, Spec { width: 11, ..base.clone() }, Spec { height: 21, ..base.clone() }, Spec { format: "jpg".into(), ..base.clone() }, Spec { work_ms: 21, ..base.clone() }] {
        check!(format!("same key with {other:?}"), store.submit("k", other.clone()), Err(ApiError::KeyReuse));
    }
}

#[test]
fn the_same_spec_under_new_keys_makes_new_jobs() {
    let store = JobStore::new();
    let a = store.submit("a", spec("same")).unwrap();
    let b = store.submit("b", spec("same")).unwrap();
    check!("(ids, created)", ((a.0.id, a.1), (b.0.id, b.1)), ((1, true), (2, true)));
    check!("keys are case sensitive", store.submit("A", spec("same")).unwrap().1, true);
}

#[test]
fn clones_share_and_new_stores_do_not() {
    let store = JobStore::new();
    let other = store.clone();
    add(&store, "a");
    check!("seen through the clone", other.get(1).map(|j| j.source), Some("a".to_string()));
    check!("another store", JobStore::new().get(1), None);
}

#[test]
fn racing_retries_make_one_job() {
    for round in 0..200 {
        let store = JobStore::new();
        let gate = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let (store, gate) = (store.clone(), gate.clone());
                std::thread::spawn(move || {
                    gate.wait();
                    let key = if i % 2 == 0 { "even" } else { "odd" };
                    store.submit(key, spec(key)).unwrap()
                })
            })
            .collect();
        let results: Vec<(JobView, bool)> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        let mut made: Vec<u64> = results.iter().filter(|r| r.1).map(|r| r.0.id).collect();
        made.sort();
        check!(format!("round {round}: ids reported as created"), made, vec![1, 2]);
        check!(format!("round {round}: no third job"), store.get(3), None);
    }
}

#[test]
fn many_jobs_stay_fast() {
    let store = JobStore::new();
    for i in 0..20_000 {
        add(&store, &format!("key-{i}"));
    }
    for i in 0..20_000 {
        let _ = store.submit(&format!("key-{i}"), spec(&format!("key-{i}"))).unwrap();
    }
    check!("last job", store.get(20_000).map(|j| j.created_at), Some(20_000));
}

#[test]
fn random_sessions_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(22002);
    for _ in 0..300 {
        let store = JobStore::new();
        // (status, created_at, updated_at, history)
        let mut jobs: Vec<(Status, u64, u64, Vec<&str>)> = Vec::new();
        let mut keys: std::collections::HashMap<String, (usize, String)> = Default::default();
        let mut clock = 0;
        let mut log = Vec::new();
        for _ in 0..25 {
            match rng.below(3) {
                0 => {
                    let key = rng.below(5).to_string();
                    let source = rng.pick(&["x", "y"]).to_string();
                    log.push(format!("submit({key}, {source})"));
                    let want = match keys.get(&key) {
                        Some((i, s)) if *s == source => Ok((*i as u64 + 1, false)),
                        Some(_) => Err(ApiError::KeyReuse),
                        None => {
                            clock += 1;
                            keys.insert(key.clone(), (jobs.len(), source.clone()));
                            jobs.push((Status::Queued, clock, clock, vec!["job.queued"]));
                            Ok((jobs.len() as u64, true))
                        }
                    };
                    check!(format!("{log:?}"), store.submit(&key, spec(&source)).map(|(j, c)| (j.id, c)), want);
                }
                action => {
                    let id = rng.below(5) as u64;
                    let outcome = [Outcome::Succeeded("o".into()), Outcome::Failed("e".into()), Outcome::Cancelled][rng.below(3)].clone();
                    let target = (id as usize).checked_sub(1).and_then(|i| jobs.get_mut(i));
                    let next = target.as_ref().and_then(|j| match (action, j.0, &outcome) {
                        (1, Status::Queued, _) => Some((Status::Running, "job.started")),
                        (2, Status::Running, Outcome::Succeeded(_)) => Some((Status::Succeeded, "job.succeeded")),
                        (2, Status::Running, Outcome::Failed(_)) => Some((Status::Failed, "job.failed")),
                        (2, Status::Running | Status::Queued, Outcome::Cancelled) => Some((Status::Cancelled, "job.cancelled")),
                        _ => None,
                    });
                    let want = next.is_some();
                    if let (Some(j), Some((status, name))) = (target, next) {
                        clock += 1;
                        j.0 = status;
                        j.2 = clock;
                        j.3.push(name);
                    }
                    let got = if action == 1 {
                        log.push(format!("start({id})"));
                        store.start(id)
                    } else {
                        log.push(format!("finish({id}, {outcome:?})"));
                        store.finish(id, outcome)
                    };
                    check!(format!("{log:?}"), got, want);
                }
            }
            for (i, j) in jobs.iter().enumerate() {
                let id = i as u64 + 1;
                check!(format!("{log:?}: job {id}"), (times(&store, id), names(&store, id)), ((j.0, j.1, j.2), j.3.clone()));
            }
        }
    }
}
