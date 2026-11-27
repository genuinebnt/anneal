use solution::*;

#[derive(Debug, PartialEq)]
struct NotClone(u32);

#[test]
fn items_need_no_traits() {
    let mut j = Journal::new();
    j.append("a", 0, vec![NotClone(1), NotClone(2)]).unwrap();
    check!("second", j.read_stream("a", 1).map(|r| r.item.0).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn a_refused_write_uses_no_position() {
    let mut j = Journal::new();
    j.append("a", 0, vec![1]).unwrap();
    let _ = j.append("a", 0, vec![2, 3]);
    let _ = j.append("b", 5, vec![4]);
    check!("next", j.append("b", 0, vec![5]), Ok(1..2));
    check!("streams", j.streams(), vec!["a", "b"]);
}

#[test]
fn a_refused_write_to_a_new_stream_leaves_no_stream() {
    let mut j = Journal::new();
    check!("refused", j.append("ghost", 3, vec![1]), Err(Conflict { expected: 3, actual: 0 }));
    check!("streams", j.streams(), Vec::<&str>::new());
}

#[test]
fn an_empty_batch_names_the_next_free_position() {
    let mut j = Journal::new();
    j.append("a", 0, vec![1, 2]).unwrap();
    check!("empty", j.append("a", 2, vec![]), Ok(2..2));
    check!("version unchanged", j.version("a"), 2);
}

#[test]
fn similar_names_are_different_streams() {
    let mut j = Journal::new();
    for (i, name) in ["", "a", "A", "a ", "é"].iter().enumerate() {
        check!(*name, j.append(name, 0, vec![i]), Ok(i as u64..i as u64 + 1));
    }
    check!("count", j.streams().len(), 5);
}

#[test]
fn the_name_asked_for_may_be_short_lived() {
    let mut j = Journal::new();
    j.append("a", 0, vec![7]).unwrap();
    // the records borrow the journal, not the name used to ask for them
    let found: Vec<&Record<i32>> = {
        let name = String::from("a");
        j.read_stream(&name, 0).collect()
    };
    check!("found", found.len(), 1);
}

#[test]
fn huge_expected_versions() {
    let mut j: Journal<u8> = Journal::new();
    check!("max", j.append("a", u64::MAX, vec![1]), Err(Conflict { expected: u64::MAX, actual: 0 }));
    j.append("a", 0, vec![1]).unwrap();
    // 1 + 2^32 is not 1
    check!("wraps", j.append("a", (1 << 32) + 1, vec![1]), Err(Conflict { expected: (1 << 32) + 1, actual: 1 }));
    check!("read", j.read_stream("a", 1 << 32).count(), 0);
}

#[test]
fn random_writes_match_a_plain_list() {
    let mut rng = anneal_prelude::Rng::new(7105);
    for _ in 0..200 {
        let mut j = Journal::new();
        let mut plain: Vec<(String, u32)> = Vec::new();
        let mut trace = Vec::new();
        for step in 0..30u32 {
            let stream = rng.pick(&["x", "y", "z"]).to_string();
            let have = plain.iter().filter(|(s, _)| *s == stream).count() as u64;
            let expected = if rng.below(4) == 0 { rng.below(5) as u64 } else { have };
            let batch: Vec<u32> = (0..rng.below(4) as u32).map(|i| step * 10 + i).collect();
            trace.push(format!("append {stream} {expected} {batch:?}"));
            let want = if expected == have {
                let first = plain.len() as u64;
                plain.extend(batch.iter().map(|&n| (stream.clone(), n)));
                Ok(first..plain.len() as u64)
            } else {
                Err(Conflict { expected, actual: have })
            };
            check!(trace.join("; "), j.append(&stream, expected, batch), want);
            let from = rng.below(plain.len() + 2);
            let got: Vec<(u64, String, u32)> = j.read_all(from as u64).iter().map(|r| (r.pos, r.stream.clone(), r.item)).collect();
            let want: Vec<(u64, String, u32)> = plain.iter().enumerate().skip(from).map(|(i, (s, n))| (i as u64, s.clone(), *n)).collect();
            check!(trace.join("; "), got, want);
            let after = rng.below(6);
            let got: Vec<(u64, u32)> = j.read_stream(&stream, after as u64).map(|r| (r.version, r.item)).collect();
            let want: Vec<(u64, u32)> = plain.iter().filter(|(s, _)| *s == stream).enumerate().skip(after).map(|(i, (_, n))| (i as u64 + 1, *n)).collect();
            check!(trace.join("; "), got, want);
        }
    }
}

#[test]
fn many_streams_stay_quick() {
    let mut j = Journal::new();
    let n = 100_000u32;
    let names: Vec<String> = (0..n).map(|i| format!("s{i}")).collect();
    for (i, name) in names.iter().enumerate() {
        j.append(name, 0, vec![i as u32]).unwrap();
        check!("version", j.version(name), 1);
    }
    for (i, name) in names.iter().enumerate() {
        let found: Vec<u32> = j.read_stream(name, 0).map(|r| r.item).collect();
        assert_eq!(found, vec![i as u32]);
    }
}

#[test]
fn one_long_stream_stays_quick() {
    let mut j = Journal::new();
    let n = 100_000u64;
    for v in 0..n {
        j.append("busy", v, vec![v]).unwrap();
        // only the new tail is asked for
        assert_eq!(j.read_stream("busy", v).count(), 1);
    }
    check!("version", j.version("busy"), n);
}
