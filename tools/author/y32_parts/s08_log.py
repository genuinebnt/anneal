from y32_parts.common import *

SOLUTION = solution(8)

HELPERS = CORE + OUTPUTS + MEMDISK + DUMP + RANDOM_COMMANDS + r'''
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
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn only_changes_are_logged_as_they_were_sent() {
    let mut core = Core::new();
    let a = core.connect();
    check!("a write, as sent", logged(&mut core, a, 0, "set k v"), vec!["set k v".to_string()]);
    check!("a read", logged(&mut core, a, 0, "GET k"), Vec::<String>::new());
    check!("a write that changes nothing", logged(&mut core, a, 0, "DEL nothing"), Vec::<String>::new());
    check!("a refused write", logged(&mut core, a, 0, "INCR k"), Vec::<String>::new());
}

#[test]
fn times_are_logged_as_fixed_points() {
    let mut core = Core::new();
    let a = core.connect();
    check!("SET EX", logged(&mut core, a, 5000, "SET k v EX 2"), vec!["SET k v PXAT 7000".to_string()]);
    check!("PEXPIRE", logged(&mut core, a, 5000, "pexpire k 10"), vec!["PEXPIREAT k 5010".to_string()]);
    check!("EXPIRE in the past", logged(&mut core, a, 5000, "EXPIRE k -1"), vec!["DEL k".to_string()]);
}

#[test]
fn expiry_is_logged_as_a_removal_before_what_follows() {
    let mut core = Core::new();
    let a = core.connect();
    logged(&mut core, a, 0, "SET n 5 PX 10");
    check!("INCR after it expired", logged(&mut core, a, 20, "INCR n"), vec!["DEL n".to_string(), "INCR n".to_string()]);
}

#[test]
fn a_transaction_is_wrapped() {
    let mut core = Core::new();
    let a = core.connect();
    for line in ["MULTI", "SET a 1", "GET a", "INCR a"] {
        check!(line, logged(&mut core, a, 0, line), Vec::<String>::new());
    }
    check!("EXEC", logged(&mut core, a, 0, "EXEC"), vec!["MULTI".to_string(), "SET a 1".to_string(), "INCR a".to_string(), "EXEC".to_string()]);
}

#[test]
fn a_restart_brings_everything_back() {
    let disk = MemDisk::default();
    let (mut log, mut core) = open(&disk);
    let a = core.connect();
    for line in ["SET s hello", "RPUSH l a b", "HSET h f v", "SADD t x", "ZADD z 1.5 m", "PEXPIRE s 5000"] {
        core.request(a, args(line), 100);
        log.write(&core.take_log()).expect("write");
    }
    let (_, mut again) = open(&disk);
    check!("the same keys", dump(&mut again, 200), dump(&mut core, 200));
}

#[test]
fn a_torn_end_is_cut_off() {
    let whole = log_of(&["SET a 1", "SET b 2"]);
    let first = log_of(&["SET a 1"]).len();
    let disk = MemDisk::with(&[(AOF, &whole[..whole.len() - 3])]);
    let (_, mut core) = open(&disk);
    check!("the first record counts", dump(&mut core, 0).len(), 1);
    check!("and the rest is cut off", disk.file(AOF), Some(whole[..first].to_vec()));
}
''',
]

HIDDEN = [
    HELPERS,
    DICE,
    r'''
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
''',
]

STATEMENT = r'''
**Capstone, step 8 of 10.** A restart must not lose data. The server keeps an **append-only file**: a log of what changed, from which it rebuilds itself. The starter holds the reference code of steps 1 to 7, the `Disk` trait with a real directory (`DirDisk`), and the `AofError` type.

@SPOILERS@

**What goes in the log.** `Core::take_log()` returns the records for everything since the last call, in order. A record is one command, as a list of arguments. Replaying the records must rebuild the keys **without knowing when they were first run**.

- A command that changes a key is recorded **as the client sent it**, letter case included, except as below. A command that changes nothing is not recorded.
- `SET` with `EX`, `PX` or `PXAT` that sets: `SET key value PXAT t`, with the deadline `t` as a number. Any `NX` or `XX` is dropped.
- `EXPIRE`, `PEXPIRE`, `PEXPIREAT`: `PEXPIREAT key t`, or `DEL key` if the key was removed at once.
- A key removed because its deadline passed, by any command or by `cycle`: `DEL key`, **before** the record of the command that found it, if that makes one.
- A blocking pop, answered at once or later: `LPOP key` or `RPOP key`, after the record of whatever enabled it.
- An `EXEC` that makes **two or more** records puts `MULTI` before them and `EXEC` after them. Pops served after it come after that `EXEC`.

These rewritten records use upper-case names. `FLUSHALL` on no keys changes nothing. Records for `LTRIM` calls that cut nothing are optional.

**The file** is named by `AOF`. It holds each record in the form of `encode_command`, one after the other.

**`Aof::write(records)`** adds them with **one** call to `append`. With `SyncPolicy::Always` it then calls `sync`. With no records it calls nothing.

In the `Disk` promise, a synced file keeps its bytes **and its name** through a power cut. So the given `DirDisk::sync` syncs the file and then its directory, because `append` may have just created it. The tests cannot prove that part of `DirDisk`: they cut the power only of a disk in memory, where a new name is always kept. Nothing here has been tried against a real power cut.

If a call fails, return `AofError::Io`. From then on every `write` returns `AofError::Broken` and touches nothing, empty ones too.

**`Aof::open(disk, policy)`** reads the file, builds a new `Core` from it and returns both. A missing file is an empty one. The core has nothing to log.

- Nothing expires during replay: the log says itself what was removed. Deadlines apply again afterwards.
- A **torn end** is a last record that is not all there, as `parse_command` says it. A last transaction without its `EXEC` is torn too. Cut off the torn end with `truncate`, so that new records follow cleanly. Call `truncate` only if there is something to cut, and make no other call that changes the disk.
- Records of the log may be larger than what clients may send. Allow values and collections far beyond `Limits::default()`.
- Anything else that is wrong is `AofError::Corrupt(offset)`, and the file is not changed. This covers:
    - a record that does not start with `*`, or that `parse_command` refuses;
    - an empty command, `MULTI` inside a transaction, or `EXEC` outside one;
    - a command that replies with an error.
- `offset` is where the bad record starts. For a failing command inside a transaction, any offset within the transaction will do.
- A failed disk call is `AofError::Io`.
'''

