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
fn stop_waits_for_every_reply_in_progress() {
    let hold = Hold::default();
    let alive = Arc::new(());
    let s = server(&hold, &alive);
    let addr = s.addr();
    let mut clients: Vec<Client> = (0..3).map(|i| {
        let mut c = Client::connect(addr);
        c.send(format!("hold {i}\n").as_bytes());
        c
    }).collect();
    hold.wait_for(3);
    let done = stopping(s);
    wait_until_refused(addr);
    check!("not back while lines are being answered", done.recv_timeout(Duration::from_millis(150)).is_err(), true);
    hold.open();
    for (i, c) in clients.iter_mut().enumerate() {
        check!(format!("client {i}"), c.rest(), format!("HOLD {i}\n"));
    }
    check!("then back", done.recv_timeout(PATIENCE).is_ok(), true);
    check!("answer released", Arc::strong_count(&alive), 1);
}

#[test]
fn answer_is_released_when_stop_returns() {
    for _ in 0..5 {
        let alive = Arc::new(());
        let s = server(&Hold::default(), &alive);
        let mut clients: Vec<Client> = (0..6).map(|_| Client::connect(s.addr())).collect();
        for c in &mut clients {
            c.ask("x");
        }
        check!("returns", stopping(s).recv_timeout(PATIENCE).is_ok(), true);
        check!("owners of what answer holds", Arc::strong_count(&alive), 1);
    }
}

#[test]
fn a_half_line_does_not_hold_it_up_and_is_not_answered() {
    let s = plain();
    let mut c = Client::connect(s.addr());
    c.ask("whole");
    c.send(b"half");
    check!("returns", stopping(s).recv_timeout(PATIENCE).is_ok(), true);
    check!("nothing more", c.rest(), String::new());
}

#[test]
fn a_client_that_never_stops_dripping() {
    let s = plain();
    let mut c = Client::connect(s.addr());
    c.ask("first");
    let done = stopping(s);
    let mut returned = false;
    for _ in 0..120 {
        c.try_send(b"a");
        if done.recv_timeout(Duration::from_millis(10)).is_ok() {
            returned = true;
            break;
        }
    }
    check!("returned while bytes kept coming", returned, true);
}

#[test]
fn drop_stops_it_too() {
    let hold = Hold::default();
    let alive = Arc::new(());
    let s = server(&hold, &alive);
    let addr = s.addr();
    let mut c = Client::connect(addr);
    c.send(b"hold\n");
    hold.wait_for(1);
    let (done, dropped) = channel();
    std::thread::spawn(move || {
        drop(s);
        let _ = done.send(());
    });
    check!("drop waits", dropped.recv_timeout(Duration::from_millis(150)).is_err(), true);
    hold.open();
    check!("drop returns", dropped.recv_timeout(PATIENCE).is_ok(), true);
    check!("reply", c.rest(), "HOLD\n".to_string());
    check!("released", Arc::strong_count(&alive), 1);
    wait_until_refused(addr);
}

#[test]
fn clients_that_left_are_counted_and_do_no_harm() {
    let s = plain();
    for i in 0..10 {
        let mut c = Client::connect(s.addr());
        check!(format!("client {i}"), c.ask("x"), "X\n".to_string());
    }
    check!("ten", stopping(s).recv_timeout(PATIENCE).ok(), Some(10));
}

#[test]
fn a_panic_costs_only_its_connection() {
    let alive = Arc::new(());
    let s = server(&Hold::default(), &alive);
    let mut other = Client::connect(s.addr());
    other.ask("before");
    let mut bad = Client::connect(s.addr());
    bad.send(b"panic\n");
    let mut out = Vec::new();
    let _ = bad.0.read_to_end(&mut out);
    check!("the other goes on", other.ask("after"), "AFTER\n".to_string());
    check!("new clients too", Client::connect(s.addr()).ask("new"), "NEW\n".to_string());
    check!("stop", stopping(s).recv_timeout(PATIENCE).ok(), Some(3));
    check!("released", Arc::strong_count(&alive), 1);
}

#[test]
fn many_stops_in_a_row() {
    for round in 0..30 {
        let s = plain();
        let mut c = Client::connect(s.addr());
        check!(format!("round {round}"), c.ask("r"), "R\n".to_string());
        check!(format!("round {round} stops"), stopping(s).recv_timeout(PATIENCE).ok(), Some(1));
    }
}

#[test]
fn one_server_stopping_leaves_another() {
    let (a, b) = (plain(), plain());
    let mut c = Client::connect(b.addr());
    check!("a stops", stopping(a).recv_timeout(PATIENCE).ok(), Some(0));
    check!("b answers", c.ask("b"), "B\n".to_string());
}
