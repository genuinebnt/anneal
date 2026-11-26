use solution::*;

/// A frame around any payload, with the right length and checksum.
#[allow(dead_code)]
fn framed(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_le_bytes().to_vec();
    out.extend(crc32(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

#[allow(dead_code)]
fn put(k: &str, v: &str) -> Entry {
    (k.as_bytes().to_vec(), Some(v.as_bytes().to_vec()))
}

#[allow(dead_code)]
fn del(k: &str) -> Entry {
    (k.as_bytes().to_vec(), None)
}

#[allow(dead_code)]
fn log_of(entries: &[Entry]) -> Vec<u8> {
    entries.iter().flat_map(|(k, v)| encode_frame(k, v.as_deref())).collect()
}

#[allow(dead_code)]
fn replayed(log: &[u8]) -> Option<(Vec<Entry>, usize)> {
    replay(log).ok()
}

#[test]
fn more_checksums() {
    check!("the quick brown fox", crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
    check!("four zeros", crc32(&[0; 4]), 0x2144_DF1C);
    check!("four 0xff", crc32(&[0xff; 4]), 0xFFFF_FFFF);
}

#[test]
fn decode_moves_past_one_record_only() {
    let mut bytes = Vec::new();
    encode_record(b"k", Some(b"v"), &mut bytes);
    encode_record(b"gone", None, &mut bytes);
    let mut pos = 0;
    let first = decode_record(&bytes, &mut pos).ok();
    check!("first record", (first, pos), (Some(put("k", "v")), 11));
    let second = decode_record(&bytes, &mut pos).ok();
    check!("second record", (second, pos), (Some(del("gone")), bytes.len()));
}

#[test]
fn bad_records_leave_the_position_alone() {
    let mut good = Vec::new();
    encode_record(b"key", Some(b"value"), &mut good);
    for keep in 0..good.len() {
        let mut pos = 0;
        let got = decode_record(&good[..keep], &mut pos);
        check!(format!("first {keep} bytes of a record"), (matches!(got, Err(KvError::Corrupt(_))), pos), (true, 0));
    }
    for bad in [vec![2, 0, 0, 0, 0, 0, 0, 0, 0], vec![0, 1, 0, 0, 0, b'k', 1, 0, 0, 0, b'v'], vec![1, 255, 255, 255, 255, 0, 0, 0, 0], vec![1, 0, 0, 0, 0, 255, 255, 255, 255, 1]] {
        let mut pos = 0;
        let got = decode_record(&bad, &mut pos);
        check!(format!("{bad:?}"), (matches!(got, Err(KvError::Corrupt(_))), pos), (true, 0));
    }
    for start in [good.len(), good.len() + 1, usize::MAX] {
        let mut pos = start;
        check!(format!("starting at {start}"), (decode_record(&good, &mut pos).is_err(), pos), (true, start));
    }
}

#[test]
fn empty_key_and_empty_value() {
    let entries = vec![put("", ""), del("")];
    let log = log_of(&entries);
    check!("an empty value is not a delete", replayed(&log), Some((entries.clone(), log.len())));
}

#[test]
fn zeros_of_every_length() {
    for n in 0..40 {
        let mut log = log_of(&[put("x", "y"), del("x")]);
        let whole = log.len();
        log.extend(std::iter::repeat_n(0, n));
        check!(format!("two frames and {n} zeros"), replayed(&log), Some((vec![put("x", "y"), del("x")], whole)));
        check!(format!("{n} zeros alone"), replayed(&vec![0; n]), Some((vec![], 0)));
    }
}

#[test]
fn every_cut_gives_the_whole_frames() {
    let entries = vec![put("a", "1"), put("bb", "two"), del("a"), put("c", "")];
    let log = log_of(&entries);
    let mut ends = vec![0];
    for i in 1..=entries.len() {
        ends.push(log_of(&entries[..i]).len());
    }
    for cut in 0..=log.len() {
        let n = ends.iter().rposition(|&e| e <= cut).unwrap();
        check!(format!("cut at {cut}"), replayed(&log[..cut]), Some((entries[..n].to_vec(), ends[n])));
    }
}

#[test]
fn every_flipped_bit_stops_before_its_frame() {
    let entries = vec![put("a", "1"), put("bb", "two"), put("c", "3")];
    let log = log_of(&entries);
    let ends = [0, log_of(&entries[..1]).len(), log_of(&entries[..2]).len()];
    for at in 0..log.len() {
        for bit in 0..8 {
            let mut bad = log.clone();
            bad[at] ^= 1 << bit;
            let frame = ends.iter().rposition(|&e| e <= at).unwrap();
            check!(format!("bit {bit} of byte {at}"), replayed(&bad), Some((entries[..frame].to_vec(), ends[frame])));
        }
    }
}

#[test]
fn a_checked_payload_must_be_exactly_one_record() {
    let mut two = Vec::new();
    encode_record(b"a", Some(b"1"), &mut two);
    let one = two.len();
    encode_record(b"b", Some(b"2"), &mut two);
    for payload in [two.clone(), two[..one + 1].to_vec(), two[..one - 1].to_vec(), vec![1]] {
        let mut log = log_of(&[put("ok", "ok")]);
        log.extend(framed(&payload));
        check!(format!("payload of {} bytes", payload.len()), matches!(replay(&log), Err(KvError::Corrupt(_))), true);
    }
}

#[test]
fn a_huge_length_is_only_a_cut_frame() {
    let mut log = u32::MAX.to_le_bytes().to_vec();
    log.extend([1, 2, 3, 4, 5, 6]);
    check!("length u32::MAX with two bytes after the header", replayed(&log), Some((vec![], 0)));
}

#[test]
fn random_logs_and_random_bytes() {
    let mut rng = anneal_prelude::Rng::new(180101);
    for case in 0..400 {
        let n = rng.below(6);
        let mut entries: Vec<Entry> = Vec::new();
        for _ in 0..n {
            let klen = rng.below(5);
            let key: Vec<u8> = rng.vec(klen, 0, 255);
            let value = if rng.bool() {
                let vlen = rng.below(8);
                Some(rng.vec(vlen, 0, 255))
            } else {
                None
            };
            entries.push((key, value));
        }
        let mut log = log_of(&entries);
        check!(format!("case {case}: {entries:?}"), replayed(&log), Some((entries.clone(), log.len())));
        let junk_len = rng.below(30);
        let junk: Vec<u8> = rng.vec(junk_len, 0, 255);
        let whole = log.len();
        log.extend(&junk);
        check!(format!("case {case}: junk {junk:?}"), replayed(&log), Some((entries.clone(), whole)));
        let mut pos = 0;
        let _ = decode_record(&junk, &mut pos);
        let _ = replay(&junk);
    }
}

#[test]
fn a_big_value() {
    let value = vec![5u8; 1 << 20];
    let log = encode_frame(b"big", Some(&value));
    check!("1 MiB", replayed(&log).map(|(e, n)| (e[0].1 == Some(value.clone()), n)), Some((true, log.len())));
}