PROBLEM = dict(
    slug="log-and-replay", title="Capstone 8: log every change and replay it", level="hard", stage="keep-and-serve",
    tags=["append-only file", "effect logging", "crash recovery", "torn writes", "atomic transactions"],
    statement=STATEMENT.replace("@SPOILERS@", spoilers(8)), starter=starter(8), solution=SOLUTION, visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "Log effects, not requests: a relative time or a blocking pop cannot be replayed as it was asked.",
        "Whatever the server decided on its own, such as an expiry, must be written down in order.",
        "A torn end is expected after a crash; damage in the middle is not, and must not be silently cut off.",
    ],
    follow_up="1) What does a failed `fsync` mean for data already written, and why does the log stop for good? 2) A flipped bit that still parses goes unnoticed. What would a checksum per record cost? 3) With `Never`, what exactly can a power cut lose?",
    hints=[
        ("approach", "Let each command say what to record when it differs from what was sent. Record expiry at the single place where it happens. Wrap `EXEC` by remembering how long the log was before it ran."),
        ("rust", "A flag that a command sets when it pushes its own record keeps the default simple. For replay, `parse_command` already tells you need-more from error, and `Vec::insert` can place `MULTI` after the fact."),
        ("edge", "A lazy removal must not count as the command having changed something, or a `GET` gets logged. What time should replay pretend it is, so that no deadline has passed?"),
    ],
    notes=("The engine records rewritten forms where a command depends on time, and a removal wherever expiry happens, so the log is a deterministic script. Replay runs it at time 0, where nothing has expired, and keeps only whole records and whole transactions.",
           "O(record) per write; O(log) to open", "O(data + unwritten records)"),
    wrong={
        "lazy_expiry_not_logged": broken(SOLUTION, (
            "        // the log says so itself, so that replaying it needs no clock\n        self.log.push(vec![b\"DEL\".to_vec(), key.clone()]);\n", "")),
        "relative_expire_logged_as_sent": broken(SOLUTION, (
            "        self.logged = true;\n        self.log.push(if past {\n            vec![b\"DEL\".to_vec(), key.to_vec()]\n        } else {\n            vec![\n                b\"PEXPIREAT\".to_vec(),\n                key.to_vec(),\n                at.to_string().into_bytes(),\n            ]\n        });\n",
            "        let _ = past;\n")),
        "replay_with_the_clock_running": broken(SOLUTION, (
            "            if matches!(engine.execute(&command, 0), Frame::Error(_)) {", "            if matches!(engine.execute(&command, u64::MAX / 2), Frame::Error(_)) {")),
        "torn_end_left_in_the_file": broken(SOLUTION, (
            "        if valid < bytes.len() {\n            disk.truncate(AOF, valid as u64)?;\n        }\n", "")),
        "half_a_transaction_kept": broken(SOLUTION, (
            "    Ok(open.map_or(at, |(start, _)| start))", "    if let Some((_, held)) = open {\n        for command in held {\n            engine.execute(&command, 0);\n        }\n        engine.log.clear();\n    }\n    Ok(at)")),
        "damage_cut_off_silently": broken(SOLUTION, (
            "        let Ok(parsed) = parse_command(&bytes[at..], &limits) else {\n            return Err(corrupt);\n        };", "        let Ok(parsed) = parse_command(&bytes[at..], &limits) else {\n            let _ = corrupt;\n            break;\n        };")),
        "keeps_writing_after_a_failure": broken(SOLUTION, ("        self.broken = result.is_err();\n", "")),
        "client_limits_on_replay": broken(SOLUTION, (
            "    let limits = Limits {\n        max_line: 64 * 1024,\n        max_bulk: 1 << 30,\n        max_array: 1 << 24,\n        max_depth: 1,\n    };", "    let limits = Limits::default();")),
    },
)
