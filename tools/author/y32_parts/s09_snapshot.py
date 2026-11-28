from y32_parts.common import *
from y32_parts.s08_log import HELPERS as LOG_HELPERS

SOLUTION = solution(9)

HELPERS = LOG_HELPERS + r'''
/// A core after these commands, each run at time 0.
#[allow(dead_code)]
fn core_with(commands: &[&str]) -> Core {
    let mut core = Core::new();
    let id = core.connect();
    for line in commands {
        core.request(id, args(line), 0);
    }
    core.disconnect(id);
    core.take_log();
    core
}
#[allow(dead_code)]
const EVERY_KIND: &[&str] = &[
    "SET s hello", "SET t gone PX 10", "SET u later PX 9000", "RPUSH l c a b a", "HSET h b 2 a 1", "SADD set z y x",
    "ZADD z 2.5 b -inf a inf c 2.5 a2", "PEXPIRE z 7000",
];
/// Runs a command, writes what it logged, and returns the reply.
#[allow(dead_code)]
fn durable(aof: &mut Aof<MemDisk>, core: &mut Core, id: ClientId, now: u64, line: &str) -> Frame {
    let out = core.request(id, args(line), now);
    aof.write(&core.take_log()).expect("write");
    out[0].frame.clone()
}
/// The CRC-32 the snapshot ends with.
#[allow(dead_code)]
fn crc(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
#[allow(dead_code)]
fn sealed(mut body: Vec<u8>) -> Vec<u8> {
    let sum = crc(&body);
    body.extend(sum.to_le_bytes());
    body
}
#[allow(dead_code)]
fn blob(text: &[u8]) -> Vec<u8> {
    let mut out = (text.len() as u32).to_le_bytes().to_vec();
    out.extend(text);
    out
}
#[allow(dead_code)]
fn recovered(disk: &MemDisk) -> (Aof<MemDisk>, Core) {
    match recover(disk.clone(), SyncPolicy::Always) {
        Ok(r) => r,
        Err(e) => panic!("recover failed: {e:?}, files {:?}", disk.names()),
    }
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn a_snapshot_byte_for_byte() {
    let core = core_with(&["SET k v", "RPUSH list a b", "PEXPIREAT list 258"]);
    let mut body = b"RSNAP1".to_vec();
    // keys in byte order: type, deadline flag, [deadline], key, value
    body.extend([0, 0]);
    body.extend(blob(b"k"));
    body.extend(blob(b"v"));
    body.extend([1, 1]);
    body.extend(258u64.to_le_bytes());
    body.extend(blob(b"list"));
    body.extend(2u32.to_le_bytes());
    body.extend(blob(b"a"));
    body.extend(blob(b"b"));
    body.push(0xFF);
    check!("encode", encode_snapshot(&core, 0), sealed(body));
}

#[test]
fn a_snapshot_round_trip() {
    let mut core = core_with(EVERY_KIND);
    let bytes = encode_snapshot(&core, 100);
    let mut again = decode_snapshot(&bytes).expect("our own snapshot");
    check!("the same keys", dump(&mut again, 100), dump(&mut core, 100));
    check!("decoding logs nothing", again.take_log().len(), 0);
}

#[test]
fn a_damaged_snapshot_is_refused() {
    let bytes = encode_snapshot(&core_with(&["SET k v"]), 0);
    let mut flipped = bytes.clone();
    flipped[8] ^= 1;
    check!("a flipped bit", decode_snapshot(&flipped).is_err(), true);
    check!("a missing byte", decode_snapshot(&bytes[..bytes.len() - 1]).is_err(), true);
    check!("nothing at all", decode_snapshot(b"").is_err(), true);
}

#[test]
fn a_rewrite_shrinks_the_log_and_keeps_later_writes() {
    let disk = MemDisk::default();
    let (mut aof, mut core) = open(&disk);
    let id = core.connect();
    for _ in 0..50 {
        durable(&mut aof, &mut core, id, 0, "INCR n");
    }
    let before = disk.file(AOF).unwrap().len();
    let base = aof.begin_rewrite(&core, 0).expect("begin");
    durable(&mut aof, &mut core, id, 0, "INCR n");
    aof.finish_rewrite(base).expect("finish");
    check!("smaller", disk.file(AOF).unwrap().len() < before / 5, true);
    durable(&mut aof, &mut core, id, 0, "INCR n");
    let (_, mut again) = open(&disk);
    check!("every increment, once", dump(&mut again, 0), dump(&mut core, 0));
}

#[test]
fn recovery_prefers_the_log() {
    let disk = MemDisk::default();
    save_snapshot(&mut disk.clone(), &core_with(&["SET from snapshot"]), 0).expect("save");
    let (mut aof, mut core) = recovered(&disk);
    check!("the snapshot", dump(&mut core, 0), dump(&mut core_with(&["SET from snapshot"]), 0));
    let id = core.connect();
    durable(&mut aof, &mut core, id, 0, "SET more data");
    let (_, mut again) = recovered(&disk);
    check!("later writes are not lost to the older snapshot", dump(&mut again, 0), dump(&mut core, 0));
}
''',
]

