use solution::*;

#[allow(dead_code)]
fn good(secret: &[u8], t: &str, body: &[u8]) -> String {
    hex::encode(hmac_sha256(secret, &[t.as_bytes(), b".", body].concat()))
}

const SECRET: &[u8] = b"whsec_test";

#[test]
fn the_primitive_is_untouched() {
    // RFC 4231, test case 2.
    check!("HMAC-SHA256(\"Jefe\", \"what do ya want for nothing?\")", hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843".to_string());
}

#[test]
fn every_prefix_is_rejected() {
    let full = good(SECRET, "7", b"x");
    for n in 0..full.len() {
        let header = format!("t=7,v1={}", &full[..n]);
        check!(format!("first {n} hex digits"), verify(SECRET, &header, b"x", 7, 5), Err(VerifyError::BadSignature));
    }
}

#[test]
fn a_longer_signature_is_rejected() {
    let header = format!("t=7,v1={}00", good(SECRET, "7", b"x"));
    check!("33 bytes", verify(SECRET, &header, b"x", 7, 5), Err(VerifyError::BadSignature));
}

#[test]
fn body_and_secret_matter() {
    let header = sign(SECRET, 7, b"pay 1");
    check!("another body", verify(SECRET, &header, b"pay 9", 7, 5), Err(VerifyError::BadSignature));
    check!("another secret", verify(b"other", &header, b"pay 1", 7, 5), Err(VerifyError::BadSignature));
}

#[test]
fn window_edges_are_allowed() {
    let header = sign(SECRET, 1000, b"b");
    check!("exactly 300 s old and 300 s early", (verify(SECRET, &header, b"b", 1300, 300), verify(SECRET, &header, b"b", 700, 300)), (Ok(()), Ok(())));
}

#[test]
fn extreme_clocks_do_not_overflow() {
    let header = sign(SECRET, u64::MAX, b"b");
    check!("t = u64::MAX, now = 0", verify(SECRET, &header, b"b", 0, 300), Err(VerifyError::Expired));
    check!("tolerance u64::MAX", verify(SECRET, &header, b"b", 0, u64::MAX), Ok(()));
}

#[test]
fn bad_signature_is_reported_before_age() {
    check!("forged and old", verify(SECRET, "t=1,v1=00", b"b", 100_000, 300), Err(VerifyError::BadSignature));
}

#[test]
fn malformed_headers() {
    let sig = good(SECRET, "5", b"b");
    for header in [
        String::new(),
        "t=5".to_string(),
        format!("v1={sig}"),
        format!("t=5,v1={sig},junk"),
        format!("t=,v1={sig}"),
        format!("t=+5,v1={sig}"),
        format!("t= 5,v1={sig}"),
        format!("t=18446744073709551616,v1={sig}"),
        format!("t=5,t=5,v1={sig}"),
    ] {
        check!(format!("{header:?}"), verify(SECRET, &header, b"b", 5, 10), Err(VerifyError::Malformed));
    }
}

#[test]
fn unknown_keys_and_order() {
    let header = format!("v0=abc,v1={},t=5,scheme=", good(SECRET, "5", b"b"));
    check!("t last, extra keys", verify(SECRET, &header, b"b", 5, 10), Ok(()));
}

#[test]
fn the_timestamp_is_signed_as_written() {
    let header = format!("t=005,v1={}", good(SECRET, "005", b"b"));
    check!("leading zeros, signed as written", verify(SECRET, &header, b"b", 5, 10), Ok(()));
}

#[test]
fn upper_case_hex_and_garbage_beside_a_match() {
    let header = format!("t=5,v1=zz,v1=,v1={}", good(SECRET, "5", b"b").to_uppercase());
    check!("two useless v1 entries, then upper case", verify(SECRET, &header, b"b", 5, 10), Ok(()));
}

#[test]
fn random_tampering_is_always_caught() {
    let mut rng = anneal_prelude::Rng::new(8117);
    for _ in 0..300 {
        let len = rng.below(40);
        let mut body: Vec<u8> = rng.vec(len, 0, 255);
        let t = rng.int(1000, 2000) as u64;
        let header = sign(SECRET, t, &body);
        check!(format!("genuine, t={t}, body {body:?}"), verify(SECRET, &header, &body, t, 0), Ok(()));
        let mut chars = header.into_bytes();
        let start = chars.len() - 64;
        if rng.bool() || body.is_empty() {
            let i = start + rng.below(64);
            chars[i] = if chars[i] == b'0' { b'1' } else { b'0' };
        } else {
            let i = rng.below(body.len());
            body[i] ^= 1 << rng.below(8);
        }
        let header = String::from_utf8(chars).unwrap();
        check!(format!("one change: {header}, body {body:?}"), verify(SECRET, &header, &body, t, 0), Err(VerifyError::BadSignature));
    }
}

#[test]
fn the_match_may_come_before_another_signature() {
    let header = format!("t=5,v1={},v1={}", good(SECRET, "5", b"b"), good(b"next-secret", "5", b"b"));
    check!("current secret first, next secret second", verify(SECRET, &header, b"b", 5, 10), Ok(()));
}
