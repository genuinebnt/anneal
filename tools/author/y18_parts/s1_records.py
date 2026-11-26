from y17_parts.common import broken
from y18_parts.code import USES_1, TYPES, CODEC

SOLUTION = USES_1 + TYPES + CODEC

STARTER = USES_1 + TYPES + r'''
/// CRC-32 as used by zlib, Ethernet and PNG (reflected polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    todo!()
}

pub fn encode_record(key: &[u8], value: Option<&[u8]>, out: &mut Vec<u8>) {
    todo!()
}

pub fn decode_record(buf: &[u8], pos: &mut usize) -> Result<Entry> {
    todo!()
}

pub fn encode_frame(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    todo!()
}

/// The records of every whole frame at the front of `log`, and the number of bytes those frames take.
pub fn replay(log: &[u8]) -> Result<(Vec<Entry>, usize)> {
    todo!()
}
'''

HARNESS = r'''
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
'''

VISIBLE = HARNESS + r'''
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
'''

HIDDEN = HARNESS + r'''
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
'''

PROBLEM = dict(
    slug="records-and-frames", title="Capstone 1: records and frames", level="medium", stage="on-disk",
    tags=["binary format", "CRC-32", "decoding", "torn writes", "thiserror"],
    statement=r'''
**Capstone, step 1 of 7.** Over seven steps you build a key-value store that keeps its data in files and recovers after a crash. It supports put, delete, get and range scans. This step defines the bytes of its log.

**`crc32`** is the common CRC-32: reflected polynomial `0xEDB88320`, start with all ones, invert at the end. `crc32(b"123456789")` is `0xCBF43926`.

**A record** is one put or one delete. All numbers are little-endian:

```
tag        u8, 1 put, 0 delete
key_len    u32
key
value_len  u32
value
```

A delete has a value length of 0. Any other tag, or a delete with a value, is invalid. `decode_record` reads the record at `*pos` and moves `pos` past it. On any error it returns `KvError::Corrupt` and leaves `pos` alone. No input may make it panic.

**A frame** wraps one record for the log: `len` (u32), `crc` (u32), then the record, where `len` and `crc` describe the record's bytes.

**`replay(log)`** reads frames from the start and returns their records, with the number of bytes they take. A crash can leave a damaged end, so the log **ends** without error at the first of these:

- fewer than 8 bytes left, or fewer than `len` after the header;
- a `len` of 0 (no record is empty, and some crashes leave a tail of zeros);
- a checksum that does not match.

A frame whose checksum **does** match holds the bytes that were written. If those are not exactly one record, return `Corrupt`: that is a bug or real damage, not a torn write.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;", crates=["thiserror"],
    hints=[
        ("approach", "Write one helper that hands out the next `n` bytes or an error, and build all decoding on it. In `replay`, decide for each way of failing whether the log has merely ended or something is wrong."),
        ("rust", "`checked_add` and `slice.get(a..b)` turn every bad length into `None`. Decode with a local copy of the position and store it only on success. `let (Ok(a), Ok(b)) = .. else` fits the header."),
        ("edge", "Work out `crc32` of an empty slice, then look at what eight zero bytes would mean as a frame."),
    ],
    notes=("All reads go through one bounds-checked helper, so no length from disk can cause a panic. Replay separates the two kinds of failure: a short or mismatched frame is the torn end of the log, while a matching checksum around a bad record is reported. A zero length is treated as the end, since the checksum of nothing is zero.", "O(bytes)", "O(records)"),
    follow_up="1) The checksum does not cover `len`. What can one flipped bit in `len` do, and how would you detect it? 2) A flipped bit in the middle of the log silently drops every later record. How could a reader tell that apart from a torn end? 3) Why would a table-driven CRC be faster, and what would it cost?",
    teaches=["Give every read of file bytes one bounds-checked path.", "Tell a torn write apart from corruption: one ends the log, the other is an error.", "Test a checksum format on all zeros: the checksum of nothing is zero."],
    wrong={
        "zero_tail_is_corruption": broken(SOLUTION, ("        if len == 0 {\n            break;\n        }\n", "")),
        "bad_payload_is_a_torn_tail": broken(SOLUTION, ("        let entry = decode_record(payload, &mut used)?;\n        if used != payload.len() {\n            return Err(KvError::Corrupt(\"bytes after the record\"));\n        }\n", "        let Ok(entry) = decode_record(payload, &mut used) else {\n            break;\n        };\n")),
        "extra_payload_bytes_ignored": broken(SOLUTION, ("        if used != payload.len() {\n            return Err(KvError::Corrupt(\"bytes after the record\"));\n        }\n", "")),
        "position_moves_on_error": broken(SOLUTION, ("    let mut at = *pos;\n    let tag = take(buf, &mut at, 1)?[0];\n    let key_len = take_u32(buf, &mut at)? as usize;\n    let key = take(buf, &mut at, key_len)?;\n    let value_len = take_u32(buf, &mut at)? as usize;\n    let value = take(buf, &mut at, value_len)?;\n", "    let tag = take(buf, pos, 1)?[0];\n    let key_len = take_u32(buf, pos)? as usize;\n    let key = take(buf, pos, key_len)?;\n    let value_len = take_u32(buf, pos)? as usize;\n    let value = take(buf, pos, value_len)?;\n    let at = *pos;\n")),
        "delete_may_carry_a_value": broken(SOLUTION, ("        0 if value.is_empty() => None,", "        0 => None,")),
        "crc_without_the_final_inversion": broken(SOLUTION, ("    !crc\n}", "    crc\n}")),
    },
)
