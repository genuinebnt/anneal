use solution::*;

#[allow(dead_code, unused_imports)]
const DATE: &str = "Thu, 01 Jan 2026 10:00:00 GMT";

#[allow(dead_code, unused_imports)]
fn file(len: u64) -> FileInfo<'static> {
    FileInfo { tag: "\"v7\"", last_modified: DATE, len }
}

#[allow(dead_code, unused_imports)]
fn get() -> Ask<'static> {
    Ask { method: "GET", if_none_match: None, if_modified_since: None, range: None }
}

#[allow(dead_code, unused_imports)]
fn range(value: &str, len: u64) -> Plan {
    plan(&Ask { range: Some(value), ..get() }, &file(len))
}

#[allow(dead_code, unused_imports)]
fn part(first: u64, last: u64) -> Plan {
    Plan::Part { first, last }
}

#[test]
fn a_plain_request_gets_everything() {
    check!("nothing asked", plan(&get(), &file(100)), Plan::Whole);
}

#[test]
fn a_current_tag_means_not_modified() {
    check!("If-None-Match: \"v7\"", plan(&Ask { if_none_match: Some("\"v7\""), ..get() }, &file(100)), Plan::NotModified);
}

#[test]
fn the_first_ten_bytes() {
    check!("bytes=0-9", (range("bytes=0-9", 100), content_range(range("bytes=0-9", 100), 100)), (part(0, 9), Some("bytes 0-9/100".to_string())));
}

#[test]
fn the_last_ten_bytes() {
    check!("bytes=-10", range("bytes=-10", 100), part(90, 99));
}

#[test]
fn past_the_end_cannot_be_served() {
    check!("bytes=100-", (range("bytes=100-", 100), content_range(Plan::Unsatisfiable, 100)), (Plan::Unsatisfiable, Some("bytes */100".to_string())));
}

#[test]
fn nonsense_is_ignored() {
    check!("bytes=a-b", range("bytes=a-b", 100), Plan::Whole);
}
