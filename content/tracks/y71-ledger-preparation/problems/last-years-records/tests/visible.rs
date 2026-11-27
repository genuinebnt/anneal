use solution::*;

#[allow(dead_code)]
fn sent(name: &str, city: &str, weight_g: u64, carrier: &str) -> Result<Parcel, ReadError> {
    Ok(Parcel::Sent { name: name.into(), city: city.into(), weight_g, carrier: carrier.into() })
}

#[test]
fn a_current_record() {
    let text = r#"{"v":3,"kind":"sent","data":{"name":"Ada","city":"York","weight_g":250,"carrier":"bike"}}"#;
    check!(text, read(text), sent("Ada", "York", 250, "bike"));
}

#[test]
fn a_version_2_record() {
    let text = r#"{"v":2,"kind":"sent","data":{"name":"Ada","city":"York","grams":250}}"#;
    check!(text, read(text), sent("Ada", "York", 250, "post"));
}

#[test]
fn a_version_1_record_with_a_comma_in_the_name() {
    let text = r#"{"v":1,"kind":"shipped","data":{"to":"King, Jr., Hull","grams":9}}"#;
    check!(text, read(text), sent("King, Jr.", "Hull", 9, "post"));
}

#[test]
fn no_version_means_version_1() {
    let text = r#"{"kind":"lost","data":{"why":"fell off"}}"#;
    check!(text, read(text), Ok(Parcel::Lost { note: "fell off".into() }));
}

#[test]
fn a_newer_version_is_refused() {
    let text = r#"{"v":4,"kind":"sent","data":{"name":"Ada","city":"York","weight_g":1,"carrier":"van"}}"#;
    check!(text, read(text), Err(ReadError::UnknownVersion(4)));
}

#[test]
fn an_old_kind_name_is_unknown_later() {
    let text = r#"{"v":2,"kind":"shipped","data":{"to":"A, B","grams":1}}"#;
    check!(text, read(text), Err(ReadError::UnknownKind("shipped".into())));
}

#[test]
fn not_a_record() {
    for text in ["", "[1]", "{\"v\":3}", "{\"v\":\"3\",\"kind\":\"lost\",\"data\":{}}"] {
        check!(text, read(text), Err(ReadError::NotARecord));
    }
}
