"""Test helpers shared by the capstone steps."""
from y17_parts.common import broken  # noqa: F401
from y32_parts.code import solution, starter  # noqa: F401

USE = r'''use solution::*;

#[allow(unused_imports)]
use anneal_prelude::Rng;
'''

# frames and command lines
FRAMES = r'''
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
'''

# for the engine alone (steps 2 to 4)
ENGINE = FRAMES + r'''
#[allow(dead_code)]
fn at(engine: &mut Engine, now: u64, line: &str) -> Frame {
    engine.execute(&args(line), now)
}
#[allow(dead_code)]
fn run(engine: &mut Engine, line: &str) -> Frame {
    at(engine, 0, line)
}
/// Runs each line and checks its reply.
#[allow(dead_code)]
fn script(engine: &mut Engine, steps: &[(&str, Frame)]) {
    for (i, (line, want)) in steps.iter().enumerate() {
        check!(format!("command {} of the script: {line}", i + 1), run(engine, line), want.clone());
    }
}
'''

DICE = r'''
#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}
'''

# for the core (steps 5 on)
CORE = FRAMES + r'''
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
'''

OUTPUTS = r'''
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
'''

# a disk in memory that can fail and lose power
MEMDISK = r'''
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
'''

# everything a client can see of a keyspace at one moment
DUMP = r'''
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
'''

# random commands over a few keys, valid and not
RANDOM_COMMANDS = r'''
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
'''

# What a step's starter spoils, by the step whose starter is the first to hold it.
SPOILED = [
    (2, "*A value inside a value, arriving in pieces*", "step 1 shows how to parse nested input in pieces"),
    (2, "*Requests that arrive in pieces*", "step 1 shows the same need-more outcomes"),
    (3, "*Throw out what has gone stale*", "`Deadlines` solves it"),
    (4, "*Who is in 40th place?*", "`Ranked` solves its core, with `f64` scores"),
    (4, "*Which names fit the pattern?*", "`glob_matches` solves it"),
    (6, "*Hold requests until something arrives*", "`Waiting` solves most of it"),
    (9, "*Come back the same after a restart*", "step 8's records use its method"),
    (9, "*A key-value store with a write-ahead log that survives torn writes*", "step 8 shows how a torn end is cut off"),
    (9, "*Capstone 1: records and frames* (key-value store series)", "the given `crc32` is its checksum, and later starters show how to decode with bounds checks"),
    (10, "*Shrink a log while it is still being written*", "step 9's rewrite uses its method"),
    (10, "*Save a file that a crash cannot tear*", "step 9 publishes files by its method"),
]


def spoilers(step):
    """The warning at the top of a step's statement: every problem the starter gives work away from, by name."""
    steps = "step 1" if step == 2 else f"steps 1 to {step - 1}"
    lines = [f"**Spoiler warning.** The starter gives away {steps} of this capstone. It also gives away work from these problems. Solve them first if you have not.", ""]
    lines += [f"- {name}: {why}" for first, name, why in SPOILED if first <= step]
    return "\n".join(lines)
