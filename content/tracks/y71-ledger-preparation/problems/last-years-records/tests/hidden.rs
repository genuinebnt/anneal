use solution::*;

use serde_json::{json, Value};

#[allow(dead_code)]
fn sent(name: &str, city: &str, weight_g: u64, carrier: &str) -> Result<Parcel, ReadError> {
    Ok(Parcel::Sent { name: name.into(), city: city.into(), weight_g, carrier: carrier.into() })
}

#[allow(dead_code)]
fn check_value(v: Value, want: Result<Parcel, ReadError>) {
    let text = v.to_string();
    check!(text.clone(), read(&text), want);
}

#[test]
fn old_records_may_carry_newer_field_names() {
    check_value(
        json!({"v":2,"kind":"sent","data":{"name":"a","city":"b","grams":5,"weight_g":900,"carrier":"rocket"}}),
        sent("a", "b", 5, "post"),
    );
    check_value(
        json!({"v":1,"kind":"shipped","data":{"to":"a, b","grams":5,"name":"x","city":"y","carrier":"rocket","weight_g":1}}),
        sent("a", "b", 5, "post"),
    );
    check_value(json!({"v":1,"kind":"lost","data":{"why":"w","note":"n"}}), Ok(Parcel::Lost { note: "w".into() }));
}

#[test]
fn newer_records_do_not_accept_old_field_names() {
    check_value(json!({"v":3,"kind":"sent","data":{"name":"a","city":"b","grams":5,"carrier":"x"}}), Err(ReadError::BadData));
    check_value(json!({"v":2,"kind":"lost","data":{"why":"w"}}), Err(ReadError::BadData));
    check_value(json!({"v":3,"kind":"sent","data":{"name":"a","city":"b","weight_g":5}}), Err(ReadError::BadData));
    check_value(json!({"v":2,"kind":"sent","data":{"to":"a, b","grams":5}}), Err(ReadError::BadData));
}

#[test]
fn splitting_at_the_last_separator() {
    check_value(json!({"kind":"shipped","data":{"to":"a, b, c","grams":0}}), sent("a, b", "c", 0, "post"));
    check_value(json!({"kind":"shipped","data":{"to":", ","grams":1}}), sent("", "", 1, "post"));
    check_value(json!({"kind":"shipped","data":{"to":"a,b","grams":1}}), Err(ReadError::BadData));
    check_value(json!({"kind":"shipped","data":{"to":"Zoë, Łódź","grams":1}}), sent("Zoë", "Łódź", 1, "post"));
}

#[test]
fn weights() {
    check_value(
        json!({"v":2,"kind":"sent","data":{"name":"a","city":"b","grams":u64::MAX}}),
        sent("a", "b", u64::MAX, "post"),
    );
    for grams in [json!(-1), json!(1.5), json!("7"), json!(null)] {
        check_value(json!({"v":2,"kind":"sent","data":{"name":"a","city":"b","grams":grams}}), Err(ReadError::BadData));
    }
}

#[test]
fn versions_outside_the_range() {
    for v in [0u64, 4, 99, u64::MAX] {
        check_value(json!({"v":v,"kind":"whatever","data":5}), Err(ReadError::UnknownVersion(v)));
    }
}

#[test]
fn a_version_that_is_not_a_whole_number() {
    for v in [json!(-1), json!(2.5), json!(null), json!([3])] {
        check_value(json!({"v":v,"kind":"lost","data":{"note":"n"}}), Err(ReadError::NotARecord));
    }
    check_value(json!({"v":3,"kind":7,"data":{}}), Err(ReadError::NotARecord));
}

#[test]
fn errors_come_in_the_stated_order() {
    check_value(json!({"v":3,"kind":"melted","data":5}), Err(ReadError::UnknownKind("melted".into())));
    check_value(json!({"v":1,"kind":"sent","data":{}}), Err(ReadError::UnknownKind("sent".into())));
    check_value(json!({"v":3,"kind":"shipped"}), Err(ReadError::UnknownKind("shipped".into())));
    check_value(json!({"v":3,"kind":"lost"}), Err(ReadError::BadData));
    check_value(json!({"v":3,"kind":"lost","data":[1]}), Err(ReadError::BadData));
}

#[test]
fn damaged_text_never_panics() {
    let good = r#"{"v":1,"kind":"shipped","data":{"to":"Zoë, Łódź","grams":12}}"#;
    for (i, _) in good.char_indices() {
        check!(&good[..i], read(&good[..i]), Err(ReadError::NotARecord));
    }
    let deep = "[".repeat(100_000);
    check!("very deep nesting", read(&deep), Err(ReadError::NotARecord));
}

#[test]
fn the_same_parcel_reads_the_same_from_every_version() {
    let mut rng = anneal_prelude::Rng::new(7104);
    for _ in 0..300 {
        let len = rng.below(8);
        let name = rng.string(len, "ab, é\"\\");
        let len = rng.below(5);
        let city = rng.string(len, "xyz é");
        let grams = rng.next_u64();
        let v1 = json!({"v":1,"kind":"shipped","data":{"to":format!("{name}, {city}"),"grams":grams}});
        let v2 = json!({"v":2,"kind":"sent","data":{"name":name,"city":city,"grams":grams}});
        let v3 = json!({"v":3,"kind":"sent","data":{"name":name,"city":city,"weight_g":grams,"carrier":"post"}});
        for v in [v1, v2, v3] {
            check_value(v, sent(&name, &city, grams, "post"));
        }
        let note = name.clone();
        for v in [json!({"kind":"lost","data":{"why":note}}), json!({"v":2,"kind":"lost","data":{"note":note}}), json!({"v":3,"kind":"lost","data":{"note":note}})] {
            check_value(v, Ok(Parcel::Lost { note: note.clone() }));
        }
    }
}
