use solution::*;

use serde_json::{json, Value};

#[allow(dead_code)]
fn line(schema: u32, payload: Value) -> Stored {
    Stored { pos: 0, stream: "a".into(), version: 1, cmd: CmdId::Client(1), print: 0, ts: 0, schema, payload }
}

#[allow(dead_code)]
fn today(event: Event) -> NewEvent {
    NewEvent::current(CmdId::Client(1), 0, 0, &event)
}

#[allow(dead_code)]
fn raw(schema: u32, payload: Value) -> NewEvent {
    NewEvent { cmd: CmdId::Client(1), print: 0, ts: 0, schema, payload }
}

#[allow(dead_code)]
fn placed(pos: u64, stream: &str, version: u64, payload: Value) -> Stored {
    Stored { pos, stream: stream.into(), version, cmd: CmdId::Client(1), print: 0, ts: 0, schema: 2, payload }
}

#[allow(dead_code)]
fn opened_json() -> Value {
    json!({"type":"Opened"})
}

#[test]
fn every_event_survives_a_round_trip() {
    let events = [
        Event::Opened,
        Event::Deposited { amount: i64::MAX },
        Event::Withdrawn { amount: 1, reason: "é\"\\".into() },
        Event::TransferredOut { transfer: u64::MAX, to: "b".into(), amount: 2 },
        Event::TransferredIn { transfer: 0, from: "b".into(), amount: 2 },
        Event::Reversed { target: 3, delta: i64::MIN },
    ];
    for e in events {
        let new = today(e.clone());
        check!(format!("{e:?}"), decode(&line(new.schema, new.payload)), Ok(e.clone()));
    }
}

#[test]
fn other_kinds_did_not_change_between_versions() {
    let l = line(1, json!({"type":"TransferredIn","data":{"transfer":4,"from":"b","amount":2}}));
    check!("v1 transfer", decode(&l), Ok(Event::TransferredIn { transfer: 4, from: "b".into(), amount: 2 }));
    check!("v1 open", decode(&line(1, opened_json())), Ok(Event::Opened));
}

#[test]
fn each_version_knows_only_its_own_fields() {
    let with_reason = line(1, json!({"type":"Withdrawn","data":{"amt":3,"reason":"made up","amount":999}}));
    check!("v1 extras", decode(&with_reason), Ok(Event::Withdrawn { amount: 3, reason: "legacy".into() }));
    let new_names = line(1, json!({"type":"Deposited","data":{"amount":3}}));
    check!("v2 names in a v1 line", decode(&new_names), Err(DecodeError::BadPayload));
    let old_names = line(2, json!({"type":"Deposited","data":{"cents":3}}));
    check!("v1 names in a v2 line", decode(&old_names), Err(DecodeError::BadPayload));
}

#[test]
fn bad_payloads_are_errors_not_panics() {
    for schema in [1, 2] {
        for payload in [json!(null), json!(5), json!([1]), json!({}), json!({"type":5}), json!({"type":"Melted"}), json!({"type":"Deposited"}), json!({"type":"Withdrawn","data":5}), json!({"type":"Deposited","data":{"cents":"7","amount":"7"}})] {
            check!(format!("schema {schema}: {payload}"), decode(&line(schema, payload.clone())), Err(DecodeError::BadPayload));
        }
    }
    for schema in [0, 3, u32::MAX] {
        check!(schema, decode(&line(schema, opened_json())), Err(DecodeError::UnknownSchema(schema)));
    }
}

#[test]
fn one_unreadable_event_refuses_the_whole_batch() {
    let mut store = MemStore::new();
    let bad = vec![today(Event::Opened), raw(2, json!({"type":"Melted"}))];
    let err = store.append("a", 0, bad).unwrap_err();
    check!("error", matches!(err, StoreError::BadEvent(DecodeError::BadPayload)), true);
    let err = store.append("a", 0, vec![raw(9, opened_json())]).unwrap_err();
    check!("newer", matches!(err, StoreError::BadEvent(DecodeError::UnknownSchema(9))), true);
    check!("nothing stored", (store.read_all(0).len(), store.version("a"), store.streams().len()), (0, 0, 0));
}

#[test]
fn a_conflict_comes_before_a_bad_event() {
    let mut store = MemStore::new();
    let err = store.append("a", 1, vec![raw(2, json!(null))]).unwrap_err();
    check!("conflict", matches!(err, StoreError::Conflict { expected: 1, actual: 0, .. }), true);
}

#[test]
fn prepare_changes_nothing() {
    let store = MemStore::new();
    let batch = store.prepare("a", 0, vec![today(Event::Opened)]).unwrap();
    check!("prepared", (batch[0].pos, batch[0].version), (0, 1));
    check!("store", (store.read_all(0).len(), store.version("a")), (0, 0));
}

