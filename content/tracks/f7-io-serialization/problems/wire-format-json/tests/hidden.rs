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
fn with_amount(a: Value) -> Option<u64> {
    read(json!({"id":"e","created":1,"type":"charge.succeeded","data":{"amountCents":a,"currency":"EUR"}})).and_then(|e| match e.payload {
        Payload::Charge { amount_cents, .. } => Some(amount_cents),
        _ => None,
    })
}

#[allow(dead_code)]
fn with_reason(r: Value) -> Option<Reason> {
    read(json!({"id":"e","created":1,"type":"charge.refunded","data":{"chargeId":"c","reason":r}})).and_then(|e| match e.payload {
        Payload::Refund { reason, .. } => Some(reason),
        _ => None,
    })
}

#[test]
fn amount_bounds() {
    check!("u64::MAX", with_amount(json!("18446744073709551615")), Some(u64::MAX));
    check!("zero", with_amount(json!("0")), Some(0));
    check!("one past u64::MAX", with_amount(json!("18446744073709551616")), None);
}

#[test]
fn amount_rejects_everything_but_digits() {
    for bad in ["", "+5", "-5", " 5", "5 ", "5.0", "0x1", "1_000", "١"] {
        check!(format!("amountCents {bad:?}"), with_amount(json!(bad)), None);
    }
    check!("null", with_amount(Value::Null), None);
}

#[test]
fn note_may_be_null_or_absent() {
    let doc = json!({"id":"e","created":1,"type":"charge.succeeded","data":{"amountCents":"1","currency":"EUR","note":null}});
    let got = read(doc).map(|e| matches!(e.payload, Payload::Charge { note: None, .. }));
    check!("note: null", got, Some(true));
    check!("note absent", with_amount(json!("1")), Some(1));
}

#[test]
fn every_reason() {
    check!("duplicate", with_reason(json!("duplicate")), Some(Reason::Duplicate));
    check!("fraudulent", with_reason(json!("fraudulent")), Some(Reason::Fraudulent));
    check!("other", with_reason(json!("other")), Some(Reason::Other));
    check!("a reason added next year", with_reason(json!("chargeback")), Some(Reason::Other));
}

#[test]
fn a_reason_must_be_a_string() {
    check!("reason: 3", with_reason(json!(3)), None);
    check!("reason: null", with_reason(Value::Null), None);
}

#[test]
fn unknown_fields_are_ignored() {
    let doc = json!({"id":"e","created":1,"livemode":true,"type":"charge.refunded","data":{"chargeId":"c","reason":"duplicate","extra":{"a":[1]}},"api_version":"9"});
    check!("extra fields at both levels", read(doc).is_some(), true);
}

#[test]
fn unknown_type_is_an_error() {
    check!("type: charge.disputed", read(json!({"id":"e","created":1,"type":"charge.disputed","data":{}})), None);
    check!("the Rust variant name is not the wire name", read(json!({"id":"e","created":1,"type":"Ping"})), None);
}

#[test]
fn required_fields_are_required() {
    let full = json!({"id":"e","created":1,"type":"charge.refunded","data":{"chargeId":"c","reason":"duplicate"}});
    check!("the complete document", read(full.clone()).is_some(), true);
    for key in ["id", "created", "type", "data"] {
        let mut doc = full.clone();
        doc.as_object_mut().unwrap().remove(key);
        check!(format!("without {key}"), read(doc), None);
    }
    for key in ["chargeId", "reason"] {
        let mut doc = full.clone();
        doc["data"].as_object_mut().unwrap().remove(key);
        check!(format!("without data.{key}"), read(doc), None);
    }
}

#[test]
fn rust_field_names_are_not_accepted() {
    let doc = json!({"id":"e","created":1,"type":"charge.succeeded","data":{"amount_cents":"1","currency":"EUR"}});
    check!("amount_cents instead of amountCents", read(doc), None);
}

#[test]
fn key_order_does_not_matter() {
    let text = r#"{"data":{"currency":"EUR","amountCents":"7"},"type":"charge.succeeded","created":1,"id":"e"}"#;
    check!("data before type", serde_json::from_str::<Event>(text).is_ok(), true);
}

#[test]
fn refund_is_written_in_wire_names() {
    let e = Event { id: "e".into(), created: 2, payload: Payload::Refund { charge_id: "c".into(), reason: Reason::Other }, tags: vec!["a".into(), "b".into()] };
    check!("a refund with tags", write(&e), json!({"id":"e","created":2,"type":"charge.refunded","data":{"chargeId":"c","reason":"other"},"tags":["a","b"]}));
}

#[test]
fn random_events_round_trip() {
    let mut rng = anneal_prelude::Rng::new(8116);
    let reasons = [Reason::Duplicate, Reason::Fraudulent, Reason::RequestedByCustomer, Reason::Other];
    let wire = ["duplicate", "fraudulent", "requested_by_customer", "other"];
    for _ in 0..400 {
        let id = rng.string(4, "ab\"\\é ");
        let created = rng.next_u64();
        let tag_count = rng.below(3);
        let tags: Vec<String> = (0..tag_count).map(|_| rng.string(2, "xy")).collect();
        let mut want = json!({"id": id, "created": created});
        let payload = match rng.below(3) {
            0 => {
                let amount_cents = rng.next_u64() >> rng.below(64);
                let note = rng.bool().then(|| rng.string(3, "n "));
                want["type"] = json!("charge.succeeded");
                want["data"] = json!({"amountCents": amount_cents.to_string(), "currency": "SEK"});
                if let Some(n) = &note {
                    want["data"]["note"] = json!(n);
                }
                Payload::Charge { amount_cents, currency: "SEK".into(), note }
            }
            1 => {
                let i = rng.below(4);
                want["type"] = json!("charge.refunded");
                want["data"] = json!({"chargeId": "c1", "reason": wire[i]});
                Payload::Refund { charge_id: "c1".into(), reason: reasons[i] }
            }
            _ => {
                want["type"] = json!("ping");
                Payload::Ping
            }
        };
        if !tags.is_empty() {
            want["tags"] = json!(tags);
        }
        let event = Event { id, created, payload, tags };
        check!(format!("write {event:?}"), write(&event), want.clone());
        check!(format!("read back {want}"), read(want.clone()), Some(event));
    }
}
