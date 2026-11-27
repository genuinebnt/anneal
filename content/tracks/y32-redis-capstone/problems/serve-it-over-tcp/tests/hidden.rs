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

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Only a broken server gets near it.
#[allow(dead_code)]
const PATIENCE: Duration = Duration::from_secs(5);

/// Runs a test body on a runtime with several threads, and gives up if it hangs.
#[allow(dead_code)]
fn run<F: std::future::Future<Output = ()>>(body: F) {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(12), body).await.expect("the test hung");
    });
}

/// A clock the test moves by hand.
#[allow(dead_code)]
#[derive(Clone, Default)]
struct Hands(Arc<AtomicU64>);
#[allow(dead_code)]
impl Hands {
    fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
    fn clock(&self) -> Clock {
        let hands = self.0.clone();
        Arc::new(move || hands.load(Ordering::SeqCst))
    }
}

#[allow(dead_code)]
fn config(hands: &Hands) -> Config {
    Config { clock: hands.clock(), push_buffer: 16, limits: Limits::default() }
}

#[allow(dead_code)]
async fn serve<D: Disk + Send + 'static>(core: Core, log: Option<Aof<D>>, config: Config) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    start(listener, core, log, config).await.expect("start")
}

/// A server with no log.
#[allow(dead_code)]
async fn memory_server() -> (Server, Hands) {
    let hands = Hands::default();
    hands.set(1000);
    let server = serve::<MemDisk>(Core::new(), None, config(&hands)).await;
    (server, hands)
}

/// Waits until the condition holds.
#[allow(dead_code)]
async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    for _ in 0..1000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("never happened: {what}");
}

#[allow(dead_code)]
struct Client {
    stream: TcpStream,
    input: Vec<u8>,
}

#[allow(dead_code)]
impl Client {
    async fn connect(server: &Server) -> Client {
        let stream = tokio::time::timeout(PATIENCE, TcpStream::connect(server.addr())).await.expect("connect in time").expect("connect");
        stream.set_nodelay(true).unwrap();
        Client { stream, input: Vec::new() }
    }
    async fn write(&mut self, bytes: &[u8]) {
        tokio::time::timeout(PATIENCE, self.stream.write_all(bytes)).await.expect("write in time").expect("write");
    }
    async fn send(&mut self, line: &str) {
        self.write(&encode_command(&args(line))).await;
    }
    /// The next reply, or `None` when the server has closed the connection.
    async fn next(&mut self) -> Option<Frame> {
        let limits = Limits { max_bulk: 64 << 20, max_array: 1 << 24, max_depth: 8, max_line: 64 * 1024 };
        loop {
            if let Some((frame, used)) = parse_frame(&self.input, &limits).expect("the server must send valid frames") {
                self.input.drain(..used);
                return Some(frame);
            }
            self.input.reserve(4096);
            match tokio::time::timeout(PATIENCE, self.stream.read_buf(&mut self.input)).await.expect("a reply in time") {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }
    async fn reply(&mut self) -> Frame {
        self.next().await.expect("the server closed the connection")
    }
    async fn ask(&mut self, line: &str) -> Frame {
        self.send(line).await;
        self.reply().await
    }
    /// Reads, and throws away, until the server closes the connection.
    async fn drain_to_close(&mut self) {
        let mut sink = vec![0; 64 * 1024];
        let reading = async {
            while matches!(self.stream.read(&mut sink).await, Ok(n) if n > 0) {}
        };
        tokio::time::timeout(PATIENCE, reading).await.expect("the server should have closed the connection");
    }
}

#[test]
fn a_command_a_byte_at_a_time() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        let mut bytes = encode_command(&args("SET key value"));
        bytes.extend(encode_command(&args("GET key")));
        for byte in bytes {
            c.write(&[byte]).await;
            tokio::task::yield_now().await;
        }
        check!("SET", c.reply().await, ok());
        check!("GET", c.reply().await, bulk("value"));
        server.shutdown().await;
    });
}

#[test]
fn large_and_binary_values() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        let value: Vec<u8> = (0..1_000_000u32).map(|i| (i % 256) as u8).collect();
        c.write(&encode_command(&[b"SET".to_vec(), b"big".to_vec(), value.clone()])).await;
        check!("SET", c.reply().await, ok());
        let reply = c.ask("GET big").await;
        check!("GET gives the same megabyte", reply == Frame::Bulk(value), true);
        server.shutdown().await;
    });
}

