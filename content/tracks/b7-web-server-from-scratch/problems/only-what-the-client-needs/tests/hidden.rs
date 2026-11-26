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
fn tag_lists_weak_and_star() {
    for value in ["\"a\", \"v7\"", "\"a\",\"v7\" , \"b\"", "W/\"v7\"", "*", "\"x\", *"] {
        check!(value, plan(&Ask { if_none_match: Some(value), ..get() }, &file(5)), Plan::NotModified);
    }
    for value in ["\"v8\"", "v7", "\"v7", "W/\"v8\"", "", "\"v77\""] {
        check!(value, plan(&Ask { if_none_match: Some(value), ..get() }, &file(5)), Plan::Whole);
    }
}

#[test]
fn the_date_counts_only_without_a_tag() {
    check!("same date", plan(&Ask { if_modified_since: Some(DATE), ..get() }, &file(5)), Plan::NotModified);
    check!("other date", plan(&Ask { if_modified_since: Some("Thu, 01 Jan 1970 00:00:00 GMT"), ..get() }, &file(5)), Plan::Whole);
    check!("a tag that differs beats a date that matches", plan(&Ask { if_none_match: Some("\"old\""), if_modified_since: Some(DATE), ..get() }, &file(5)), Plan::Whole);
    check!("a tag that matches beats a date that differs", plan(&Ask { if_none_match: Some("\"v7\""), if_modified_since: Some("x"), ..get() }, &file(5)), Plan::NotModified);
}

#[test]
fn current_wins_over_any_range() {
    for value in ["bytes=0-1", "bytes=500-", "junk"] {
        check!(value, plan(&Ask { if_none_match: Some("\"v7\""), range: Some(value), ..get() }, &file(5)), Plan::NotModified);
    }
}

#[test]
fn ranges_apply_only_to_get() {
    for method in ["HEAD", "POST", "get"] {
        check!(method, plan(&Ask { method, range: Some("bytes=0-1"), ..get() }, &file(5)), Plan::Whole);
    }
    check!("HEAD can still be not modified", plan(&Ask { method: "HEAD", if_none_match: Some("*"), ..get() }, &file(5)), Plan::NotModified);
}

#[test]
fn open_and_clamped_ranges() {
    check!("90-", range("bytes=90-", 100), part(90, 99));
    check!("50-5000", range("bytes=50-5000", 100), part(50, 99));
    check!("99-99", range("bytes=99-99", 100), part(99, 99));
    check!("-500", range("bytes=-500", 100), part(0, 99));
    check!("spaces around", range("  bytes=1-2 ", 100), part(1, 2));
}

#[test]
fn unsatisfiable_ranges() {
    for value in ["bytes=100-200", "bytes=101-", "bytes=-0"] {
        check!(value, range(value, 100), Plan::Unsatisfiable);
    }
}

#[test]
fn an_empty_file_has_no_bytes_to_give() {
    for value in ["bytes=0-", "bytes=0-0", "bytes=-1", "bytes=-0", "bytes=5-9"] {
        check!(value, range(value, 0), Plan::Unsatisfiable);
    }
    check!("whole", plan(&get(), &file(0)), Plan::Whole);
}

#[test]
fn ignored_values() {
    for value in ["bytes=5-2", "bytes=1-2,4-5", "bytes=0-1,", "items=0-1", "bytes=", "bytes=-", "bytes", "bytes=+1-2", "bytes=1-+2", "bytes=-+3", "bytes=1 -2", "bytes=--5", "BYTES0-1", "", "bytes=150-120"] {
        check!(value, range(value, 100), Plan::Whole);
    }
}

#[test]
fn numbers_of_any_size() {
    let huge = "99999999999999999999999999999999";
    check!("huge end", range(&format!("bytes=0-{huge}"), 100), part(0, 99));
    check!("huge start", range(&format!("bytes={huge}-"), 100), Plan::Unsatisfiable);
    check!("huge suffix", range(&format!("bytes=-{huge}"), 100), part(0, 99));
}

#[test]
fn the_largest_file() {
    let max = u64::MAX;
    check!("last byte", range("bytes=-1", max), part(max - 1, max - 1));
    check!("from zero", range("bytes=0-", max), part(0, max - 1));
    check!("past the end", range(&format!("bytes={max}-"), max), Plan::Unsatisfiable);
}

#[test]
fn content_range_text() {
    check!("none for the others", (content_range(Plan::Whole, 5), content_range(Plan::NotModified, 5)), (None, None));
    check!("part", content_range(part(2, 3), 9), Some("bytes 2-3/9".to_string()));
}

#[test]
fn random_ranges_against_a_model() {
    let mut rng = anneal_prelude::Rng::new(71301);
    for _ in 0..2000 {
        let len = rng.below(30) as u64;
        let (a, b) = (rng.below(40) as u64, rng.below(40) as u64);
        let (value, wanted): (String, Option<Vec<u64>>) = match rng.below(3) {
            0 => (format!("bytes={a}-{b}"), (a <= b).then(|| (0..len).filter(|i| (a..=b).contains(i)).collect())),
            1 => (format!("bytes={a}-"), Some((a..len).collect())),
            _ => (format!("bytes=-{a}"), Some((0..len).filter(|i| len - i <= a).collect())),
        };
        let want = match wanted {
            None => Plan::Whole,
            Some(bytes) if bytes.is_empty() => Plan::Unsatisfiable,
            Some(bytes) => part(bytes[0], bytes[bytes.len() - 1]),
        };
        check!(format!("{value} of {len} bytes"), range(&value, len), want);
    }
}
