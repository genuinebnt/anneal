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
fn a_new_job() {
    let store = JobStore::new();
    let (job, created) = store.submit("k1", spec("cat.png")).unwrap();
    check!("created", created, true);
    let want = json!({"id":1,"status":"queued","source":"cat.png","width":10,"height":20,"format":"png","created_at":1,"updated_at":1,"output":null,"error":null});
    check!("the job as JSON", serde_json::to_value(&job).unwrap(), want);
    check!("get(1)", store.get(1), Some(job));
}

#[test]
fn a_retry_changes_nothing() {
    let store = JobStore::new();
    let first = store.submit("k1", spec("a")).unwrap();
    let again = store.submit("k1", spec("a")).unwrap();
    check!("second submit: (job, created)", again, (first.0, false));
    check!("history", names(&store, 1), vec!["job.queued"]);
}

#[test]
fn the_same_key_for_another_job() {
    let store = JobStore::new();
    add(&store, "k1");
    check!("same key, other spec", store.submit("k1", spec("b")), Err(ApiError::KeyReuse));
}

#[test]
fn from_queued_to_succeeded() {
    let store = JobStore::new();
    let id = add(&store, "k");
    check!("start", store.start(id), true);
    check!("finish", store.finish(id, Outcome::Succeeded("out.png".into())), true);
    check!("(status, created_at, updated_at)", times(&store, id), (Status::Succeeded, 1, 3));
    check!("output", view(&store, id).output, Some("out.png".to_string()));
    check!("history", names(&store, id), vec!["job.queued", "job.started", "job.succeeded"]);
}

#[test]
fn history_has_ids_and_data() {
    let store = JobStore::new();
    add(&store, "a");
    let id = add(&store, "b");
    store.start(id);
    check!("history of job 2", entries(&store, id, 0), vec![(1, "job.queued", json!({"job_id":2,"status":"queued"})), (2, "job.started", json!({"job_id":2,"status":"running"}))]);
    check!("after entry 1", entries(&store, id, 1).len(), 1);
}