#[test]
fn empty_commands_get_no_reply() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        c.write(b"\r\n*0\r\n*-1\r\n   \r\nPING\r\n").await;
        check!("only PING answers", c.reply().await, simple("PONG"));
        check!("and nothing else is queued", c.ask("ECHO x").await, bulk("x"));
        server.shutdown().await;
    });
}

#[test]
fn quit_replies_then_closes() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        let mut bytes = Vec::new();
        for line in ["SET a 1", "QUIT", "SET b 2"] {
            bytes.extend(encode_command(&args(line)));
        }
        c.write(&bytes).await;
        check!("SET", c.reply().await, ok());
        check!("QUIT", c.reply().await, ok());
        check!("closed", c.next().await, None);
        let mut other = Client::connect(&server).await;
        check!("nothing after QUIT ran", other.ask("EXISTS a b").await, int(1));
        server.shutdown().await;
    });
}

#[test]
fn the_limits_of_the_config_apply() {
    run(async {
        let hands = Hands::default();
        let small = Config { limits: Limits { max_bulk: 100, max_line: 100, ..Limits::default() }, ..config(&hands) };
        let server = serve::<MemDisk>(Core::new(), None, small).await;
        for attack in [b"*2\r\n$3\r\nGET\r\n$101\r\n".to_vec(), vec![b'a'; 200], b"*1000000\r\n".to_vec()] {
            let mut c = Client::connect(&server).await;
            c.write(&attack).await;
            let reply = c.reply().await;
            check!(format!("{reply:?}"), is_error(&reply, "ERR Protocol error"), true);
            check!("closed", c.next().await, None);
        }
        let mut c = Client::connect(&server).await;
        check!("others are served", c.ask("PING").await, simple("PONG"));
        server.shutdown().await;
    });
}

#[test]
fn the_error_arrives_even_when_more_input_is_on_the_way() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        let mut bytes = b"*1\r\n?\r\n".to_vec();
        bytes.extend(vec![b'x'; 60_000]);
        c.write(&bytes).await;
        // give a reset, if the server causes one, the time to overtake the reply
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut got = Vec::new();
        let end = tokio::time::timeout(PATIENCE, c.stream.read_to_end(&mut got)).await.expect("the end in time");
        check!(format!("a normal end of stream, not {end:?}"), end.is_ok(), true);
        check!(format!("{:?}", String::from_utf8_lossy(&got)), got.starts_with(b"-ERR Protocol error"), true);
        server.shutdown().await;
    });
}

#[test]
fn connected_counts_clients_and_forgets_those_who_leave() {
    run(async {
        let (server, _) = memory_server().await;
        let mut a = Client::connect(&server).await;
        let mut b = Client::connect(&server).await;
        a.ask("PING").await;
        b.ask("PING").await;
        check!("two", server.stats().connected, 2);
        drop(a);
        eventually("one left", || server.stats().connected == 1).await;
        b.ask("QUIT").await;
        eventually("none left", || server.stats().connected == 0).await;
        server.shutdown().await;
    });
}

#[test]
fn a_blocked_client_that_leaves_takes_nothing() {
    run(async {
        let (server, _) = memory_server().await;
        let mut gone = Client::connect(&server).await;
        gone.send("BLPOP q 0").await;
        eventually("blocked", || server.stats().blocked == 1).await;
        drop(gone);
        eventually("the server noticed", || server.stats().connected == 0 && server.stats().blocked == 0).await;
        let mut c = Client::connect(&server).await;
        c.ask("RPUSH q kept").await;
        check!("the element is still there", c.ask("LLEN q").await, int(1));
        server.shutdown().await;
    });
}

#[test]
fn commands_sent_just_before_leaving_still_run() {
    run(async {
        let (server, _) = memory_server().await;
        for i in 0..50 {
            let mut c = Client::connect(&server).await;
            c.send(&format!("SET key{i} v")).await;
            // close the sending side and leave without reading
            c.stream.shutdown().await.unwrap();
            c.drain_to_close().await;
        }
        let mut c = Client::connect(&server).await;
        let mut ran = c.ask("DBSIZE").await;
        for _ in 0..200 {
            if ran == int(50) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
            ran = c.ask("DBSIZE").await;
        }
        check!("all fifty ran", ran, int(50));
        server.shutdown().await;
    });
}

