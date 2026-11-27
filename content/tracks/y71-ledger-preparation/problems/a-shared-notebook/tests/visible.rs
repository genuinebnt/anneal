use solution::*;

#[allow(dead_code)]
fn items<'a>(records: impl Iterator<Item = &'a Record<&'static str>>) -> Vec<&'static str> {
    records.map(|r| r.item).collect()
}

#[test]
fn versions_count_per_stream_and_positions_over_all() {
    let mut j = Journal::new();
    check!("first", j.append("day", 0, vec!["a", "b"]), Ok(0..2));
    check!("other stream", j.append("night", 0, vec!["c"]), Ok(2..3));
    check!("again", j.append("day", 2, vec!["d"]), Ok(3..4));
    check!("day version", j.version("day"), 3);
    let last = j.read_all(3)[0].clone();
    check!("last record", (last.pos, last.stream.as_str(), last.version, last.item), (3, "day", 3, "d"));
}

#[test]
fn a_blind_write_is_refused_as_a_whole() {
    let mut j = Journal::new();
    j.append("day", 0, vec!["a"]).unwrap();
    check!("stale", j.append("day", 0, vec!["x", "y"]), Err(Conflict { expected: 0, actual: 1 }));
    check!("ahead", j.append("day", 2, vec!["x"]), Err(Conflict { expected: 2, actual: 1 }));
    check!("nothing was added", j.read_all(0).len(), 1);
}

#[test]
fn reading_after_a_version() {
    let mut j = Journal::new();
    j.append("day", 0, vec!["a", "b", "c"]).unwrap();
    check!("after 0", items(j.read_stream("day", 0)), vec!["a", "b", "c"]);
    check!("after 2", items(j.read_stream("day", 2)), vec!["c"]);
    check!("after 3", items(j.read_stream("day", 3)), Vec::<&str>::new());
}

#[test]
fn reading_past_the_end() {
    let mut j: Journal<&str> = Journal::new();
    check!("empty journal", j.read_all(0).len(), 0);
    check!("unknown stream", items(j.read_stream("nobody", 0)), Vec::<&str>::new());
    j.append("day", 0, vec!["a"]).unwrap();
    check!("far past", j.read_all(u64::MAX).len(), 0);
    check!("far past in a stream", j.read_stream("day", u64::MAX).count(), 0);
}

#[test]
fn an_empty_batch_is_checked_and_adds_nothing() {
    let mut j: Journal<&str> = Journal::new();
    check!("on a new stream", j.append("day", 0, vec![]), Ok(0..0));
    check!("no stream appears", j.streams(), Vec::<&str>::new());
    check!("still checked", j.append("day", 1, vec![]), Err(Conflict { expected: 1, actual: 0 }));
}

#[test]
fn stream_names_are_sorted() {
    let mut j = Journal::new();
    for name in ["m", "a", "z"] {
        j.append(name, 0, vec![1]).unwrap();
    }
    check!("names", j.streams(), vec!["a", "m", "z"]);
}
