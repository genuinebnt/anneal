use solution::*;

use serde_json::json;

#[allow(dead_code)]
fn line(schema: u32, payload: serde_json::Value) -> Stored {
    Stored { pos: 0, stream: "a".into(), version: 1, cmd: CmdId::Client(1), print: 0, ts: 0, schema, payload }
}

#[allow(dead_code)]
fn today(event: Event) -> NewEvent {
    NewEvent::current(CmdId::Client(1), 0, 0, &event)
}

#[test]
fn todays_shape_decodes() {
    let l = line(2, json!({"type":"Withdrawn","data":{"amount":5,"reason":"atm"}}));
    check!("schema 2", decode(&l), Ok(Event::Withdrawn { amount: 5, reason: "atm".into() }));
    check!("no data", decode(&line(2, json!({"type":"Opened"}))), Ok(Event::Opened));
}

#[test]
fn version_1_shapes_decode() {
    let dep = line(1, json!({"type":"Deposited","data":{"cents":1000}}));
    check!("cents", decode(&dep), Ok(Event::Deposited { amount: 1000 }));
    let wd = line(1, json!({"type":"Withdrawn","data":{"amt":250}}));
    check!("amt", decode(&wd), Ok(Event::Withdrawn { amount: 250, reason: "legacy".into() }));
}

#[test]
fn a_newer_schema_is_not_guessed_at() {
    let l = line(3, json!({"type":"Opened"}));
    check!("schema 3", decode(&l), Err(DecodeError::UnknownSchema(3)));
}

#[test]
fn appending_gives_places() {
    let mut store = MemStore::new();
    let a = store.append("a", 0, vec![today(Event::Opened), today(Event::Deposited { amount: 5 })]).unwrap();
    let b = store.append("b", 0, vec![today(Event::Opened)]).unwrap();
    check!("a", a.iter().map(|l| (l.pos, l.version)).collect::<Vec<_>>(), vec![(0, 1), (1, 2)]);
    check!("b", (b[0].pos, b[0].version), (2, 1));
    check!("streams", store.streams(), vec!["a".to_string(), "b".to_string()]);
    check!("tail of a", store.read_stream("a", 1), a[1..].to_vec());
}

#[test]
fn a_stale_append_changes_nothing() {
    let mut store = MemStore::new();
    store.append("a", 0, vec![today(Event::Opened)]).unwrap();
    let err = store.append("a", 0, vec![today(Event::Opened)]).unwrap_err();
    check!("conflict", matches!(err, StoreError::Conflict { expected: 0, actual: 1, .. }), true);
    check!("lines", store.read_all(0).len(), 1);
}

#[test]
fn old_lines_stay_as_they_were_written() {
    let mut store = MemStore::new();
    let old = NewEvent { cmd: CmdId::Client(1), print: 0, ts: 9, schema: 1, payload: json!({"type":"Deposited","data":{"cents":7}}) };
    store.append("a", 0, vec![old.clone()]).unwrap();
    let kept = &store.read_all(0)[0];
    check!("payload", kept.payload.clone(), old.payload);
    check!("schema, ts", (kept.schema, kept.ts), (1, 9));
}
