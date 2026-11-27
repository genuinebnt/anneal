//! RECIPE 194 (concurrency): a small actor framework on tokio: mailboxes, request/reply, supervision with restarts, registry.
//! LEVEL: advanced
//! Use in: services with many independent stateful entities (accounts, sessions, devices), fault isolation, "let it crash" designs.   Run: cargo run --release --example actor_model
#![allow(dead_code)]
//!
//! WHAT: an `Actor` owns its state EXCLUSIVELY and processes one message at a time from a bounded mailbox
//! (`tokio::sync::mpsc`). Nobody else touches the state, so there are no locks around it and no data races by construction.
//! `Addr<A>` is the cloneable handle: `send().await` applies back-pressure when the mailbox is full, `try_send` fails fast.
//! Request/reply = put a `oneshot::Sender` in the message (`Addr::ask`). A supervisor task runs the actor as a child task,
//! catches a panic through the `JoinHandle` (`JoinError::is_panic`), waits an exponentially growing backoff, builds a FRESH
//! actor from a factory (state is intentionally reset: "let it crash") and resumes on the SAME mailbox: the receiver lives in
//! an `Arc<tokio::sync::Mutex<..>>` owned by the supervisor, so queued messages survive (only the message being handled when it
//! crashed is lost). After `max_restarts` restarts (a lifetime total, never reset) it gives up and the mailbox closes (senders get `Err`). Graceful stop: a `Stop`
//! envelope (or `Control::Stop`) closes the mailbox to NEW sends and then DRAINS everything already queued before exiting,
//! also across a crash during the drain (if it does not run out of restarts). Aborting the supervisor aborts the actor.
//! A `Registry` maps names to type-erased `Addr`s (`Box<dyn Any>` + downcast).
//!
//! ACTORS vs SHARED-STATE LOCKS (see shared_state/locks_deadlocks for the lock side):
//!  * Locks: simplest and fastest for small shared data (counter, cache) and short critical sections; but lock ordering, deadlock
//!    and holding guards across `.await` are your problem, and one panic can poison state.
//!  * Actors: better when state is large/long-lived, operations are multi-step or call out (await IO), you want serialised
//!    per-entity ordering, back-pressure, isolation and restart-on-failure. Costs: a channel hop per call, reply plumbing,
//!    and deadlock is still possible (two actors awaiting each other's reply): keep call graphs acyclic.
//! Here the bank demo has 10k concurrent transfers across 8 account actors, driven by a coordinator actor, and the invariant
//! "total money is conserved" is asserted.
//! COMMON USES: Erlang/OTP supervision trees (the original "let it crash" design), actix and ractor are Rust actor
//! frameworks worth reading; the same pattern appears as per-connection/per-session tasks owning their state in servers.
//! Read: https://tokio.rs/tokio/tutorial/channels , https://www.erlang.org/doc/system/design_principles.html , https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Control {
    Continue,
    Stop,
}

pub trait Actor: Send + 'static {
    type Msg: Send + 'static;
    fn handle(&mut self, msg: Self::Msg) -> impl Future<Output = Control> + Send;
}

enum Envelope<M> {
    Msg(M),
    Stop,
}

