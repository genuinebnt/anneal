use solution::*;

/// The hex of HMAC-SHA256(secret, "<t>.<body>"), built from the provided primitive.
#[allow(dead_code)]
fn good(secret: &[u8], t: &str, body: &[u8]) -> String {
    hex::encode(hmac_sha256(secret, &[t.as_bytes(), b".", body].concat()))
}

const SECRET: &[u8] = b"whsec_test";
const BODY: &[u8] = br#"{"amount":100}"#;

#[test]
fn accepts_a_genuine_request() {
    let header = format!("t=1000,v1={}", good(SECRET, "1000", BODY));
    check!(header.clone(), verify(SECRET, &header, BODY, 1010, 300), Ok(()));
}

#[test]
fn sign_covers_the_timestamp() {
    check!("sign at t=1000", sign(SECRET, 1000, BODY), format!("t=1000,v1={}", good(SECRET, "1000", BODY)));
}

#[test]
fn an_old_signature_cannot_be_given_a_new_timestamp() {
    let old = sign(SECRET, 1000, BODY);
    let forged = old.replacen("t=1000", "t=900000", 1);
    check!("t rewritten on a captured header", verify(SECRET, &forged, BODY, 900000, 300), Err(VerifyError::BadSignature));
}

#[test]
fn an_empty_signature_is_not_a_signature() {
    check!(r#""t=1000,v1=""#, verify(SECRET, "t=1000,v1=", BODY, 1000, 300), Err(VerifyError::BadSignature));
}

#[test]
fn expiry_works_in_both_directions() {
    let h = sign(SECRET, 10_000, BODY);
    check!("301 s old", verify(SECRET, &h, BODY, 10_301, 300), Err(VerifyError::Expired));
    check!("301 s in the future", verify(SECRET, &h, BODY, 9_699, 300), Err(VerifyError::Expired));
}

#[test]
fn any_of_several_signatures_may_match() {
    let header = format!("t=50,v1={},v1={}", good(b"old-secret", "50", BODY), good(SECRET, "50", BODY));
    check!("old secret first, current secret second", verify(SECRET, &header, BODY, 50, 300), Ok(()));
}
