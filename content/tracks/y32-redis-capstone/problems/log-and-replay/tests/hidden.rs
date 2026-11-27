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

#[allow(dead_code)]
struct Seeded(Rng);
impl Dice for Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0.below(n)
    }
}

#[test]
fn records_of_every_kind_of_write() {
    let mut core = Core::new();
    let a = core.connect();
    let cases: &[(u64, &str, &[&str])] = &[
        (0, "MSET a 1 b 2", &["MSET a 1 b 2"]),
        (0, "append a x", &["append a x"]),
        (0, "SET c v NX", &["SET c v NX"]),
        (0, "SET c w NX", &[]),
        (0, "SET c w XX", &["SET c w XX"]),
        (0, "SET c w XX PX 5", &["SET c w PXAT 5"]),
        (0, "SET c w KEEPTTL", &["SET c w KEEPTTL"]),
        (0, "SET c w PXAT 90", &["SET c w PXAT 90"]),
        (10, "set d w ex 1 xx", &[]),
        (10, "set d w ex 1 nx", &["SET d w PXAT 1010"]),
        (10, "EXPIRE d 2", &["PEXPIREAT d 2010"]),
        (10, "PEXPIREAT d 5000", &["PEXPIREAT d 5000"]),
        (10, "PEXPIREAT d 10", &["DEL d"]),
        (10, "EXPIRE d 10", &[]),
        (10, "PERSIST c", &["PERSIST c"]),
        (10, "PERSIST c", &[]),
        (10, "RPUSH l x y z", &["RPUSH l x y z"]),
        (10, "LPOP l", &["LPOP l"]),
        (10, "RPOP l 5", &["RPOP l 5"]),
        (10, "RPOP l", &[]),
        (10, "SADD s m", &["SADD s m"]),
        (10, "SADD s m", &[]),
        (10, "SREM s nothing", &[]),
        (10, "HSET h f 1", &["HSET h f 1"]),
        (10, "HINCRBY h f 2", &["HINCRBY h f 2"]),
        (10, "HDEL h nothing", &[]),
        (10, "ZADD z 1 m", &["ZADD z 1 m"]),
        (10, "ZADD z 1 m", &[]),
        (10, "ZINCRBY z 2 m", &["ZINCRBY z 2 m"]),
        (10, "ZREM z m", &["ZREM z m"]),
        (10, "RENAME a e", &["RENAME a e"]),
        (10, "RENAME nothing e", &[]),
        (10, "DEL b nothing", &["DEL b nothing"]),
        (10, "FLUSHALL", &["FLUSHALL"]),
        (10, "FLUSHALL", &[]),
        (10, "PING", &[]),
        (10, "PUBLISH c m", &[]),
        (10, "SUBSCRIBE c", &[]),
    ];
    for &(now, line, want) in cases {
        check!(format!("at {now}: {line}"), logged(&mut core, a, now, line), want.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    }
}

#[test]
fn take_log_empties_the_list() {
    let mut core = Core::new();
    let a = core.connect();
    core.request(a, args("SET a 1"), 0);
    core.request(a, args("SET b 2"), 0);
    check!("both, in order", lines(&core.take_log()), vec!["SET a 1".to_string(), "SET b 2".to_string()]);
    check!("then nothing", core.take_log().len(), 0);
}

#[test]
fn every_way_of_finding_a_key_expired_logs_its_removal_once() {
    for line in ["GET k", "EXISTS k", "TYPE k", "TTL k", "WATCH k"] {
        let mut core = Core::new();
        let a = core.connect();
        logged(&mut core, a, 0, "SET k v PX 10");
        check!(line, logged(&mut core, a, 10, line), vec!["DEL k".to_string()]);
        check!(format!("{line} again"), logged(&mut core, a, 10, line), Vec::<String>::new());
    }
    let mut core = Core::new();
    let a = core.connect();
    logged(&mut core, a, 0, "SET k v PX 10");
    check!("cycle", core.cycle(10, &mut Seeded(Rng::new(4))), 1);
    check!("cycle's record", lines(&core.take_log()), vec!["DEL k".to_string()]);
    logged(&mut core, a, 0, "SET j v PX 10");
    check!("a refused command can still remove", logged(&mut core, a, 10, "RENAME j k"), vec!["DEL j".to_string()]);
}

#[test]
fn blocking_pops_are_logged_as_plain_pops() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    logged(&mut core, a, 0, "RPUSH q 1");
    check!("a pop that answers at once", logged(&mut core, a, 0, "BLPOP none q 0"), vec!["LPOP q".to_string()]);
    check!("blocking logs nothing", logged(&mut core, a, 0, "BRPOP q 0"), Vec::<String>::new());
    check!("the push, then the pop it served", logged(&mut core, b, 0, "RPUSH q 2 3"), vec!["RPUSH q 2 3".to_string(), "RPOP q".to_string()]);
    check!("a timeout logs nothing", { core.request(a, args("BLPOP x 1"), 0); core.tick(5000); core.take_log().len() }, 0);
}