#[test]
fn many_clients_count_to_one_number() {
    run(async {
        let (server, _) = memory_server().await;
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..40 {
            let mut c = Client::connect(&server).await;
            tasks.spawn(async move {
                let mut got = Vec::new();
                for _ in 0..50 {
                    match c.ask("INCR n").await {
                        Frame::Int(v) => got.push(v),
                        other => panic!("{other:?}"),
                    }
                }
                got
            });
        }
        let mut all = Vec::new();
        while let Some(got) = tasks.join_next().await {
            all.extend(got.unwrap());
        }
        all.sort_unstable();
        check!("every number once", all, (1..=2000).collect::<Vec<i64>>());
        server.shutdown().await;
    });
}

#[test]
fn transactions_and_watches_across_connections() {
    run(async {
        let (server, _) = memory_server().await;
        let (mut a, mut b) = (Client::connect(&server).await, Client::connect(&server).await);
        a.ask("WATCH k").await;
        b.ask("SET k b").await;
        a.ask("MULTI").await;
        a.ask("SET k a").await;
        check!("EXEC", a.ask("EXEC").await, Frame::NullArray);
        check!("GET", a.ask("GET k").await, bulk("b"));
        server.shutdown().await;
    });
}

#[test]
fn published_messages_reach_other_connections() {
    run(async {
        let (server, _) = memory_server().await;
        let (mut reader, mut writer) = (Client::connect(&server).await, Client::connect(&server).await);
        check!("SUBSCRIBE", reader.ask("SUBSCRIBE c").await, Frame::Array(vec![bulk("subscribe"), bulk("c"), int(1)]));
        // many more than the push buffer, read as they come
        for i in 0..200 {
            check!("PUBLISH", writer.ask(&format!("PUBLISH c m{i}")).await, int(1));
            check!(format!("message {i}"), reader.reply().await, arr(&["message", "c", &format!("m{i}")]));
        }
        server.shutdown().await;
    });
}

#[test]
fn a_reader_that_stops_reading_is_dropped_and_others_go_on() {
    run(async {
        let (server, _) = memory_server().await;
        let mut stuck = Client::connect(&server).await;
        let mut healthy = Client::connect(&server).await;
        let mut writer = Client::connect(&server).await;
        stuck.ask("SUBSCRIBE c").await;
        healthy.ask("SUBSCRIBE c").await;
        let payload = "x".repeat(64 * 1024);
        let command = encode_command(&args(&format!("PUBLISH c {payload}")));
        let mut dropped_after = None;
        // far more than any socket buffer holds
        for i in 0..1000 {
            writer.write(&command).await;
            let listeners = writer.reply().await;
            let got = healthy.reply().await;
            check!(format!("message {i} reaches the healthy reader"), got == arr(&["message", "c", &payload]), true);
            if listeners == int(1) {
                dropped_after = Some(i);
                break;
            }
            check!(format!("PUBLISH {i}"), listeners, int(2));
        }
        check!("the stuck reader was dropped", dropped_after.is_some(), true);
        eventually("two clients left", || server.stats().connected == 2).await;
        check!("the writer still works", writer.ask("PING").await, simple("PONG"));
        // the stuck reader has still read nothing: only cancelling its write lets the server end
        server.shutdown().await;
        stuck.drain_to_close().await;
    });
}

#[test]
fn expired_keys_leave_even_if_nobody_asks() {
    run(async {
        let hands = Hands::default();
        let disk = MemDisk::default();
        let (log, core) = Aof::open(disk.clone(), SyncPolicy::Always).expect("open");
        let server = serve(core, Some(log), config(&hands)).await;
        let mut c = Client::connect(&server).await;
        c.ask("SET forgotten v PX 10").await;
        hands.set(100);
        // the timer's cycle removes it and logs that
        eventually("the removal is logged", || {
            disk.file(AOF).unwrap_or_default().windows(8).any(|w| w == b"$3\r\nDEL\r")
        })
        .await;
        server.shutdown().await;
    });
}

/// A disk whose append takes a while, as a real one may.
#[allow(dead_code)]
#[derive(Clone)]
struct SlowDisk(MemDisk);

impl Disk for SlowDisk {
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        self.0.read(name)
    }
    fn append(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        std::thread::sleep(Duration::from_millis(20));
        self.0.append(name, bytes)
    }
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        self.0.truncate(name, len)
    }
    fn sync(&mut self, name: &str) -> io::Result<()> {
        self.0.sync(name)
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        self.0.rename(from, to)
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        self.0.remove(name)
    }
}

