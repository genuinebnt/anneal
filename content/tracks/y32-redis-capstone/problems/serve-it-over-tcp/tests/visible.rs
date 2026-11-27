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
fn commands_over_a_socket() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        check!("PING", c.ask("PING").await, simple("PONG"));
        check!("SET", c.ask("SET k v").await, ok());
        check!("GET", c.ask("GET k").await, bulk("v"));
        c.write(b"get k\r\n").await;
        check!("typed by hand", c.reply().await, bulk("v"));
        server.shutdown().await;
    });
}

#[test]
fn many_commands_in_one_write() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        let mut bytes = Vec::new();
        for _ in 0..500 {
            bytes.extend(encode_command(&args("INCR n")));
        }
        c.write(&bytes).await;
        for i in 1..=500 {
            check!(format!("reply {i}"), c.reply().await, int(i));
        }
        server.shutdown().await;
    });
}

#[test]
fn a_push_wakes_a_client_on_another_connection() {
    run(async {
        let (server, _) = memory_server().await;
        let (mut worker, mut boss) = (Client::connect(&server).await, Client::connect(&server).await);
        worker.send("BLPOP jobs 0").await;
        eventually("the worker is blocked", || server.stats().blocked == 1).await;
        check!("RPUSH", boss.ask("RPUSH jobs j1").await, int(1));
        check!("the worker", worker.reply().await, arr(&["jobs", "j1"]));
        server.shutdown().await;
    });
}

#[test]
fn time_comes_from_the_clock() {
    run(async {
        let (server, hands) = memory_server().await;
        let mut c = Client::connect(&server).await;
        c.ask("SET k v PX 100").await;
        check!("before", c.ask("GET k").await, bulk("v"));
        hands.set(2000);
        check!("after", c.ask("GET k").await, Frame::Null);
        c.send("BLPOP q 5").await;
        eventually("blocked", || server.stats().blocked == 1).await;
        hands.set(7000);
        check!("the time is up", c.reply().await, Frame::NullArray);
        server.shutdown().await;
    });
}

#[test]
fn a_broken_stream_is_told_so_and_closed() {
    run(async {
        let (server, _) = memory_server().await;
        let mut c = Client::connect(&server).await;
        let mut bytes = encode_command(&args("SET k v"));
        bytes.extend(b"*1\r\n:oops\r\n");
        bytes.extend(encode_command(&args("SET never run")));
        c.write(&bytes).await;
        check!("the good command first", c.reply().await, ok());
        let reply = c.reply().await;
        check!(format!("then {reply:?}"), is_error(&reply, "ERR Protocol error"), true);
        check!("then the end", c.next().await, None);
        let mut other = Client::connect(&server).await;
        check!("nothing after the bad bytes ran", other.ask("EXISTS never").await, int(0));
        server.shutdown().await;
    });
}

#[test]
fn shutdown_closes_everything() {
    run(async {
        let (server, _) = memory_server().await;
        let (mut idle, mut waiting) = (Client::connect(&server).await, Client::connect(&server).await);
        idle.ask("PING").await;
        waiting.send("BLPOP q 0").await;
        eventually("blocked", || server.stats().blocked == 1).await;
        let addr = server.addr();
        server.shutdown().await;
        check!("the idle client", idle.next().await, None);
        check!("the blocked client", waiting.next().await, None);
        let again = tokio::time::timeout(PATIENCE, TcpStream::connect(addr)).await.expect("in time");
        check!("nobody is listening", again.is_err(), true);
    });
}