#[test]
fn how_a_transaction_is_wrapped() {
    let transaction = |commands: &[&str]| {
        let mut core = Core::new();
        let a = core.connect();
        core.request(a, args("SET text v"), 0);
        core.take_log();
        core.request(a, args("MULTI"), 0);
        for c in commands {
            core.request(a, args(c), 0);
        }
        logged(&mut core, a, 0, "EXEC")
    };
    check!("nothing to log", transaction(&["GET text", "PING"]), Vec::<String>::new());
    check!("one record needs no wrapping", transaction(&["GET text", "SET a 1", "INCR text"]), vec!["SET a 1".to_string()]);
    let two = ["MULTI", "SET a 1", "LPUSH l x", "EXEC"].map(String::from).to_vec();
    check!("two records, a failed command between them", transaction(&["SET a 1", "INCR text", "LPUSH l x"]), two);
    let popped = ["MULTI", "RPUSH l x", "LPOP l", "EXEC"].map(String::from).to_vec();
    check!("a blocking pop inside", transaction(&["RPUSH l x", "BLPOP l 0"]), popped);
    check!("two records from one command", {
        let mut core = Core::new();
        let a = core.connect();
        core.request(a, args("SET n 1 PX 5"), 0);
        core.take_log();
        core.request(a, args("MULTI"), 0);
        core.request(a, args("INCR n"), 0);
        logged(&mut core, a, 9, "EXEC")
    }, ["MULTI", "DEL n", "INCR n", "EXEC"].map(String::from).to_vec());
}

#[test]
fn a_served_pop_comes_after_the_whole_transaction() {
    let mut core = Core::new();
    let [a, b] = clients(&mut core);
    core.request(a, args("BLPOP q 0"), 0);
    core.request(b, args("MULTI"), 0);
    core.request(b, args("RPUSH q 1"), 0);
    core.request(b, args("SET x 1"), 0);
    let want = ["MULTI", "RPUSH q 1", "SET x 1", "EXEC", "LPOP q"].map(String::from).to_vec();
    check!("EXEC", logged(&mut core, b, 0, "EXEC"), want);
}

#[test]
fn writing_makes_one_append_and_syncs_as_the_policy_says() {
    for (policy, syncs) in [(SyncPolicy::Always, 2), (SyncPolicy::Never, 0)] {
        let disk = MemDisk::default();
        let (mut log, _) = Aof::open(disk.clone(), policy).expect("open an empty disk");
        let before = disk.calls().len();
        log.write(&[args("SET a 1"), args("SET b 2")]).expect("write");
        log.write(&[]).expect("an empty write");
        log.write(&[args("DEL a")]).expect("write");
        check!(format!("{policy:?}: the file"), disk.file(AOF), Some(log_of(&["SET a 1", "SET b 2", "DEL a"])));
        let calls = disk.calls()[before..].to_vec();
        check!(format!("{policy:?}: appends in {calls:?}"), calls.iter().filter(|c| c.starts_with("append")).count(), 2);
        check!(format!("{policy:?}: syncs in {calls:?}"), calls.iter().filter(|c| c.starts_with("sync")).count(), syncs);
        check!(format!("{policy:?}: each sync follows its append"), calls.first().map(|c| c.starts_with("append")), Some(true));
    }
}

#[test]
fn after_a_failed_write_the_log_takes_no_more() {
    for failing in 0..2 {
        let disk = MemDisk::default();
        let (mut log, _) = open(&disk);
        log.write(&[args("SET a 1")]).expect("write");
        disk.fail_in(failing, false);
        let result = log.write(&[args("SET b 2")]);
        check!(format!("call {failing} fails: {result:?}"), matches!(result, Err(AofError::Io(_))), true);
        disk.heal();
        let calls = disk.calls().len();
        let result = log.write(&[args("SET c 3")]);
        check!(format!("then {result:?}"), matches!(result, Err(AofError::Broken)), true);
        check!("without touching the disk", disk.calls().len(), calls);
        check!("an empty write is refused too", matches!(log.write(&[]), Err(AofError::Broken)), true);
        // whatever reached the file opens again
        let (_, mut core) = open(&disk);
        check!("the first record survived", dump(&mut core, 0).iter().any(|k| k.starts_with("\"a\"")), true);
    }
}

