use solution::*;

#[allow(unused_imports)]
use anneal_prelude::Rng;


#[allow(dead_code)]
fn args(line: &str) -> Vec<Vec<u8>> {
    line.split(' ').map(|w| w.as_bytes().to_vec()).collect()
}
#[allow(dead_code)]
fn bulk(s: &str) -> Frame {
    Frame::Bulk(s.as_bytes().to_vec())
}
#[allow(dead_code)]
fn int(n: i64) -> Frame {
    Frame::Int(n)
}
#[allow(dead_code)]
fn arr(items: &[&str]) -> Frame {
    Frame::Array(items.iter().map(|s| bulk(s)).collect())
}
#[allow(dead_code)]
fn ok() -> Frame {
    Frame::Simple("OK".to_string())
}
#[allow(dead_code)]
fn simple(s: &str) -> Frame {
    Frame::Simple(s.to_string())
}
#[allow(dead_code)]
fn err(s: &str) -> Frame {
    Frame::Error(s.to_string())
}
#[allow(dead_code)]
const WRONGTYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";
#[allow(dead_code)]
const NOT_INT: &str = "ERR value is not an integer or out of range";
/// Every error reply must start with `prefix`.
#[allow(dead_code)]
fn is_error(frame: &Frame, prefix: &str) -> bool {
    matches!(frame, Frame::Error(text) if text.starts_with(prefix))
}

/// Runs a command that must produce exactly one reply, to the caller, and returns it.
#[allow(dead_code)]
fn say_at(core: &mut Core, id: ClientId, now: u64, line: &str) -> Frame {
    let out = core.request(id, args(line), now);
    assert!(
        out.len() == 1 && out[0].to == id && !out[0].published && !out[0].close,
        "{line:?} should produce one ordinary reply to client {id}, but produced {out:?}"
    );
    out[0].frame.clone()
}
#[allow(dead_code)]
fn say(core: &mut Core, id: ClientId, line: &str) -> Frame {
    say_at(core, id, 0, line)
}
#[allow(dead_code)]
fn session(core: &mut Core, id: ClientId, steps: &[(&str, Frame)]) {
    for (i, (line, want)) in steps.iter().enumerate() {
        check!(format!("client {id}, command {}: {line}", i + 1), say(core, id, line), want.clone());
    }
}
#[allow(dead_code)]
fn queued() -> Frame {
    simple("QUEUED")
}

/// The frames meant for one client, in order.
#[allow(dead_code)]
fn to(out: &[Delivery], id: ClientId) -> Vec<Frame> {
    out.iter().filter(|d| d.to == id).map(|d| d.frame.clone()).collect()
}
#[allow(dead_code)]
fn send(core: &mut Core, id: ClientId, line: &str) -> Vec<Delivery> {
    core.request(id, args(line), 0)
}
#[allow(dead_code)]
fn clients<const N: usize>(core: &mut Core) -> [ClientId; N] {
    std::array::from_fn(|_| core.connect())
}

use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
#[derive(Default, Clone)]
struct MemFile {
    data: Vec<u8>,
    /// How much of it would survive a power cut.
    synced: usize,
}

