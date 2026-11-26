use solution::*;

use serde_json::{Value, json};

#[allow(dead_code)]
fn read(v: Value) -> Option<Event> {
    serde_json::from_str(&v.to_string()).ok()
}

#[allow(dead_code)]
fn write(e: &Event) -> Value {
    serde_json::to_value(e).unwrap()
}

#[allow(dead_code)]
fn charge(amount_cents: u64, note: Option<&str>, tags: &[&str]) -> Event {
    Event {
        id: "evt_1".into(),
        created: 1_700_000_000,
        payload: Payload::Charge { amount_cents, currency: "EUR".into(), note: note.map(Into::into) },
        tags: tags.iter().map(|t| t.to_string()).collect(),
    }
}

#[test]
fn reads_a_charge() {
    let doc = json!({"id":"evt_1","created":1700000000u64,"type":"charge.succeeded","data":{"amountCents":"1250","currency":"EUR","note":"gift"},"tags":["web"]});
    check!("the charge from the statement", read(doc), Some(charge(1250, Some("gift"), &["web"])));
}

#[test]
fn writes_a_charge() {
    let want = json!({"id":"evt_1","created":1700000000u64,"type":"charge.succeeded","data":{"amountCents":"1250","currency":"EUR"}});
    check!("a charge with no note and no tags", write(&charge(1250, None, &[])), want);
}

#[test]
fn reads_a_refund() {
    let doc = json!({"id":"evt_2","created":5,"type":"charge.refunded","data":{"chargeId":"ch_9","reason":"requested_by_customer"}});
    let want = Event { id: "evt_2".into(), created: 5, payload: Payload::Refund { charge_id: "ch_9".into(), reason: Reason::RequestedByCustomer }, tags: vec![] };
    check!("the refund from the statement", read(doc), Some(want));
}

#[test]
fn ping_has_no_data() {
    let e = Event { id: "evt_3".into(), created: 6, payload: Payload::Ping, tags: vec![] };
    check!("write", write(&e), json!({"id":"evt_3","created":6,"type":"ping"}));
    check!("read", read(json!({"id":"evt_3","created":6,"type":"ping"})), Some(e));
}

#[test]
fn amount_must_be_a_string() {
    let doc = json!({"id":"e","created":1,"type":"charge.succeeded","data":{"amountCents":12,"currency":"EUR"}});
    check!("amountCents as a number", read(doc), None);
}
