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
fn the_checksum_is_the_standard_one() {
    check!("crc32 of 123456789, of nothing, and of a", (crc32(b"123456789"), crc32(b""), crc32(b"a")), (0xCBF4_3926, 0, 0xE8B7_BE43));
}

#[test]
fn record_bytes() {
    let mut out = vec![9];
    encode_record(b"ab", Some(b"c"), &mut out);
    encode_record(b"d", None, &mut out);
    check!("put ab=c, then delete d, after a byte already there", out, vec![9, 1, 2, 0, 0, 0, b'a', b'b', 1, 0, 0, 0, b'c', 0, 1, 0, 0, 0, b'd', 0, 0, 0, 0]);
}

#[test]
fn frames_replay_in_order() {
    let entries = vec![put("a", "1"), del("a"), put("b", "")];
    let log = log_of(&entries);
    check!(format!("{entries:?}"), replayed(&log), Some((entries.clone(), log.len())));
}

#[test]
fn a_cut_frame_ends_the_log() {
    let mut log = log_of(&[put("a", "1")]);
    let whole = log.len();
    log.extend_from_slice(&encode_frame(b"b", Some(b"2"))[..10]);
    check!("one frame and ten bytes of the next", replayed(&log), Some((vec![put("a", "1")], whole)));
}

#[test]
fn a_tail_of_zeros_ends_the_log() {
    let mut log = log_of(&[put("a", "1")]);
    let whole = log.len();
    log.extend([0; 64]);
    check!("one frame, then 64 zero bytes", replayed(&log), Some((vec![put("a", "1")], whole)));
}

#[test]
fn a_good_checksum_around_nonsense_is_corruption() {
    let log = framed(&[7, 0, 0, 0, 0, 0, 0, 0, 0]);
    check!("tag 7", matches!(replay(&log), Err(KvError::Corrupt(_))), true);
}