#[allow(dead_code)]
#[derive(Default)]
struct DiskState {
    files: BTreeMap<String, MemFile>,
    /// Every call that changes or syncs something, as "op name".
    calls: Vec<String>,
    /// The call with this number (from 0) fails. A failing append keeps half of its bytes.
    fail_at: Option<usize>,
    /// After the failing call every later call fails too, as when the power is gone.
    power_off: bool,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
struct MemDisk(Arc<Mutex<DiskState>>);

#[allow(dead_code)]
impl MemDisk {
    fn with(files: &[(&str, &[u8])]) -> MemDisk {
        let disk = MemDisk::default();
        for (name, data) in files {
            let file = MemFile { data: data.to_vec(), synced: data.len() };
            disk.0.lock().unwrap().files.insert(name.to_string(), file);
        }
        disk
    }
    fn file(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().files.get(name).map(|f| f.data.clone())
    }
    fn names(&self) -> Vec<String> {
        self.0.lock().unwrap().files.keys().cloned().collect()
    }
    fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().calls.clone()
    }
    fn count(&self, op: &str) -> usize {
        self.calls().iter().filter(|c| c.starts_with(op)).count()
    }
    /// Fails the `n`th changing call from now on (0 is the next one).
    fn fail_in(&self, n: usize, power_off: bool) {
        let mut state = self.0.lock().unwrap();
        state.fail_at = Some(state.calls.len() + n);
        state.power_off = power_off;
    }
    fn heal(&self) {
        let mut state = self.0.lock().unwrap();
        state.fail_at = None;
        state.power_off = false;
    }
    /// The disk after a power cut: each file keeps its synced bytes and up to `extra` more.
    fn after_power_cut(&self, extra: usize) -> MemDisk {
        let state = self.0.lock().unwrap();
        let disk = MemDisk::default();
        for (name, file) in &state.files {
            let len = file.data.len().min(file.synced.saturating_add(extra));
            let kept = MemFile { data: file.data[..len].to_vec(), synced: len };
            disk.0.lock().unwrap().files.insert(name.clone(), kept);
        }
        disk
    }
    /// Counts a changing call; `Err` if it is the one that fails.
    fn step(state: &mut DiskState, call: String) -> io::Result<()> {
        let n = state.calls.len();
        state.calls.push(call);
        match state.fail_at {
            Some(at) if n == at || (state.power_off && n > at) => Err(io::Error::from(io::ErrorKind::StorageFull)),
            _ => Ok(()),
        }
    }
    fn dead(state: &DiskState) -> bool {
        state.power_off && state.fail_at.is_some_and(|at| state.calls.len() > at)
    }
}

impl Disk for MemDisk {
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let state = self.0.lock().unwrap();
        if MemDisk::dead(&state) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        Ok(state.files.get(name).map(|f| f.data.clone()))
    }
    fn append(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        let first_failure = state.fail_at == Some(state.calls.len());
        let result = MemDisk::step(&mut state, format!("append {name}"));
        let keep = match &result {
            Ok(()) => bytes.len(),
            Err(_) if first_failure => bytes.len() / 2,
            Err(_) => return result,
        };
        state.files.entry(name.to_string()).or_default().data.extend_from_slice(&bytes[..keep]);
        result
    }
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        MemDisk::step(&mut state, format!("truncate {name}"))?;
        let file = state.files.get_mut(name).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        file.data.truncate(len as usize);
        file.synced = file.synced.min(file.data.len());
        Ok(())
    }
    fn sync(&mut self, name: &str) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        MemDisk::step(&mut state, format!("sync {name}"))?;
        let file = state.files.get_mut(name).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        file.synced = file.data.len();
        Ok(())
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        MemDisk::step(&mut state, format!("rename {from} {to}"))?;
        let file = state.files.remove(from).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        state.files.insert(to.to_string(), file);
        Ok(())
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        MemDisk::step(&mut state, format!("remove {name}"))?;
        state.files.remove(name);
        Ok(())
    }
}

/// Every live key at `now`, with its type, content and time left, read back through commands.
#[allow(dead_code)]
fn dump(core: &mut Core, now: u64) -> Vec<String> {
    let id = core.connect();
    let Frame::Array(keys) = say_at(core, id, now, "KEYS *") else { panic!("KEYS * must reply with an array") };
    let mut out = Vec::new();
    for key in keys {
        let Frame::Bulk(key) = key else { panic!("a key must be a bulk string") };
        let mut ask = |words: &[&str]| {
            let mut command = vec![words[0].as_bytes().to_vec(), key.clone()];
            command.extend(words[1..].iter().map(|w| w.as_bytes().to_vec()));
            let reply = core.request(id, command, now);
            reply[0].frame.clone()
        };
        let Frame::Simple(kind) = ask(&["TYPE"]) else { panic!("TYPE must reply with a simple string") };
        let value = match kind.as_str() {
            "string" => ask(&["GET"]),
            "list" => ask(&["LRANGE", "0", "-1"]),
            "hash" => ask(&["HGETALL"]),
            "set" => ask(&["SMEMBERS"]),
            "zset" => ask(&["ZRANGE", "0", "-1", "WITHSCORES"]),
            other => panic!("unknown type {other}"),
        };
        let ttl = ask(&["PTTL"]);
        out.push(format!("{:?} {kind} {value:?} {ttl:?}", String::from_utf8_lossy(&key)));
    }
    core.disconnect(id);
    out
}

