use solution::*;

#[allow(dead_code, unused_imports)]
use std::io::{BufRead, BufReader, Read, Write};
#[allow(dead_code, unused_imports)]
use std::net::{SocketAddr, TcpStream};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::{channel, Receiver};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Condvar, Mutex};
#[allow(dead_code, unused_imports)]
use std::time::{Duration, Instant};

#[allow(dead_code, unused_imports)]
const PATIENCE: Duration = Duration::from_secs(3);

#[derive(Clone, Default)]
#[allow(dead_code, unused_imports)]
struct Hold(Arc<(Mutex<(bool, usize)>, Condvar)>);

#[allow(dead_code, unused_imports)]
impl Hold {
    fn pass(&self) {
        let (lock, cv) = &*self.0;
        let mut s = lock.lock().unwrap();
        s.1 += 1;
        cv.notify_all();
        let _ = cv.wait_timeout_while(s, PATIENCE, |s| !s.0).unwrap();
    }
    fn open(&self) {
        let (lock, cv) = &*self.0;
        lock.lock().unwrap().0 = true;
        cv.notify_all();
    }
    fn wait_for(&self, n: usize) {
        let (lock, cv) = &*self.0;
        let s = lock.lock().unwrap();
        let (s, _) = cv.wait_timeout_while(s, PATIENCE, |s| s.1 < n).unwrap();
        assert!(s.1 >= n, "only {} of {n} lines reached `answer`", s.1);
    }
}

/// Upper-cases each line; a line starting `hold` waits for the test. `alive` is owned by `answer`.
#[allow(dead_code, unused_imports)]
fn server(hold: &Hold, alive: &Arc<()>) -> Server {
    let (hold, alive) = (hold.clone(), alive.clone());
    Server::start(move |line| {
        let _owned = &alive;
        assert!(line != "panic", "asked to panic");
        if line.starts_with("hold") {
            hold.pass();
        }
        line.to_uppercase()
    })
    .expect("start")
}

#[allow(dead_code, unused_imports)]
fn plain() -> Server {
    server(&Hold::default(), &Arc::new(()))
}

#[allow(dead_code, unused_imports)]
struct Client(BufReader<TcpStream>);

#[allow(dead_code, unused_imports)]
impl Client {
    fn connect(addr: SocketAddr) -> Client {
        let s = TcpStream::connect_timeout(&addr, PATIENCE).expect("connect");
        s.set_read_timeout(Some(PATIENCE)).unwrap();
        s.set_write_timeout(Some(PATIENCE)).unwrap();
        s.set_nodelay(true).unwrap();
        Client(BufReader::new(s))
    }
    fn send(&mut self, bytes: &[u8]) {
        self.0.get_mut().write_all(bytes).expect("client write");
    }
    fn try_send(&mut self, bytes: &[u8]) {
        let _ = self.0.get_mut().write_all(bytes);
    }
    fn line(&mut self) -> String {
        let mut line = String::new();
        self.0.read_line(&mut line).unwrap_or_else(|e| panic!("no reply: {e}"));
        line
    }
    fn ask(&mut self, text: &str) -> String {
        self.send(format!("{text}\n").as_bytes());
        self.line()
    }
    fn rest(&mut self) -> String {
        let mut out = String::new();
        if let Err(e) = self.0.read_to_string(&mut out) {
            panic!("the connection did not end cleanly: {e}");
        }
        out
    }
}

/// Runs `stop` on another thread, so that the test can go on while it works.
#[allow(dead_code, unused_imports)]
fn stopping(server: Server) -> Receiver<usize> {
    let (done, result) = channel();
    std::thread::spawn(move || {
        let _ = done.send(server.stop());
    });
    result
}

#[allow(dead_code, unused_imports)]
fn wait_until_refused(addr: SocketAddr) {
    // shorter than a held line waits, so a server that refuses only after its replies are done is caught
    let deadline = Instant::now() + Duration::from_secs(2);
    // only a refusal counts: a listener nobody accepts from fills its backlog, and then connects merely time out
    while !matches!(
        TcpStream::connect_timeout(&addr, Duration::from_millis(200)),
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused
    ) {
        assert!(Instant::now() < deadline, "connections are still accepted");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn it_answers() {
    let s = plain();
    let mut c = Client::connect(s.addr());
    check!("two lines", (c.ask("a"), c.ask("b")), ("A\n".to_string(), "B\n".to_string()));
}

#[test]
fn clients_are_served_at_the_same_time() {
    let s = plain();
    let mut first = Client::connect(s.addr());
    first.send(b"half a line");
    let mut second = Client::connect(s.addr());
    check!("second while first is quiet", second.ask("x"), "X\n".to_string());
}

#[test]
fn stop_with_no_clients() {
    check!("returns 0", stopping(plain()).recv_timeout(PATIENCE).ok(), Some(0));
}

#[test]
fn stop_closes_quiet_clients_and_counts_them() {
    let s = plain();
    let mut a = Client::connect(s.addr());
    let mut b = Client::connect(s.addr());
    a.ask("a");
    b.ask("b");
    check!("two connections", stopping(s).recv_timeout(PATIENCE).ok(), Some(2));
    check!("both closed", (a.rest(), b.rest()), (String::new(), String::new()));
}

#[test]
fn a_reply_in_progress_is_delivered() {
    let hold = Hold::default();
    let s = server(&hold, &Arc::new(()));
    let addr = s.addr();
    let mut c = Client::connect(addr);
    c.send(b"hold this\n");
    hold.wait_for(1);
    let done = stopping(s);
    wait_until_refused(addr);
    hold.open();
    check!("the reply", c.rest(), "HOLD THIS\n".to_string());
    check!("stop returns", done.recv_timeout(PATIENCE).is_ok(), true);
}
