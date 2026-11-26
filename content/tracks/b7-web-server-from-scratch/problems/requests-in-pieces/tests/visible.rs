use solution::*;

#[allow(dead_code, unused_imports)]
fn message(verb: &str, name: &str, headers: &[(&str, &str)], body: &[u8]) -> Message {
    Message { verb: verb.into(), name: name.into(), headers: headers.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(), body: body.to_vec() }
}

#[allow(dead_code, unused_imports)]
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(300)]))
}

#[test]
fn one_whole_request() {
    let bytes = b"PUT note MSG/1\r\nContent-Length: 5\r\n\r\nhello";
    check!(show(bytes), read_one(bytes), Read::Whole(message("PUT", "note", &[("content-length", "5")], b"hello"), bytes.len()));
}

#[test]
fn half_a_request_needs_more() {
    for bytes in [&b""[..], b"GET x MSG", b"GET x MSG/1\r\n", b"GET x MSG/1\r\nA: 1\r\n\r"] {
        check!(show(bytes), read_one(bytes), Read::NeedMore);
    }
}

#[test]
fn a_body_still_arriving_needs_more() {
    let bytes = b"PUT n MSG/1\r\nContent-Length: 5\r\n\r\nhel";
    check!(show(bytes), read_one(bytes), Read::NeedMore);
}

#[test]
fn two_requests_and_the_start_of_a_third() {
    let mut buf = b"GET a MSG/1\r\n\r\nGET b MSG/1\r\n\r\nGET c".to_vec();
    let got = take_all(&mut buf).map(|v| v.into_iter().map(|m| m.name).collect::<Vec<_>>());
    check!("two and a half", (got, buf), (Ok(vec!["a".to_string(), "b".to_string()]), b"GET c".to_vec()));
}

#[test]
fn a_wrong_version_is_invalid() {
    let bytes = b"GET a MSG/2\r\n\r\n";
    check!(show(bytes), read_one(bytes), Read::Invalid);
}

#[test]
fn bad_bytes_keep_what_came_before() {
    let mut buf = b"GET a MSG/1\r\n\r\nnonsense\r\n\r\n".to_vec();
    let got = take_all(&mut buf);
    check!("a good one then nonsense", (got, buf), (Err(Invalid { before: vec![message("GET", "a", &[], b"")] }), b"nonsense\r\n\r\n".to_vec()));
}