#[allow(dead_code)]
fn random_command(rng: &mut Rng, now: u64) -> String {
    let key = *rng.pick(&["a", "b", "c", "d", "e"]);
    let other = *rng.pick(&["a", "b", "c", "d", "e"]);
    let word = *rng.pick(&["x", "y", "z", "1", "2", "-3"]);
    let small = rng.int(-3, 3);
    let soon = now + rng.below(60) as u64;
    let ttl = rng.below(50) + 1;
    match rng.below(34) {
        0 => format!("SET {key} {word}"),
        1 => format!("SET {key} {word} PX {ttl}"),
        2 => format!("SET {key} {word} NX"),
        3 => format!("SET {key} {word} XX KEEPTTL"),
        4 => format!("APPEND {key} {word}"),
        5 => format!("INCRBY {key} {small}"),
        6 => format!("DEL {key} {other}"),
        7 => format!("RENAME {key} {other}"),
        8 => format!("LPUSH {key} {word}"),
        9 => format!("RPUSH {key} {word} {other}"),
        10 => format!("LPOP {key}"),
        11 => format!("RPOP {key} 2"),
        12 => format!("LTRIM {key} {small} -1"),
        13 => format!("HSET {key} {word} {other}"),
        14 => format!("HDEL {key} {word}"),
        15 => format!("HINCRBY {key} {word} {small}"),
        16 => format!("SADD {key} {word} {other}"),
        17 => format!("SREM {key} {word}"),
        18 => format!("ZADD {key} {small} {word}"),
        19 => format!("ZINCRBY {key} 1.5 {word}"),
        20 => format!("ZREM {key} {word}"),
        21 => format!("PEXPIRE {key} {ttl}"),
        22 => format!("PEXPIREAT {key} {soon}"),
        23 => format!("PERSIST {key}"),
        24 => format!("EXPIRE {key} {small}"),
        25 => format!("GET {key}"),
        26 => format!("EXISTS {key}"),
        27 => format!("MSET {key} {word} {other} {word}"),
        28 => "MULTI".to_string(),
        29 | 30 => "EXEC".to_string(),
        31 => format!("LLEN {key}"),
        32 => format!("TYPE {key}"),
        _ => format!("PTTL {key}"),
    }
}

/// Records written as lines.
#[allow(dead_code)]
fn lines(records: &[Vec<Vec<u8>>]) -> Vec<String> {
    records.iter().map(|r| r.iter().map(|a| String::from_utf8_lossy(a).into_owned()).collect::<Vec<_>>().join(" ")).collect()
}
/// The log's bytes for these commands.
#[allow(dead_code)]
fn log_of(commands: &[&str]) -> Vec<u8> {
    commands.iter().flat_map(|c| encode_command(&args(c))).collect()
}
/// Runs a command and returns what it put in the log.
#[allow(dead_code)]
fn logged(core: &mut Core, id: ClientId, now: u64, line: &str) -> Vec<String> {
    core.request(id, args(line), now);
    lines(&core.take_log())
}
#[allow(dead_code)]
fn open(disk: &MemDisk) -> (Aof<MemDisk>, Core) {
    match Aof::open(disk.clone(), SyncPolicy::Always) {
        Ok(opened) => opened,
        Err(e) => panic!("the log did not open: {e:?}, log {:?}", disk.file(AOF).map(|b| String::from_utf8_lossy(&b).into_owned())),
    }
}
/// The dump of a core rebuilt from these bytes.
#[allow(dead_code)]
fn reopened(log: &[u8], now: u64) -> Vec<String> {
    let (_, mut core) = open(&MemDisk::with(&[(AOF, log)]));
    dump(&mut core, now)
}

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

#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}

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
        ("a score that is NaN", build(&[&[4, 0], &k, &one, &nan, &v, &[0xFF]])),
        ("a key twice", build(&[&string_k, &string_k, &[0xFF]])),
        ("a set member twice", build(&[&[3, 0], &k, &2u32.to_le_bytes(), &v, &v, &[0xFF]])),
        ("a hash field twice", build(&[&[2, 0], &k, &2u32.to_le_bytes(), &v, &v, &v, &v, &[0xFF]])),
        ("a sorted member twice", build(&[&[4, 0], &k, &2u32.to_le_bytes(), &score, &v, &score, &v, &[0xFF]])),
        ("a wrong magic", sealed(b"RSNAP2\xff".to_vec())),
    ];
    for (what, bytes) in cases {
        check!(what, decode_snapshot(&bytes).is_err(), true);
    }
    let good = build(&[&string_k, &[0xFF]]);
    check!("the same pieces put right are accepted", decode_snapshot(&good).is_ok(), true);
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