HIDDEN = [
    HELPERS,
    DICE,
    r'''
#[test]
fn every_kind_in_the_snapshot_format() {
    let core = core_with(&["HSET h f v", "SADD s m", "ZADD z 1.5 m"]);
    let mut body = b"RSNAP1".to_vec();
    body.extend([2, 0]);
    body.extend(blob(b"h"));
    body.extend(1u32.to_le_bytes());
    body.extend(blob(b"f"));
    body.extend(blob(b"v"));
    body.extend([3, 0]);
    body.extend(blob(b"s"));
    body.extend(1u32.to_le_bytes());
    body.extend(blob(b"m"));
    body.extend([4, 0]);
    body.extend(blob(b"z"));
    body.extend(1u32.to_le_bytes());
    body.extend(1.5f64.to_bits().to_le_bytes());
    body.extend(blob(b"m"));
    body.push(0xFF);
    check!("encode", encode_snapshot(&core, 0), sealed(body));
    let mut empty = b"RSNAP1".to_vec();
    empty.push(0xFF);
    check!("an empty keyspace", encode_snapshot(&Core::new(), 0), sealed(empty));
}

#[test]
fn a_sorted_set_is_saved_in_its_order_and_lists_as_they_are() {
    let core = core_with(&["ZADD z 2 a 1 b"]);
    let bytes = encode_snapshot(&core, 0);
    let position = |needle: &[u8]| bytes.windows(needle.len()).position(|w| w == needle);
    check!("b, with the lower score, comes first", position(&blob(b"b")) < position(&blob(b"a")), true);
}

#[test]
fn expired_keys_are_left_out() {
    let core = core_with(&["SET gone v PX 10", "SET kept v PX 11"]);
    let mut again = decode_snapshot(&encode_snapshot(&core, 10)).expect("decode");
    check!("only the live key, even at time 0", dump(&mut again, 0).len(), 1);
}

#[test]
fn every_truncation_and_bit_flip_is_refused() {
    let bytes = encode_snapshot(&core_with(EVERY_KIND), 0);
    for cut in 0..bytes.len() {
        check!(format!("cut at {cut}"), decode_snapshot(&bytes[..cut]).is_err(), true);
    }
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut bad = bytes.clone();
            bad[at] ^= 1 << bit;
            check!(format!("byte {at}, bit {bit}"), decode_snapshot(&bad).is_err(), true);
        }
    }
}

#[test]
fn a_correct_checksum_does_not_make_bad_contents_good() {
    let build = |parts: &[&[u8]]| {
        let mut body = b"RSNAP1".to_vec();
        for p in parts {
            body.extend(*p);
        }
        sealed(body)
    };
    let one = 1u32.to_le_bytes();
    let zero = 0u32.to_le_bytes();
    let lie = u32::MAX.to_le_bytes();
    let k = blob(b"k");
    let v = blob(b"v");
    let nan = f64::NAN.to_bits().to_le_bytes();
    let score = 1f64.to_bits().to_le_bytes();
    let string_k: Vec<u8> = [&[0u8, 0][..], &k, &v].concat();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("no end marker", build(&[&string_k])),
        ("bytes after the end", build(&[&string_k, &[0xFF, 0]])),
        ("an unknown type", build(&[&[5, 0], &k, &one, &v, &[0xFF]])),
        ("a deadline flag of 2", build(&[&[0, 2], &k, &v, &[0xFF]])),
        ("a key length that lies", build(&[&[0, 0], &lie, b"k", &[0xFF]])),
        ("a list count that lies", build(&[&[1, 0], &k, &lie, &v, &[0xFF]])),
        ("a hash count that lies", build(&[&[2, 0], &k, &lie, &[0xFF]])),
        ("an empty list", build(&[&[1, 0], &k, &zero, &[0xFF]])),
        ("an empty sorted set", build(&[&[4, 0], &k, &zero, &[0xFF]])),
        ("an empty hash", build(&[&[2, 0], &k, &zero, &[0xFF]])),
        ("an empty set", build(&[&[3, 0], &k, &zero, &[0xFF]])),
        ("a score that is NaN", build(&[&[4, 0], &k, &one, &nan, &v, &[0xFF]])),
        ("a score with the bits of -0.0", build(&[&[4, 0], &k, &one, &(-0.0f64).to_bits().to_le_bytes(), &v, &[0xFF]])),
        ("a key twice", build(&[&string_k, &string_k, &[0xFF]])),
        ("a set member twice", build(&[&[3, 0], &k, &2u32.to_le_bytes(), &v, &v, &[0xFF]])),
        ("a hash field twice", build(&[&[2, 0], &k, &2u32.to_le_bytes(), &v, &v, &v, &v, &[0xFF]])),
        ("a sorted member twice", build(&[&[4, 0], &k, &2u32.to_le_bytes(), &score, &v, &score, &v, &[0xFF]])),
        ("a wrong magic", sealed(b"RSNAP2\xff".to_vec())),
        ("keys out of byte order", build(&[&[0, 0], &blob(b"m"), &v, &string_k, &[0xFF]])),
        ("set members out of byte order", build(&[&[3, 0], &k, &2u32.to_le_bytes(), &blob(b"w"), &v, &[0xFF]])),
        ("hash fields out of byte order", build(&[&[2, 0], &k, &2u32.to_le_bytes(), &blob(b"w"), &v, &v, &v, &[0xFF]])),
        ("a higher score first", build(&[&[4, 0], &k, &2u32.to_le_bytes(), &2f64.to_bits().to_le_bytes(), &blob(b"a"), &score, &blob(b"b"), &[0xFF]])),
        ("equal scores, members out of order", build(&[&[4, 0], &k, &2u32.to_le_bytes(), &score, &blob(b"w"), &score, &v, &[0xFF]])),
    ];
    for (what, bytes) in cases {
        check!(what, decode_snapshot(&bytes).is_err(), true);
    }
    let good = build(&[&string_k, &[0xFF]]);
    check!("the same pieces put right are accepted", decode_snapshot(&good).is_ok(), true);
    let ordered = build(&[
        &string_k,
        &[3, 0], &blob(b"m"), &2u32.to_le_bytes(), &v, &blob(b"w"),
        &[4, 0], &blob(b"z"), &2u32.to_le_bytes(), &score, &blob(b"b"), &2f64.to_bits().to_le_bytes(), &blob(b"a"),
        &[0xFF],
    ]);
    check!("keys, members and scores in order are accepted", decode_snapshot(&ordered).is_ok(), true);
}

#[test]
fn random_keyspaces_survive_a_snapshot_and_a_rewrite() {
    let mut rng = Rng::new(0x320c);
    for run_number in 0..150 {
        let mut core = Core::new();
        let id = core.connect();
        let mut now = 1;
        let mut history = Vec::new();
        for _ in 0..60 {
            now += rng.below(3) as u64;
            let line = random_command(&mut rng, now);
            history.push(format!("{now}: {line}"));
            core.request(id, args(&line), now);
        }
        core.take_log();
        let snapshot = encode_snapshot(&core, now);
        let commands = rewrite_commands(&core, now);
        let want = dump(&mut core, now);
        let mut from_snapshot = decode_snapshot(&snapshot).expect("our own snapshot");
        check!(format!("run {run_number}: snapshot after {history:?}"), dump(&mut from_snapshot, now), want.clone());
        let (_, mut from_log) = open(&MemDisk::with(&[(AOF, &commands)]));
        check!(format!("run {run_number}: rewrite after {history:?}"), dump(&mut from_log, now), want);
    }
}

#[test]
fn a_rewrite_has_no_record_without_bound_and_no_history() {
    let mut core = Core::new();
    let id = core.connect();
    for i in 0..1000 {
        for line in [format!("RPUSH l {i}"), format!("SADD s {i}"), format!("HSET h {i} {i}"), format!("ZADD z {i} m{i}"), "INCR n".to_string()] {
            core.request(id, args(&line), 0);
        }
    }
    core.request(id, args("SET dead v PX 1"), 0);
    core.take_log();
    let bytes = rewrite_commands(&core, 5);
    let limits = Limits { max_array: 1 << 20, ..Limits::default() };
    let (mut at, mut commands, mut longest) = (0, 0, 0);
    while at < bytes.len() {
        let (command, used) = parse_command(&bytes[at..], &limits).expect("valid commands").expect("whole commands");
        at += used;
        commands += 1;
        longest = longest.max(command.len());
        check!("no expired key", command.iter().any(|a| a == b"dead"), false);
    }
    check!(format!("the longest command has {longest} arguments"), longest <= 130, true);
    check!(format!("{commands} commands for 5 keys of 1000 elements"), commands <= 100, true);
    let (_, mut again) = open(&MemDisk::with(&[(AOF, &bytes)]));
    check!("the same keys", dump(&mut again, 5) == dump(&mut core, 5), true);
}

#[test]
fn rewrite_calls_out_of_turn() {
    let disk = MemDisk::default();
    let (mut aof, core) = open(&disk);
    check!("finish with none", matches!(aof.finish_rewrite(Vec::new()), Err(AofError::NotRewriting)), true);
    let base = aof.begin_rewrite(&core, 0).expect("begin");
    check!("a second begin", matches!(aof.begin_rewrite(&core, 0).map(|_| ()), Err(AofError::Busy)), true);
    aof.finish_rewrite(base).expect("the first one still finishes");
    aof.begin_rewrite(&core, 0).expect("begin again");
    aof.cancel_rewrite();
    aof.cancel_rewrite();
    check!("finish after cancel", matches!(aof.finish_rewrite(Vec::new()), Err(AofError::NotRewriting)), true);
    aof.begin_rewrite(&core, 0).expect("begin after cancel");
}

#[test]
fn a_cancelled_rewrite_changes_nothing_and_leaves_no_tail_behind() {
    let disk = MemDisk::default();
    let (mut aof, mut core) = open(&disk);
    let id = core.connect();
    durable(&mut aof, &mut core, id, 0, "INCR n");
    aof.begin_rewrite(&core, 0).expect("begin");
    durable(&mut aof, &mut core, id, 0, "INCR n");
    let before = disk.file(AOF);
    aof.cancel_rewrite();
    check!("the log", disk.file(AOF), before);
    let base = aof.begin_rewrite(&core, 0).expect("begin");
    aof.finish_rewrite(base).expect("finish");
    let (_, mut again) = open(&disk);
    check!("nothing counted twice", dump(&mut again, 0), dump(&mut core, 0));
}

#[test]
fn a_broken_log_cannot_be_rewritten() {
    let disk = MemDisk::default();
    let (mut aof, mut core) = open(&disk);
    let base = aof.begin_rewrite(&core, 0).expect("begin");
    let id = core.connect();
    core.request(id, args("SET a 1"), 0);
    disk.fail_in(0, false);
    check!("the write fails", aof.write(&core.take_log()).is_err(), true);
    disk.heal();
    check!("finish", matches!(aof.finish_rewrite(base), Err(AofError::Broken)), true);
    check!("begin", matches!(aof.begin_rewrite(&core, 0).map(|_| ()), Err(AofError::Broken)), true);
}

#[test]
fn a_power_cut_or_error_at_any_point_of_a_rewrite_loses_nothing() {
    for failing in 0..12 {
        for power_cut in [false, true] {
            for leftover in [None, Some(&b"*1\r\n$4\r\nPING\r\n-junk"[..])] {
                let disk = MemDisk::default();
                if let Some(junk) = leftover {
                    // an earlier rewrite that never finished left its temporary file
                    disk.0.lock().unwrap().files.insert("appendonly.aof.tmp".to_string(), MemFile { data: junk.to_vec(), synced: junk.len() });
                }
                let (mut aof, mut core) = open(&disk);
                let id = core.connect();
                durable(&mut aof, &mut core, id, 0, "INCR n");
                durable(&mut aof, &mut core, id, 0, "RPUSH l a b");
                let base = aof.begin_rewrite(&core, 0).expect("begin");
                durable(&mut aof, &mut core, id, 0, "INCR n");
                let want = dump(&mut core, 0);
                disk.fail_in(failing, power_cut);
                let result = aof.finish_rewrite(base);
                let case = format!("call {failing} of finish fails (power cut: {power_cut}, stale temp: {})", leftover.is_some());
                if power_cut {
                    for extra in [0, 7, usize::MAX] {
                        let (_, mut again) = open(&disk.after_power_cut(extra));
                        check!(format!("{case}, {extra} unsynced bytes survive"), dump(&mut again, 0), want.clone());
                    }
                } else {
                    disk.heal();
                    if let Err(e) = result {
                        check!(format!("{case}: {e:?}"), matches!(e, AofError::Io(_)), true);
                    }
                    // the log is still usable, and a rewrite can be tried again
                    durable(&mut aof, &mut core, id, 0, "INCR n");
                    let (_, mut again) = open(&disk);
                    check!(format!("{case}: then another write"), dump(&mut again, 0), dump(&mut core, 0));
                    let base = aof.begin_rewrite(&core, 0).expect("begin again");
                    aof.finish_rewrite(base).expect("finish again");
                    let (_, mut again) = open(&disk.after_power_cut(0));
                    check!(format!("{case}: after a second rewrite"), dump(&mut again, 0), dump(&mut core, 0));
                }
            }
        }
    }
}

#[test]
fn saving_a_snapshot_never_leaves_a_half_written_one() {
    let (old, new) = (core_with(&["SET version old"]), core_with(&["SET version new", "RPUSH more x y z"]));
    for failing in 0..10 {
        for leftover in [false, true] {
            let disk = MemDisk::default();
            save_snapshot(&mut disk.clone(), &old, 0).expect("first save");
            if leftover {
                disk.0.lock().unwrap().files.insert("dump.snap.tmp".to_string(), MemFile { data: b"junk".to_vec(), synced: 4 });
            }
            disk.fail_in(failing, true);
            let result = save_snapshot(&mut disk.clone(), &new, 0);
            for extra in [0, 1, 20, usize::MAX] {
                let kept = disk.after_power_cut(extra);
                let bytes = kept.file(SNAPSHOT).unwrap_or_default();
                let what = format!("call {failing} fails, then power cut with {extra} unsynced bytes");
                let ok_old = bytes == encode_snapshot(&old, 0);
                let ok_new = bytes == encode_snapshot(&new, 0);
                check!(format!("{what}: the snapshot is the old or the new one"), ok_old || ok_new, true);
                if result.is_ok() {
                    check!(format!("{what}: a save that returned Ok is kept"), ok_new, true);
                }
            }
        }
    }
}

#[test]
fn recovery_in_every_starting_state() {
    let snapshot = encode_snapshot(&core_with(&["SET s snap"]), 0);
    let log = log_of(&["SET l log"]);
    let keys = |disk: &MemDisk| {
        let (_, mut core) = recovered(disk);
        dump(&mut core, 0)
    };
    let from_log = dump(&mut core_with(&["SET l log"]), 0);
    let from_snapshot = dump(&mut core_with(&["SET s snap"]), 0);
    check!("nothing", keys(&MemDisk::default()), Vec::<String>::new());
    check!("both", keys(&MemDisk::with(&[(AOF, &log), (SNAPSHOT, &snapshot)])), from_log.clone());
    check!("an empty log", keys(&MemDisk::with(&[(AOF, b""), (SNAPSHOT, &snapshot)])), from_snapshot.clone());
    check!("a log and a damaged snapshot", keys(&MemDisk::with(&[(AOF, &log), (SNAPSHOT, b"junk")])), from_log);
    let damaged = recover(MemDisk::with(&[(SNAPSHOT, b"junk")]), SyncPolicy::Always).map(|_| ());
    check!(format!("only a damaged snapshot: {damaged:?}"), matches!(damaged, Err(AofError::BadSnapshot)), true);
}

#[test]
fn a_crash_while_starting_from_a_snapshot_loses_nothing() {
    let core = core_with(EVERY_KIND);
    let want = dump(&mut decode_snapshot(&encode_snapshot(&core, 0)).expect("decode"), 0);
    for failing in 0..10 {
        let disk = MemDisk::with(&[(SNAPSHOT, &encode_snapshot(&core, 0))]);
        disk.fail_in(failing, true);
        let _ = recover(disk.clone(), SyncPolicy::Always);
        for extra in [0, 1, 30, 100, usize::MAX] {
            let (_, mut again) = recovered(&disk.after_power_cut(extra));
            check!(format!("call {failing} fails, {extra} unsynced bytes survive: the next start"), dump(&mut again, 0), want.clone());
        }
    }
}

#[test]
fn a_real_directory() {
    let dir = std::env::temp_dir().join(format!("anneal-redis-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let want = {
        let (mut aof, mut core) = recover(DirDisk::new(&dir), SyncPolicy::Always).expect("first start");
        let id = core.connect();
        for line in ["SET a 1", "INCR a", "RPUSH l x y"] {
            core.request(id, args(line), 0);
            aof.write(&core.take_log()).expect("write");
        }
        let base = aof.begin_rewrite(&core, 0).expect("begin");
        core.request(id, args("INCR a"), 0);
        aof.write(&core.take_log()).expect("write during");
        aof.finish_rewrite(base).expect("finish");
        save_snapshot(&mut DirDisk::new(&dir), &core, 0).expect("snapshot");
        dump(&mut core, 0)
    };
    let (_, mut again) = recover(DirDisk::new(&dir), SyncPolicy::Always).expect("second start");
    check!("from the log", dump(&mut again, 0), want.clone());
    std::fs::remove_file(dir.join(AOF)).unwrap();
    let (_, mut again) = recover(DirDisk::new(&dir), SyncPolicy::Always).expect("start from the snapshot");
    check!("from the snapshot", dump(&mut again, 0), want);
    let _ = std::fs::remove_dir_all(&dir);
}
''',
]

