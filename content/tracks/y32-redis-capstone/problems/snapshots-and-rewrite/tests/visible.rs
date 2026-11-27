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