#[test]
fn an_empty_append() {
    let mut store = MemStore::new();
    check!("empty", store.append("a", 0, vec![]).unwrap(), vec![]);
    check!("no stream", store.streams().len(), 0);
    check!("still checked", store.append("a", 2, vec![]).is_err(), true);
}

#[test]
fn admit_takes_only_what_continues_the_log() {
    let mut store = MemStore::new();
    check!("empty", store.admit(vec![]), Err(BadBatch::Empty));
    store.admit(vec![placed(0, "a", 1, opened_json())]).unwrap();
    let cases = [
        (vec![placed(0, "b", 1, opened_json())], 0),
        (vec![placed(2, "b", 1, opened_json())], 2),
        (vec![placed(1, "a", 1, opened_json())], 1),
        (vec![placed(1, "a", 3, opened_json())], 1),
        (vec![placed(1, "b", 1, opened_json()), placed(2, "b", 1, opened_json())], 2),
        (vec![placed(1, "b", 1, opened_json()), placed(1, "c", 1, opened_json())], 1),
    ];
    for (batch, pos) in cases {
        check!(format!("{batch:?}"), store.admit(batch.clone()), Err(BadBatch::OutOfSequence { pos }));
        check!("unchanged", (store.read_all(0).len(), store.streams().len()), (1, 1));
    }
    let bad = vec![placed(1, "b", 1, opened_json()), placed(2, "a", 2, json!(7))];
    check!("unreadable", store.admit(bad), Err(BadBatch::Unreadable { pos: 2, why: DecodeError::BadPayload }));
    check!("unchanged", (store.read_all(0).len(), store.version("b")), (1, 0));
    // two streams in one batch, each continuing its own count
    let good = vec![placed(1, "a", 2, opened_json()), placed(2, "b", 1, opened_json()), placed(3, "a", 3, opened_json())];
    check!("good", store.admit(good), Ok(()));
    check!("versions", (store.version("a"), store.version("b")), (3, 1));
}

#[test]
fn reading_past_the_end() {
    let mut store = MemStore::new();
    store.append("a", 0, vec![today(Event::Opened)]).unwrap();
    check!("all", store.read_all(u64::MAX).len(), 0);
    check!("stream", store.read_stream("a", u64::MAX).len(), 0);
    check!("unknown", store.read_stream("zz", 0).len(), 0);
}

#[test]
fn random_appends_match_a_plain_list() {
    let mut rng = anneal_prelude::Rng::new(7202);
    for _ in 0..200 {
        let mut store = MemStore::new();
        let mut plain: Vec<(String, i64)> = Vec::new();
        for step in 0..25i64 {
            let stream = rng.pick(&["x", "y", "z"]).to_string();
            let have = plain.iter().filter(|(s, _)| *s == stream).count() as u64;
            let expected = if rng.below(4) == 0 { rng.below(5) as u64 } else { have };
            let count = rng.below(3) as i64;
            let batch = (0..count).map(|i| today(Event::Deposited { amount: step * 10 + i })).collect();
            let got = store.append(&stream, expected, batch);
            check!(format!("step {step}"), got.is_ok(), expected == have);
            if got.is_ok() {
                plain.extend((0..count).map(|i| (stream.clone(), step * 10 + i)));
            }
            let seen: Vec<(String, i64)> = store
                .read_all(0)
                .iter()
                .map(|l| match decode(l) {
                    Ok(Event::Deposited { amount }) => (l.stream.clone(), amount),
                    other => panic!("{other:?}"),
                })
                .collect();
            check!(format!("step {step}"), seen, plain.clone());
            let after = rng.below(5);
            let got: Vec<u64> = store.read_stream(&stream, after as u64).iter().map(|l| l.version).collect();
            let n = plain.iter().filter(|(s, _)| *s == stream).count() as u64;
            check!(format!("step {step}"), got, (after as u64 + 1..=n).collect::<Vec<_>>());
        }
    }
}

#[test]
fn a_line_with_wrong_places_and_a_bad_payload_is_out_of_sequence() {
    let mut store = MemStore::new();
    store.admit(vec![placed(0, "a", 1, opened_json())]).unwrap();
    check!("wrong position, unreadable", store.admit(vec![placed(5, "a", 2, json!(7))]), Err(BadBatch::OutOfSequence { pos: 5 }));
    check!("wrong version, unreadable", store.admit(vec![placed(1, "a", 9, json!(7))]), Err(BadBatch::OutOfSequence { pos: 1 }));
    check!("unchanged", store.read_all(0).len(), 1);
}