STATEMENT = r'''
**Capstone, step 9 of 10.** The log grows for ever: a counter increased a million times is a million records. This step adds **snapshots**, compact copies of the keys, and a **rewrite** that shrinks the log while writes go on. The starter holds the reference code of steps 1 to 8, a `crc32` function, and three new errors.

@SPOILERS@

**The snapshot format.** All numbers are little-endian. A *blob* is a `u32` length and then the bytes.

```
"RSNAP1"
for each live key, in byte order:
    u8 type          0 string, 1 list, 2 hash, 3 set, 4 sorted set
    u8 has_deadline  0 or 1
    u64 deadline     only if has_deadline is 1
    blob key
    value
u8 0xFF
u32 crc32 of every byte before this one
```

A string value is a blob. The others are a `u32` count, then that many items:

| Kind | Each item | Order |
|---|---|---|
| list | blob | from the left |
| set | blob | byte order |
| hash | blob field, blob value | by field |
| sorted set | `u64` score bits, blob member | the set's order |

Score bits are `f64::to_bits`. A score is never stored as `-0.0` (it is kept as 0), so the bits of `-0.0` are not this format.

**`encode_snapshot(core, now)`** writes the keys that are live at `now`.

**`decode_snapshot(bytes)`** builds a core, with nothing to log. Refuse, with `BadSnapshot`, anything that is not exactly this format:

- a wrong magic or checksum, or bytes missing or left over;
- an unknown type or flag, a count of 0, a NaN score, or the bits of `-0.0`;
- a key, member or field that appears twice;
- keys, set members, hash fields or sorted-set items out of the order the format gives. So a core has exactly one snapshot.

**No input may panic.** Do not reserve memory for what a count merely claims. The checksum is no proof: the tests send bad contents with a correct checksum.

**`rewrite_commands(core, now)`** returns, in log form, commands that build the live keys. No command may have more than 130 arguments, and 5 keys of 1000 elements must take at most 100 commands. Keys with a deadline end up with it.

**`save_snapshot(disk, core, now)`** stores the snapshot as the file named `SNAPSHOT`. At every moment, a power cut leaves the old snapshot or the new one, whole. If it returns `Ok`, the new one is kept. You may use the temporary name `dump.snap.tmp`, which may be left over from an earlier crash.

**The rewrite** has two halves, so that the compact copy can be written elsewhere while the server keeps working. The caller has written all of the core's records before it starts.

- `begin_rewrite(core, now)` returns the compact commands for the keys now. If a rewrite is running: `AofError::Busy`. If the log is broken: `AofError::Broken`.
- Writes made meanwhile go to the old log **as usual**.
- `finish_rewrite(base)` replaces the log with `base` followed by everything written since `begin_rewrite`, **each record once**. With none running: `NotRewriting`. If the log broke meanwhile: `Broken`.
- However `finish_rewrite` ends, that rewrite is over. If a disk call fails, return `Io`; the log still holds everything, **keeps taking writes**, and another rewrite can be tried.
- At every moment, a power cut leaves a log that rebuilds the same keys. You may use the temporary name `appendonly.aof.tmp`, which may be left over from an earlier crash.
- `cancel_rewrite()` ends a running rewrite and does nothing otherwise.

The disk's promises are those of the `Disk` trait: appended bytes survive a power cut only after `sync`, while a `rename` is atomic and survives.

**`recover(disk, policy)`** starts the server's data.

- If the log exists and is not empty, open it as `Aof::open` does. The snapshot is not even read.
- Otherwise, if a snapshot exists, load it. A bad one is `AofError::BadSnapshot`. Before returning, write the log from it, so that later restarts keep both the snapshot's keys and new writes. A crash during this must not lose keys.
- Otherwise start empty.
'''