#[test]
fn nothing_is_acknowledged_before_it_is_in_the_log() {
    run(async {
        let hands = Hands::default();
        let disk = MemDisk::default();
        let (log, core) = Aof::open(SlowDisk(disk.clone()), SyncPolicy::Always).expect("open");
        let server = serve(core, Some(log), config(&hands)).await;
        let (mut a, mut reader) = (Client::connect(&server).await, Client::connect(&server).await);
        reader.ask("SUBSCRIBE c").await;
        let logged = |needle: String| {
            let log = disk.after_power_cut(0).file(AOF).unwrap_or_default();
            log.windows(needle.len()).any(|w| w == needle.as_bytes())
        };
        for i in 0..10 {
            check!("SET", a.ask(&format!("SET key{i} v")).await, ok());
            check!(format!("key{i} is synced when its reply arrives"), logged(format!("key{i}\r\n")), true);
        }
        // a message sent by a transaction is not seen before the writes beside it are safe
        a.ask("MULTI").await;
        a.ask("PUBLISH c go").await;
        a.ask("SET published yes").await;
        a.send("EXEC").await;
        reader.reply().await;
        check!("the write beside the message", logged("published\r\n".to_string()), true);
        server.shutdown().await;
    });
}

#[test]
fn many_commands_at_once_share_log_writes() {
    run(async {
        let hands = Hands::default();
        let disk = MemDisk::default();
        let (log, core) = Aof::open(disk.clone(), SyncPolicy::Always).expect("open");
        let server = serve(core, Some(log), config(&hands)).await;
        let mut c = Client::connect(&server).await;
        let mut bytes = Vec::new();
        for i in 0..1000 {
            bytes.extend(encode_command(&args(&format!("SET key{i} v"))));
        }
        c.write(&bytes).await;
        for _ in 0..1000 {
            check!("SET", c.reply().await, ok());
        }
        let syncs = disk.count("sync");
        check!(format!("{syncs} syncs for 1000 commands that arrived together"), syncs < 200, true);
        server.shutdown().await;
    });
}

#[test]
fn a_log_that_fails_stops_the_server_without_an_ok() {
    run(async {
        let hands = Hands::default();
        let disk = MemDisk::default();
        let (log, core) = Aof::open(disk.clone(), SyncPolicy::Always).expect("open");
        let server = serve(core, Some(log), config(&hands)).await;
        let (mut a, mut idle) = (Client::connect(&server).await, Client::connect(&server).await);
        idle.ask("PING").await;
        check!("first", a.ask("SET a 1").await, ok());
        disk.fail_in(0, true);
        a.send("SET b 2").await;
        check!("no reply, only the end", a.next().await, None);
        check!("other connections are closed too", idle.next().await, None);
        server.shutdown().await;
    });
}

#[test]
fn what_was_acknowledged_survives_a_restart() {
    run(async {
        let dir = std::env::temp_dir().join(format!("anneal-redis-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for round in 0..3 {
            let hands = Hands::default();
            let (log, core) = recover(DirDisk::new(&dir), SyncPolicy::Always).expect("recover");
            let server = serve(core, Some(log), config(&hands)).await;
            let mut c = Client::connect(&server).await;
            check!(format!("round {round}"), c.ask("INCR starts").await, int(round + 1));
            c.send("BLPOP q 0").await;
            eventually("blocked", || server.stats().blocked == 1).await;
            let mut other = Client::connect(&server).await;
            other.ask("RPUSH q taken").await;
            c.reply().await;
            check!("the pop is not undone", other.ask("LLEN q").await, int(0));
            server.shutdown().await;
        }
        let _ = std::fs::remove_dir_all(&dir);
    });
}

#[test]
fn the_server_starts_with_the_core_it_is_given() {
    run(async {
        let mut core = Core::new();
        let id = core.connect();
        core.request(id, args("SET ready yes"), 0);
        core.disconnect(id);
        core.take_log();
        let hands = Hands::default();
        let server = serve::<MemDisk>(core, None, config(&hands)).await;
        let mut c = Client::connect(&server).await;
        check!("GET", c.ask("GET ready").await, bulk("yes"));
        server.shutdown().await;
    });
}