#[test]
fn an_empty_or_missing_log_is_an_empty_server() {
    for disk in [MemDisk::default(), MemDisk::with(&[(AOF, b"")])] {
        let (_, mut core) = open(&disk);
        check!("no keys", dump(&mut core, 0), Vec::<String>::new());
        check!("nothing to log", core.take_log().len(), 0);
        check!("no truncation", disk.count("truncate"), 0);
    }
}

#[test]
fn replaying_logs_nothing_and_leaves_a_whole_log_alone() {
    let bytes = log_of(&["SET a 1 PXAT 50", "MULTI", "INCR a", "DEL a", "EXEC", "RPUSH l x", "LPOP l"]);
    let disk = MemDisk::with(&[(AOF, &bytes)]);
    let (_, mut core) = open(&disk);
    check!("nothing to log", core.take_log().len(), 0);
    check!("untouched", (disk.file(AOF), disk.calls().len()), (Some(bytes), 0));
    check!("no keys", dump(&mut core, 0), Vec::<String>::new());
}

#[test]
fn the_log_decides_what_expired_not_the_clock() {
    // k was alive when the later commands ran, whatever the time is now
    let bytes = log_of(&["RPUSH k a", "PEXPIREAT k 1000", "RPUSH k b", "SET n 5 PXAT 1000", "INCR n"]);
    let now_500 = vec![
        format!("{:?} list {:?} {:?}", "k", arr(&["a", "b"]), int(500)),
        format!("{:?} string {:?} {:?}", "n", bulk("6"), int(500)),
    ];
    check!("read at 500", reopened(&bytes, 500), now_500);
    check!("read at 2000", reopened(&bytes, 2000), Vec::<String>::new());
}

#[test]
fn every_cut_of_a_log_opens_to_whole_records_and_transactions() {
    let units: &[&[&str]] = &[
        &["SET a 1"],
        &["RPUSH l x y"],
        &["MULTI", "INCR a", "LPOP l", "EXEC"],
        &["SET b text PXAT 900"],
        &["MULTI", "DEL a", "SADD s m", "EXEC"],
        &["DEL b"],
    ];
    let mut whole = Vec::new();
    let mut ends = vec![0];
    for unit in units {
        whole.extend(log_of(unit));
        ends.push(whole.len());
    }
    for cut in 0..=whole.len() {
        let disk = MemDisk::with(&[(AOF, &whole[..cut])]);
        let (mut log, mut core) = open(&disk);
        let good = *ends.iter().filter(|&&e| e <= cut).max().unwrap();
        check!(format!("cut at {cut}: the file"), disk.file(AOF), Some(whole[..good].to_vec()));
        check!(format!("cut at {cut}: the keys"), dump(&mut core, 0), reopened(&whole[..good], 0));
        // and new records go on cleanly from there
        log.write(&[args("SET after cut")]).expect("write");
        let mut want = whole[..good].to_vec();
        want.extend(log_of(&["SET after cut"]));
        check!(format!("cut at {cut}: after another write"), reopened(&disk.file(AOF).unwrap(), 0), reopened(&want, 0));
    }
}