PROBLEM = dict(
    slug="snapshots-and-rewrite", title="Capstone 9: snapshots and log rewrite", level="hard", stage="keep-and-serve",
    tags=["snapshot", "binary format", "CRC-32", "atomic rename", "log compaction"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(9)), starter=starter(9), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Publish a file by writing it aside, syncing it and renaming it: readers see old or new, never half.",
        "A rewrite is a snapshot plus the tail written since, and each write must be in exactly one of them.",
        "A checksum finds accidents; only bounds checks and validation make a decoder safe.",
    ],
    follow_up="1) `begin_rewrite` holds the core still while it serialises. Redis forks instead. What would a copy-on-write structure give you here? 2) The tail is kept in memory and in the old log. When could it grow too large? 3) Why must the directory itself be synced after a rename on a real file system?",
    hints=[
        ("approach", "One helper publishes a file: remove the temporary one, append, sync, rename. The rewrite remembers the bytes of every write since it began, and the snapshot helper covers the seed too."),
        ("rust", "Give the decoder a reader type whose only way to get bytes is a bounds-checked `take`. `Option<Vec<u8>>` for the tail says whether a rewrite runs. `take()` it at the start of finish, so every exit ends the rewrite."),
        ("edge", "`append` adds to whatever the temporary file already holds. A non-empty but partial log wins over the snapshot at the next start, so the seed must appear all at once. Remember to sync before you rename."),
    ],
    notes=("The rewrite takes the compact form at the start and collects what is written afterwards; finishing publishes the two together with one rename, so every write is in exactly one place. Recovery seeds the log the same way, which keeps a crash during the first start harmless.",
           "O(data) per snapshot or rewrite", "O(data + tail)"),
    wrong={
        "tail_dropped": broken(SOLUTION, ("        base.extend(tail);\n", "        let _ = tail;\n")),
        "no_sync_before_rename": broken(SOLUTION, ("    disk.sync(temp)?;\n    disk.rename(temp, name)", "    disk.rename(temp, name)")),
        "stale_temporary_file_reused": broken(SOLUTION, ("    disk.remove(temp)?;\n    disk.append(temp, bytes)?;", "    disk.append(temp, bytes)?;")),
        "seed_written_in_place": broken(SOLUTION, (
            "            publish_file(&mut disk, \"appendonly.aof.tmp\", AOF, &seed)?;", "            disk.append(AOF, &seed)?;\n            disk.sync(AOF)?;")),
        "snapshot_keeps_expired_keys": broken(SOLUTION, (
            "        (!deadline.is_some_and(|at| at <= now)).then_some((key, value, deadline))", "        let _ = now;\n        Some((key, value, deadline))")),
        "decoder_trusts_the_checksum": broken(SOLUTION, (
            "                    if score.is_nan() || previous.as_ref()", "                    if false && previous.as_ref()"),
            ("    if !reader.0.is_empty() {\n        return Err(BadSnapshot);\n    }\n", "")),
        "negative_zero_turned_into_zero": broken(SOLUTION, (
            "                    if bits == (-0.0f64).to_bits() {\n                        return Err(BadSnapshot);\n                    }\n                    let score = f64::from_bits(bits);\n",
            "                    let score = f64::from_bits(bits) + 0.0;\n")),
        "keys_in_any_order": broken(SOLUTION, (
            "        if core.engine.map.keys().next_back().is_some_and(|last| *last >= key) {\n", "        if core.engine.map.contains_key(&key) {\n")),
        "members_in_any_order": broken(SOLUTION, (
            "                    if set.last().is_some_and(|last| *last >= member) {\n", "                    if set.contains(&member) {\n")),
        "one_command_per_collection": broken(SOLUTION, (
            "            for chunk in items.chunks(64 * width) {", "            for chunk in items.chunks(usize::MAX / width) {")),
        "empty_hash_or_set_accepted": broken(SOLUTION, (
            "\n        if count == 0 {\n", "\n        // only the kinds that keep an order are checked for being empty\n        if count == 0 && (kind == 1 || kind == 4) {\n")),
    },
)