pub struct Addr<A: Actor> {
    tx: mpsc::Sender<Envelope<A::Msg>>,
}
impl<A: Actor> Clone for Addr<A> {
    fn clone(&self) -> Self {
        Addr {
            tx: self.tx.clone(),
        }
    }
}
impl<A: Actor> Addr<A> {
    /// Waits for mailbox space (back-pressure). Err = actor is gone/closed; the message is returned.
    pub async fn send(&self, m: A::Msg) -> Result<(), A::Msg> {
        self.tx.send(Envelope::Msg(m)).await.map_err(|e| match e.0 {
            Envelope::Msg(m) => m,
            Envelope::Stop => unreachable!(),
        })
    }
    pub fn try_send(&self, m: A::Msg) -> Result<(), A::Msg> {
        self.tx
            .try_send(Envelope::Msg(m))
            .map_err(|e| match e.into_inner() {
                Envelope::Msg(m) => m,
                Envelope::Stop => unreachable!(),
            })
    }
    /// Request/reply: build the message around a oneshot sender, then await the answer. None = actor died before replying.
    pub async fn ask<R>(&self, f: impl FnOnce(oneshot::Sender<R>) -> A::Msg) -> Option<R> {
        let (tx, rx) = oneshot::channel();
        self.send(f(tx)).await.ok()?;
        rx.await.ok()
    }
    /// Graceful stop: messages queued before this are processed; later sends are drained or rejected.
    pub async fn stop(&self) {
        let _ = self.tx.send(Envelope::Stop).await;
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Exit {
    Stopped, // Stop requested, mailbox drained
    Drained, // every Addr dropped and the mailbox is empty
    GaveUp,  // too many crashes
}
#[derive(Debug, Clone, Copy)]
pub struct Report {
    pub exit: Exit,
    pub restarts: u32,
}
#[derive(Clone, Copy)]
pub struct Policy {
    pub max_restarts: u32,
    pub backoff: Duration, // doubled with each restart, capped at 8x (the count is never reset)
}

type Rx<A> = Arc<Mutex<mpsc::Receiver<Envelope<<A as Actor>::Msg>>>>;

/// `stopping` lives outside the actor's task: a crash while draining must not forget that a stop was requested.
async fn run<A: Actor>(mut actor: A, rx: Rx<A>, stopping: Arc<AtomicBool>) -> Exit {
    let mut rx = rx.lock().await; // guard is dropped on panic unwinding; tokio's Mutex has no poisoning
    loop {
        match rx.recv().await {
            None => {
                return if stopping.load(Ordering::SeqCst) {
                    Exit::Stopped
                } else {
                    Exit::Drained
                };
            }
            Some(Envelope::Stop) => {
                stopping.store(true, Ordering::SeqCst);
                rx.close(); // refuse new sends, keep draining what is queued
            }
            Some(Envelope::Msg(m)) => {
                if actor.handle(m).await == Control::Stop {
                    stopping.store(true, Ordering::SeqCst);
                    rx.close(); // closing twice is harmless
                }
            }
        }
    }
}

/// Spawn a supervised actor. `factory` builds a fresh actor at start and after every crash (a panic in `factory` itself
/// is not supervised). Aborting the returned handle also aborts the actor.
pub fn spawn_supervised<A: Actor>(
    factory: impl Fn() -> A + Send + 'static,
    mailbox: usize,
    policy: Policy,
) -> (Addr<A>, JoinHandle<Report>) {
    let (tx, rx) = mpsc::channel(mailbox);
    let rx: Rx<A> = Arc::new(Mutex::new(rx));
    let sup = tokio::spawn(async move {
        let mut restarts = 0u32;
        let stopping = Arc::new(AtomicBool::new(false));
        // a JoinSet aborts its tasks when dropped; a plain JoinHandle would leave the actor running without us
        let mut child = JoinSet::new();
        loop {
            child.spawn(run(factory(), rx.clone(), stopping.clone()));
            match child.join_next().await.expect("one child") {
                Ok(exit) => return Report { exit, restarts },
                Err(e) if e.is_panic() => {
                    restarts += 1;
                    if restarts > policy.max_restarts {
                        return Report {
                            exit: Exit::GaveUp,
                            restarts: restarts - 1,
                        };
                    }
                    let mult = 1u32 << (restarts - 1).min(3);
                    tokio::time::sleep(policy.backoff * mult).await;
                }
                Err(_) => {
                    return Report {
                        exit: Exit::Stopped,
                        restarts,
                    };
                } // cancelled
            }
        }
    });
    (Addr { tx }, sup)
}

/// Named actors. Type-erased storage; lookup downcasts back to `Addr<A>`.
#[derive(Default)]
pub struct Registry {
    map: StdMutex<HashMap<String, Box<dyn Any + Send + Sync>>>,
}
impl Registry {
    pub fn register<A: Actor>(&self, name: &str, addr: Addr<A>) -> bool {
        let mut m = self.map.lock().unwrap();
        if m.contains_key(name) {
            return false;
        }
        m.insert(name.to_string(), Box::new(addr));
        true
    }
    pub fn lookup<A: Actor>(&self, name: &str) -> Option<Addr<A>> {
        self.map
            .lock()
            .unwrap()
            .get(name)?
            .downcast_ref::<Addr<A>>()
            .cloned()
    }
    pub fn unregister(&self, name: &str) {
        self.map.lock().unwrap().remove(name);
    }
}

// ---------------- demo 1: bank accounts + coordinator ----------------
#[derive(Debug)]
enum AccMsg {
    Deposit(u64),
    Withdraw(u64, oneshot::Sender<bool>),
    Balance(oneshot::Sender<u64>),
}
struct Account {
    balance: u64,
}
impl Actor for Account {
    type Msg = AccMsg;
    async fn handle(&mut self, msg: AccMsg) -> Control {
        match msg {
            AccMsg::Deposit(n) => self.balance += n,
            AccMsg::Withdraw(n, r) => {
                let ok = self.balance >= n;
                if ok {
                    self.balance -= n;
                }
                let _ = r.send(ok);
            }
            AccMsg::Balance(r) => {
                let _ = r.send(self.balance);
            }
        }
        Control::Continue
    }
}

#[derive(Debug)]
enum CoordMsg {
    Transfer {
        from: usize,
        to: usize,
        amount: u64,
        reply: oneshot::Sender<bool>,
    },
}
struct Coordinator {
    accounts: Vec<Addr<Account>>,
}
impl Actor for Coordinator {
    type Msg = CoordMsg;
    async fn handle(&mut self, msg: CoordMsg) -> Control {
        let CoordMsg::Transfer {
            from,
            to,
            amount,
            reply,
        } = msg;
        // withdraw first; only if that succeeded deposit, so money is never created. Money is "in flight" only inside this call.
        let ok = self.accounts[from]
            .ask(|r| AccMsg::Withdraw(amount, r))
            .await
            == Some(true);
        if ok {
            self.accounts[to]
                .send(AccMsg::Deposit(amount))
                .await
                .expect("account alive");
        }
        let _ = reply.send(ok);
        Control::Continue
    }
}

// ---------------- demo 2: a flaky actor ----------------
struct Flaky {
    processed: Arc<AtomicUsize>,
}
#[derive(Debug)]
enum FlakyMsg {
    Work(u32),
}
impl Actor for Flaky {
    type Msg = FlakyMsg;
    async fn handle(&mut self, msg: FlakyMsg) -> Control {
        let FlakyMsg::Work(i) = msg;
        if i % 5 == 0 {
            panic!("simulated crash on item {i}");
        }
        self.processed.fetch_add(1, Ordering::SeqCst);
        Control::Continue
    }
}

enum DrainMsg {
    Hold(oneshot::Receiver<()>), // waits, so that the check can queue the messages behind it
    Stop,
    Crash,
    Other,
}
struct StopThenCrash;
impl Actor for StopThenCrash {
    type Msg = DrainMsg;
    async fn handle(&mut self, m: DrainMsg) -> Control {
        match m {
            DrainMsg::Hold(gate) => {
                let _ = gate.await;
                Control::Continue
            }
            DrainMsg::Stop => Control::Stop,
            DrainMsg::Crash => panic!("simulated crash while draining"),
            DrainMsg::Other => Control::Continue,
        }
    }
}

struct Counter {
    seen: Arc<AtomicUsize>,
}
impl Actor for Counter {
    type Msg = u32;
    async fn handle(&mut self, _m: u32) -> Control {
        tokio::task::yield_now().await;
        self.seen.fetch_add(1, Ordering::SeqCst);
        Control::Continue
    }
}

fn xorshift(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

#[tokio::main]
async fn main() {
    // --- bank: 8 accounts x 1000, 10k concurrent transfers with seeded random endpoints ---
    const N: usize = 8;
    const START: u64 = 1_000;
    let pol = Policy {
        max_restarts: 0,
        backoff: Duration::from_millis(1),
    };
    let mut accounts = Vec::new();
    let mut handles = Vec::new();
    for _ in 0..N {
        let (a, h) = spawn_supervised(|| Account { balance: START }, 64, pol);
        accounts.push(a);
        handles.push(h);
    }
    let accs = accounts.clone();
    let (coord, coord_h) = spawn_supervised(
        move || Coordinator {
            accounts: accs.clone(),
        },
        64,
        pol,
    );
    let reg = Registry::default();
    assert!(reg.register("coordinator", coord.clone()));
    assert!(
        !reg.register("coordinator", coord.clone()),
        "names are unique"
    );
    assert!(
        reg.lookup::<Account>("coordinator").is_none(),
        "wrong type does not downcast"
    );
    drop(coord);

    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut tasks = Vec::new();
    for _ in 0..10_000 {
        let (from, mut to) = (
            (xorshift(&mut seed) % N as u64) as usize,
            (xorshift(&mut seed) % N as u64) as usize,
        );
        if to == from {
            to = (to + 1) % N;
        }
        let amount = xorshift(&mut seed) % 400 + 1;
        let c = reg
            .lookup::<Coordinator>("coordinator")
            .expect("registered");
        tasks.push(tokio::spawn(async move {
            c.ask(|reply| CoordMsg::Transfer {
                from,
                to,
                amount,
                reply,
            })
            .await
            .unwrap()
        }));
    }
    let mut ok = 0;
    for t in tasks {
        ok += t.await.unwrap() as usize;
    }
    let mut total = 0;
    for a in &accounts {
        total += a.ask(AccMsg::Balance).await.unwrap();
    }
    assert_eq!(total, N as u64 * START, "money must be conserved");
    assert!(ok > 0 && ok <= 10_000);
    println!(
        "bank: {ok}/10000 transfers succeeded, {} rejected (insufficient funds), total = {total} (conserved)",
        10_000 - ok
    );
    reg.unregister("coordinator");
    drop(accounts);
    let r = coord_h.await.unwrap();
    assert_eq!(
        r.exit,
        Exit::Drained,
        "all Addrs dropped: coordinator drains and exits"
    );
    for h in handles {
        assert_eq!(h.await.unwrap().exit, Exit::Drained);
    }

    // --- flaky actor restarted: multiples of 5 crash it; mailbox survives every crash ---
    std::panic::set_hook(Box::new(|_| {})); // silence the simulated panics
    let processed = Arc::new(AtomicUsize::new(0));
    let built = Arc::new(AtomicUsize::new(0));
    let (p2, b2) = (processed.clone(), built.clone());
    let pol = Policy {
        max_restarts: 10,
        backoff: Duration::from_millis(1),
    };
    let (addr, h) = spawn_supervised(
        move || {
            b2.fetch_add(1, Ordering::SeqCst);
            Flaky {
                processed: p2.clone(),
            }
        },
        8,
        pol,
    );
    for i in 1..=50u32 {
        addr.send(FlakyMsg::Work(i)).await.unwrap();
    }
    drop(addr);
    let rep = h.await.unwrap();
    assert_eq!((rep.exit, rep.restarts), (Exit::Drained, 10));
    assert_eq!(
        processed.load(Ordering::SeqCst),
        40,
        "all non-crashing messages processed across restarts"
    );
    assert_eq!(built.load(Ordering::SeqCst), 11, "1 initial + 10 restarts");
    println!(
        "flaky: restarted {} times, {} messages processed, mailbox preserved",
        rep.restarts,
        processed.load(Ordering::SeqCst)
    );

    // --- restart limit: always-crashing input makes the supervisor give up and close the mailbox ---
    let (addr, h) = spawn_supervised(
        || Flaky {
            processed: Arc::new(AtomicUsize::new(0)),
        },
        4,
        Policy {
            max_restarts: 2,
            backoff: Duration::from_millis(1),
        },
    );
    for i in [5u32, 10, 15, 20] {
        let _ = addr.send(FlakyMsg::Work(i)).await;
    }
    let rep = h.await.unwrap();
    assert_eq!((rep.exit, rep.restarts), (Exit::GaveUp, 2));
    assert!(
        addr.send(FlakyMsg::Work(1)).await.is_err(),
        "mailbox closed after giving up"
    );
    assert!(addr.try_send(FlakyMsg::Work(1)).is_err());

    // --- a crash while draining: the stop request survives the restart (an Addr is still alive, so not `Drained`).
    let (addr, h) = spawn_supervised(|| StopThenCrash, 8, pol);
    let (open, gate) = oneshot::channel();
    for m in [
        DrainMsg::Hold(gate),
        DrainMsg::Stop,
        DrainMsg::Crash,
        DrainMsg::Other,
    ] {
        assert!(addr.send(m).await.is_ok());
    }
    open.send(()).unwrap();
    let rep2 = h.await.unwrap();
    let _ = std::panic::take_hook();
    assert_eq!((rep2.exit, rep2.restarts), (Exit::Stopped, 1));

    // --- aborting the supervisor takes the actor with it: nobody is left to answer
    let (account, h) = spawn_supervised(|| Account { balance: 7 }, 8, pol);
    assert_eq!(account.ask(AccMsg::Balance).await, Some(7));
    h.abort();
    assert!(h.await.unwrap_err().is_cancelled());
    // the actor's task is cancelled and drops the mailbox; the time limit only turns a hang into a failure
    tokio::time::timeout(Duration::from_secs(10), account.tx.closed())
        .await
        .expect("no orphan actor keeps the mailbox open");
    assert_eq!(account.ask(AccMsg::Balance).await, None);
    println!(
        "limit: gave up after {} restarts, sends now fail",
        rep.restarts
    );

    // --- graceful stop drains the mailbox; try_send shows back-pressure ---
    let seen = Arc::new(AtomicUsize::new(0));
    let s2 = seen.clone();
    let (addr, h) = spawn_supervised(move || Counter { seen: s2.clone() }, 16, pol);
    for i in 0..100 {
        addr.send(i).await.unwrap();
    }
    let late = addr.clone();
    let late_task = tokio::spawn(async move {
        let mut accepted = 0;
        for i in 0..50 {
            if late.send(i).await.is_ok() {
                accepted += 1;
            }
        }
        accepted
    });
    addr.stop().await;
    let accepted = late_task.await.unwrap();
    let rep = h.await.unwrap();
    assert_eq!(rep.exit, Exit::Stopped);
    assert_eq!(
        seen.load(Ordering::SeqCst),
        100 + accepted,
        "everything accepted before close was drained"
    );
    println!(
        "stop: processed {} = 100 + {accepted} late messages that were accepted",
        seen.load(Ordering::SeqCst)
    );
    let (full, h) = spawn_supervised(
        || Counter {
            seen: Arc::new(AtomicUsize::new(0)),
        },
        2,
        pol,
    );
    let rejected = (0..1000).filter(|i| full.try_send(*i).is_err()).count();
    assert!(rejected > 0, "bounded mailbox rejects when full");
    drop(full);
    h.await.unwrap();
    println!("ALL OK");
}
