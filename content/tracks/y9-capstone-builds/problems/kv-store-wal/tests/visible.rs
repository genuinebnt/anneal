use solution::*;

#[test]
fn put_get_delete() {
    check!(r#"the basic map behaviour"#, { let mut s = Store::new(); s.put(b"a", b"1"); s.put(b"b", b"2"); s.put(b"a", b"3"); s.delete(b"b"); (s.get(b"a").map(|v| v.to_vec()), s.get(b"b").map(|v| v.to_vec()), s.len()) }, (Some(b"3".to_vec()), None, 1));
}

#[test]
fn a_put_record_layout() {
    check!(r#"header and payload"#, { let mut s = Store::new(); s.put(b"k", b"vv"); let l = s.log().to_vec(); (l.len(), l[4..12].to_vec(), l[12..].to_vec(), u32::from_le_bytes(l[0..4].try_into().unwrap()) == checksum(&l[4..])) }, (15, vec![1, 0, 0, 0, 2, 0, 0, 0], b"kvv".to_vec(), true));
}

#[test]
fn a_delete_is_a_tombstone_record() {
    check!(r#"vlen is u32::MAX, no value bytes"#, { let mut s = Store::new(); s.delete(b"k"); let l = s.log().to_vec(); (l.len(), l[8..12].to_vec()) }, (13, vec![255, 255, 255, 255]));
}

#[test]
fn recover_replays_the_log() {
    check!(r#"same contents, all bytes valid"#, { let mut s = Store::new(); s.put(b"a", b"1"); s.put(b"b", b"2"); s.delete(b"a"); let (r, n) = Store::recover(s.log()); (r.get(b"a").is_none(), r.get(b"b").map(|v| v.to_vec()), r.len(), n == s.log().len(), r.log() == s.log()) }, (true, Some(b"2".to_vec()), 1, true, true));
}

#[test]
fn a_torn_tail_is_dropped() {
    check!(r#"cut in the middle of the last record"#, { let mut s = Store::new(); s.put(b"a", b"1"); let good = s.log().len(); s.put(b"b", b"22"); let cut = &s.log()[..s.log().len() - 1]; let (r, n) = Store::recover(cut); (r.get(b"a").map(|v| v.to_vec()), r.get(b"b").is_none(), n == good) }, (Some(b"1".to_vec()), true, true));
}

#[test]
fn a_flipped_bit_stops_recovery() {
    check!(r#"checksum catches it"#, { let mut s = Store::new(); s.put(b"a", b"1"); let good = s.log().len(); s.put(b"b", b"2"); s.put(b"c", b"3"); let mut bad = s.log().to_vec(); bad[good + 13] ^= 1; let (r, n) = Store::recover(&bad); (r.len(), r.get(b"c").is_none(), n == good) }, (1, true, true));
}