#[test]
fn damage_is_not_a_torn_end() {
    let good = log_of(&["SET a 1"]);
    let damaged: Vec<(&str, Vec<u8>)> = vec![
        ("bytes that start no command", b"garbage\r\n".to_vec()),
        ("an inline command", b"SET b 2\r\n".to_vec()),
        ("a zero byte", vec![0]),
        ("a number where a string belongs", b"*1\r\n:1\r\n".to_vec()),
        ("a wrong terminator", b"*1\r\n$1\r\naXX".to_vec()),
        ("an empty command", b"*0\r\n".to_vec()),
        ("EXEC with no MULTI", log_of(&["EXEC"])),
        ("MULTI inside MULTI", log_of(&["MULTI", "MULTI"])),
        ("an unknown command", log_of(&["NOPE x"])),
        ("a command that fails", log_of(&["INCR text", "SET text v", "LPUSH text x"])),
        ("a command that fails in a transaction", log_of(&["SET t v", "MULTI", "LPUSH t x", "EXEC"])),
    ];
    for (what, bad) in damaged {
        // the damage is followed by a good record, so it cannot be a torn end
        let mut bytes = good.clone();
        bytes.extend(&bad);
        bytes.extend(&good);
        let disk = MemDisk::with(&[(AOF, &bytes)]);
        let result = Aof::open(disk.clone(), SyncPolicy::Never).map(|_| ());
        match result {
            Err(AofError::Corrupt(at)) => check!(format!("{what}: where"), at >= good.len() as u64 && at < (good.len() + bad.len()) as u64, true),
            other => panic!("{what}: expected Corrupt, got {other:?}"),
        }
        check!(format!("{what}: the file is left alone"), disk.file(AOF), Some(bytes));
    }
}

#[test]
fn the_corrupt_position_is_where_the_record_starts() {
    let good = log_of(&["SET a 1", "SET b 2"]);
    let mut bytes = good.clone();
    bytes.extend(b"?");
    let result = Aof::open(MemDisk::with(&[(AOF, &bytes)]), SyncPolicy::Never).map(|_| ());
    match result {
        Err(AofError::Corrupt(at)) => check!("offset", at, good.len() as u64),
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

#[test]
fn a_disk_that_cannot_be_read_is_an_error() {
    let disk = MemDisk::with(&[(AOF, b"*1\r\n")]);
    disk.fail_in(0, true);
    let result = Aof::open(disk, SyncPolicy::Always).map(|_| ());
    check!(format!("{result:?}"), matches!(result, Err(AofError::Io(_))), true);
}

#[test]
fn values_larger_than_a_clients_limit_replay() {
    let mut core = Core::new();
    let a = core.connect();
    let big = vec![7u8; 3 << 20];
    let mut push: Vec<Vec<u8>> = vec![b"RPUSH".to_vec(), b"l".to_vec()];
    push.extend((0..70_000).map(|i| i.to_string().into_bytes()));
    core.request(a, vec![b"SET".to_vec(), b"big".to_vec(), big], 0);
    core.request(a, push, 0);
    let disk = MemDisk::default();
    let (mut log, _) = open(&disk);
    log.write(&core.take_log()).expect("write");
    let (_, mut again) = open(&disk);
    check!("both came back", dump(&mut again, 0) == dump(&mut core, 0), true);
}

#[test]
fn random_histories_replay_to_the_same_keys() {
    let mut rng = Rng::new(0x320b);
    for run_number in 0..250 {
        let disk = MemDisk::default();
        let (mut log, mut core) = open(&disk);
        let ids: [ClientId; 3] = clients(&mut core);
        let mut now = 1;
        let mut history = Vec::new();
        for _ in 0..80 {
            now += rng.below(8) as u64;
            let id = *rng.pick(&ids);
            if rng.below(20) == 0 {
                core.cycle(now, &mut Seeded(Rng::new(rng.next_u64())));
                history.push(format!("{now}: cycle"));
            } else if rng.below(12) == 0 {
                let key = *rng.pick(&["a", "b", "c"]);
                let line = format!("BLPOP {key} 1");
                history.push(format!("{now}: {id}: {line}"));
                core.request(id, args(&line), now);
            } else if rng.below(15) == 0 {
                core.tick(now);
                history.push(format!("{now}: tick"));
            } else {
                let line = random_command(&mut rng, now);
                history.push(format!("{now}: {id}: {line}"));
                core.request(id, args(&line), now);
            }
            log.write(&core.take_log()).expect("write");
        }
        for later in [now, now + 20, now + 1000] {
            let (_, mut again) = open(&disk);
            // compare on copies, so that looking does not change either side
            let (_, mut also) = open(&disk);
            check!(format!("run {run_number}: two restarts agree at {later}, after {history:?}"), dump(&mut again, later), dump(&mut also, later));
            if later == now + 1000 {
                check!(format!("run {run_number}: a restart at {later} matches the running server, after {history:?}"), dump(&mut also, later), dump(&mut core, later));
            }
        }
        let (_, mut again) = open(&disk);
        check!(format!("run {run_number}: a restart at {now} matches, after {history:?}"), dump(&mut again, now), dump(&mut core, now));
    }
}
