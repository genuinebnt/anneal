//! RECIPE 213 (patterns): high-level design patterns in idiomatic Rust: builder, strategy, observer, visitor, command, DI and more.
//! LEVEL: advanced
//! Use in: application architecture, library APIs, editors, interpreters, plugin hosts.   Run: cargo run --release --example design_patterns_high
#![allow(dead_code)]
//!
//! The GoF catalogue was written for languages with inheritance. Rust has enums, traits, closures, ownership and RAII, so most
//! patterns shrink to a few lines. Each numbered section below is small, tested, and ends with the trade-off that matters.
//!
//! | pattern              | Rust idiom                                  | enum when...                        | trait object when...                  |
//! |----------------------|---------------------------------------------|-------------------------------------|---------------------------------------|
//! | builder              | consuming `self` or `&mut self` + `build()` | n/a (validate in `build`)           | n/a                                   |
//! | strategy             | generic `S: Fn`/trait, or `Box<dyn Fn>`      | fixed, known set of algorithms      | set chosen at runtime / plugin        |
//! | observer             | `Vec<Weak<dyn Observer>>`                    | closed set of listener kinds        | open set (the usual case)             |
//! | visitor              | `match` on enum (default) / `Visitor` trait  | MANY operations, FEW node types     | many node types, third-party nodes    |
//! | command + undo       | enum of commands returning their inverse     | closed command set (serialisable)   | user-defined commands                 |
//! | interpreter          | AST enum + recursive `eval`                  | always (the AST is closed data)     | rarely                                |
//! | chain of resp.       | `Vec<Box<dyn Middleware>>` + `Next`          | fixed pipeline                      | composable, user-supplied stages      |
//! | dependency injection | generic param (static) / `Arc<dyn T>`        | n/a                                 | runtime-swappable, heterogeneous      |
//! | plugin registry      | `HashMap<&str, fn() -> Box<dyn Plugin>>`     | built-in plugins only               | third-party plugins                   |
//! | newtype              | tuple struct; `Deref` ONLY for read-only     | n/a                                 | n/a                                   |
//! | RAII guard           | struct with `Drop`                            | n/a                                 | n/a                                   |
//! | illegal states       | enum variants carry the data they need       | ALWAYS prefer                       | n/a                                   |
//!
//! Rule of thumb: an enum is a CLOSED world (you own every case; the compiler checks exhaustiveness; no allocation; adding an
//! OPERATION is easy). A trait object is an OPEN world (others add cases; one vtable call per use; adding a VARIANT is easy).
//! The "expression problem" is exactly this choice. Rust has no inheritance, so patterns built on it become composition.
//! COMMON USES: HTTP middleware stacks, text editors/undo (e.g. in IDE buffers), compilers (AST walkers, rustc's `Visitor`),
//! typed configuration builders (`std::process::Command`, `reqwest::ClientBuilder`), mockable services in tests, plugin hosts.
//! Read: https://rust-unofficial.github.io/patterns/ , https://rust-lang.github.io/api-guidelines/ , https://docs.rs/inventory , https://docs.rs/linkme
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------- 1. builder
#[derive(Debug, PartialEq)]
struct Server {
    host: String,
    port: u16,
    workers: usize,
}
#[derive(Debug, PartialEq)]
enum BuildError {
    MissingHost,
    ZeroWorkers,
}

/// CONSUMING builder: `self -> Self`. Chains from a temporary, moves non-Clone fields into the product for free, and `build(self)`
/// cannot be called twice. Downside: awkward to configure conditionally (`b = b.x()` rebinding).
#[derive(Default)]
struct ServerBuilder {
    host: Option<String>,
    port: Option<u16>,
    workers: Option<usize>,
}
impl ServerBuilder {
    fn host(mut self, h: impl Into<String>) -> Self {
        self.host = Some(h.into());
        self
    }
    fn port(mut self, p: u16) -> Self {
        self.port = Some(p);
        self
    }
    fn workers(mut self, n: usize) -> Self {
        self.workers = Some(n);
        self
    }
    /// Validation lives here: an invalid `Server` can never exist.
    fn build(self) -> Result<Server, BuildError> {
        let host = self.host.ok_or(BuildError::MissingHost)?;
        let workers = self.workers.unwrap_or(4);
        if workers == 0 {
            return Err(BuildError::ZeroWorkers);
        }
        Ok(Server {
            host,
            port: self.port.unwrap_or(8080),
            workers,
        })
    }
}

/// `&mut self` builder (like `std::process::Command`): reusable, easy to configure in `if`/loops; `build(&self)` must clone.
#[derive(Default)]
struct ServerBuilderMut {
    inner: ServerBuilder,
}
impl ServerBuilderMut {
    fn host(&mut self, h: &str) -> &mut Self {
        self.inner.host = Some(h.to_string());
        self
    }
    fn workers(&mut self, n: usize) -> &mut Self {
        self.inner.workers = Some(n);
        self
    }
    fn build(&self) -> Result<Server, BuildError> {
        ServerBuilder {
            host: self.inner.host.clone(),
            port: self.inner.port,
            workers: self.inner.workers,
        }
        .build()
    }
}

fn demo_builder() {
    let s = ServerBuilder::default()
        .host("example.org")
        .port(9000)
        .build()
        .unwrap();
    assert_eq!(
        s,
        Server {
            host: "example.org".into(),
            port: 9000,
            workers: 4
        }
    );
    assert_eq!(
        ServerBuilder::default().build(),
        Err(BuildError::MissingHost)
    );
    assert_eq!(
        ServerBuilder::default().host("h").workers(0).build(),
        Err(BuildError::ZeroWorkers)
    );
    let mut b = ServerBuilderMut::default();
    b.host("a");
    for n in [1, 2, 3] {
        b.workers(n); // reuse the same builder, producing several products
        assert_eq!(b.build().unwrap().workers, n);
    }
}

// --------------------------------------------------------------- 2. strategy
/// Static dispatch: monomorphised, the closure can inline. A non-capturing closure is a zero-sized type.
fn apply_generic<F: Fn(i64) -> i64>(f: F, xs: &[i64]) -> Vec<i64> {
    xs.iter().map(|&x| f(x)).collect()
}
/// Dynamic dispatch: one vtable call per element, but ONE compiled copy and the strategy can be picked at runtime.
fn apply_dyn(f: &dyn Fn(i64) -> i64, xs: &[i64]) -> Vec<i64> {
    xs.iter().map(|&x| f(x)).collect()
}
trait Pricing {
    fn price(&self, base: u32) -> u32;
}
struct Flat;
struct Percent(u32);
impl Pricing for Flat {
    fn price(&self, base: u32) -> u32 {
        base
    }
}
impl Pricing for Percent {
    fn price(&self, base: u32) -> u32 {
        base * (100 - self.0) / 100
    }
}
fn demo_strategy() {
    let xs = [1, 2, 3];
    assert_eq!(apply_generic(|x| x * 2, &xs), apply_dyn(&|x| x * 2, &xs));
    // measured cost model: a non-capturing closure stores nothing; a boxed strategy is a fat pointer (data + vtable).
    let double = |x: i64| x * 2;
    assert_eq!(std::mem::size_of_val(&double), 0);
    assert_eq!(
        std::mem::size_of::<Box<dyn Fn(i64) -> i64>>(),
        2 * std::mem::size_of::<usize>()
    );
    // runtime choice needs a trait object (or an enum)
    let pick: Box<dyn Pricing> = if xs.len() > 2 {
        Box::new(Percent(10))
    } else {
        Box::new(Flat)
    };
    assert_eq!(pick.price(200), 180);
}

// --------------------------------------------------------------- 3. observer
trait Observer {
    fn notify(&self, event: &str);
}
struct Recorder {
    seen: RefCell<Vec<String>>,
}
impl Observer for Recorder {
    fn notify(&self, e: &str) {
        self.seen.borrow_mut().push(e.to_string());
    }
}
/// The subject holds only `Weak` references: it does not keep subscribers alive, and dead ones are pruned on publish,
/// so there are no leaks and no explicit unsubscribe (the usual source of observer bugs).
#[derive(Default)]
struct Subject {
    subs: Vec<Weak<dyn Observer>>,
}
impl Subject {
    fn subscribe<O: Observer + 'static>(&mut self, o: &Rc<O>) {
        let w: Weak<O> = Rc::downgrade(o);
        let w: Weak<dyn Observer> = w; // unsized coercion
        self.subs.push(w);
    }
    fn publish(&mut self, e: &str) -> usize {
        self.subs.retain(|w| w.strong_count() > 0);
        for w in &self.subs {
            if let Some(o) = w.upgrade() {
                o.notify(e);
            }
        }
        self.subs.len()
    }
}
fn demo_observer() {
    let mut subject = Subject::default();
    let a = Rc::new(Recorder {
        seen: RefCell::default(),
    });
    {
        let b = Rc::new(Recorder {
            seen: RefCell::default(),
        });
        subject.subscribe(&a);
        subject.subscribe(&b);
        assert_eq!(subject.publish("one"), 2);
        assert_eq!(b.seen.borrow().len(), 1);
    } // b dropped here
    assert_eq!(subject.publish("two"), 1, "dead subscriber pruned");
    assert_eq!(*a.seen.borrow(), vec!["one", "two"]);
}

// ----------------------------------------------- 4+6. visitor and interpreter
#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Num(f64),
    Var(String),
    Neg(Box<Expr>),
    Bin(Box<Expr>, char, Box<Expr>),
}
use Expr::*;
fn bin(l: Expr, op: char, r: Expr) -> Expr {
    Bin(Box::new(l), op, Box::new(r))
}

/// Classic trait visitor: adding a new OPERATION = a new impl (open), adding a new NODE type = edit the trait and every impl.
trait Visitor {
    type Out;
    fn num(&mut self, n: f64) -> Self::Out;
    fn var(&mut self, name: &str) -> Self::Out;
    fn neg(&mut self, inner: Self::Out) -> Self::Out;
    fn bin(&mut self, l: Self::Out, op: char, r: Self::Out) -> Self::Out;
}
fn walk<V: Visitor>(e: &Expr, v: &mut V) -> V::Out {
    match e {
        Num(n) => v.num(*n),
        Var(s) => v.var(s),
        Neg(i) => {
            let o = walk(i, v);
            v.neg(o)
        }
        Bin(l, op, r) => {
            let (a, b) = (walk(l, v), walk(r, v));
            v.bin(a, *op, b)
        }
    }
}
struct Printer;
impl Visitor for Printer {
    type Out = String;
    fn num(&mut self, n: f64) -> String {
        n.to_string()
    }
    fn var(&mut self, s: &str) -> String {
        s.to_string()
    }
    fn neg(&mut self, i: String) -> String {
        format!("-{i}")
    }
    fn bin(&mut self, l: String, op: char, r: String) -> String {
        format!("({l} {op} {r})")
    }
}
struct NodeCount(usize);
impl Visitor for NodeCount {
    type Out = ();
    fn num(&mut self, _: f64) {
        self.0 += 1
    }
    fn var(&mut self, _: &str) {
        self.0 += 1
    }
    fn neg(&mut self, _: ()) {
        self.0 += 1
    }
    fn bin(&mut self, _: (), _: char, _: ()) {
        self.0 += 1
    }
}

/// The idiomatic alternative: plain `match`. A new operation is just a new function (no trait, no double dispatch), and adding a
/// variant makes the compiler list every `match` to fix. This doubles as the INTERPRETER pattern: the AST is data, `eval` is
/// the interpreter, and the environment is an explicit argument.
fn eval(e: &Expr, env: &HashMap<&str, f64>) -> Result<f64, String> {
    Ok(match e {
        Num(n) => *n,
        Var(s) => *env
            .get(s.as_str())
            .ok_or_else(|| format!("unbound variable {s}"))?,
        Neg(i) => -eval(i, env)?,
        Bin(l, op, r) => {
            let (a, b) = (eval(l, env)?, eval(r, env)?);
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' if b == 0.0 => return Err("division by zero".into()),
                '/' => a / b,
                _ => return Err(format!("unknown operator {op}")),
            }
        }
    })
}
fn demo_visitor_interpreter() {
    // (x + 2) * -y
    let ast = bin(
        bin(Var("x".into()), '+', Num(2.0)),
        '*',
        Neg(Box::new(Var("y".into()))),
    );
    assert_eq!(walk(&ast, &mut Printer), "((x + 2) * -y)");
    let mut c = NodeCount(0);
    walk(&ast, &mut c);
    assert_eq!(c.0, 6);
    let env = HashMap::from([("x", 3.0), ("y", 2.0)]);
    assert_eq!(eval(&ast, &env), Ok(-10.0));
    assert_eq!(
        eval(&bin(Num(1.0), '/', Num(0.0)), &env),
        Err("division by zero".into())
    );
    assert!(
        eval(&Var("z".into()), &env)
            .unwrap_err()
            .contains("unbound")
    );
}

// ------------------------------------------------- 5. command with undo/redo
/// Every command, when applied, returns its INVERSE. Undo applies the inverse and files the result on the redo stack.
#[derive(Debug, Clone, PartialEq)]
enum Cmd {
    Insert { at: usize, text: String },
    Delete { at: usize, len: usize },
}
impl Cmd {
    fn apply(&self, buf: &mut String) -> Cmd {
        match self {
            Cmd::Insert { at, text } => {
                buf.insert_str(*at, text);
                Cmd::Delete {
                    at: *at,
                    len: text.len(),
                }
            }
            Cmd::Delete { at, len } => {
                let removed: String = buf.drain(*at..*at + *len).collect();
                Cmd::Insert {
                    at: *at,
                    text: removed,
                }
            }
        }
    }
}
#[derive(Default)]
struct Editor {
    text: String,
    undo: Vec<Cmd>,
    redo: Vec<Cmd>,
}
impl Editor {
    fn exec(&mut self, c: Cmd) {
        let inv = c.apply(&mut self.text);
        self.undo.push(inv);
        self.redo.clear(); // a new edit invalidates the redo history
    }
    fn undo(&mut self) -> bool {
        let Some(inv) = self.undo.pop() else {
            return false;
        };
        let fwd = inv.apply(&mut self.text);
        self.redo.push(fwd);
        true
    }
    fn redo(&mut self) -> bool {
        let Some(fwd) = self.redo.pop() else {
            return false;
        };
        let inv = fwd.apply(&mut self.text);
        self.undo.push(inv);
        true
    }
}
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}
fn demo_command() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _round in 0..50 {
        let mut ed = Editor::default();
        let mut snapshots = vec![ed.text.clone()];
        let steps = 1 + rng.below(30);
        for _ in 0..steps {
            let len = ed.text.len();
            let cmd = if len == 0 || rng.below(3) > 0 {
                let word: String = (0..1 + rng.below(4))
                    .map(|_| (b'a' + rng.below(26) as u8) as char)
                    .collect();
                Cmd::Insert {
                    at: rng.below(len + 1),
                    text: word,
                }
            } else {
                let at = rng.below(len);
                Cmd::Delete {
                    at,
                    len: 1 + rng.below(len - at),
                }
            };
            ed.exec(cmd);
            snapshots.push(ed.text.clone());
        }
        // undo walks back through every snapshot...
        for i in (0..steps).rev() {
            assert!(ed.undo());
            assert_eq!(ed.text, snapshots[i]);
        }
        assert!(!ed.undo());
        // ...and redo walks forward again: undo(redo(x)) == x and redo(undo(x)) == x.
        for i in 1..=steps {
            assert!(ed.redo());
            assert_eq!(ed.text, snapshots[i]);
            assert!(ed.undo() && ed.redo());
            assert_eq!(ed.text, snapshots[i]);
        }
        assert!(!ed.redo());
    }
}

// --------------------------------------- 7. chain of responsibility / middleware
struct Request {
    path: String,
    user: Option<String>,
}
#[derive(Debug, PartialEq)]
struct Response {
    status: u16,
    body: String,
}
struct Next<'a> {
    rest: &'a [Box<dyn Middleware>],
    endpoint: &'a dyn Fn(&Request) -> Response,
}
impl Next<'_> {
    fn run(&self, req: &Request) -> Response {
        match self.rest.split_first() {
            Some((m, tail)) => m.handle(
                req,
                &Next {
                    rest: tail,
                    endpoint: self.endpoint,
                },
            ),
            None => (self.endpoint)(req),
        }
    }
}
trait Middleware {
    /// May short-circuit (return without calling `next`) or wrap the downstream response.
    fn handle(&self, req: &Request, next: &Next) -> Response;
}
struct Auth;
impl Middleware for Auth {
    fn handle(&self, req: &Request, next: &Next) -> Response {
        if req.user.is_none() {
            return Response {
                status: 401,
                body: "login required".into(),
            };
        }
        next.run(req)
    }
}
struct Log(Rc<RefCell<Vec<String>>>);
impl Middleware for Log {
    fn handle(&self, req: &Request, next: &Next) -> Response {
        let resp = next.run(req);
        self.0
            .borrow_mut()
            .push(format!("{} -> {}", req.path, resp.status));
        resp
    }
}
fn demo_chain() {
    let log = Rc::new(RefCell::new(vec![]));
    let chain: Vec<Box<dyn Middleware>> = vec![Box::new(Log(log.clone())), Box::new(Auth)];
    let endpoint = |r: &Request| Response {
        status: 200,
        body: format!("hello {}", r.user.as_deref().unwrap_or("?")),
    };
    let run = |req: &Request| {
        Next {
            rest: &chain,
            endpoint: &endpoint,
        }
        .run(req)
    };
    let ok = run(&Request {
        path: "/a".into(),
        user: Some("ann".into()),
    });
    let denied = run(&Request {
        path: "/b".into(),
        user: None,
    });
    assert_eq!(
        ok,
        Response {
            status: 200,
            body: "hello ann".into()
        }
    );
    assert_eq!(denied.status, 401);
    // Log is outermost so it sees the short-circuited 401 too: ORDER matters (see tower_service_layer for the async/tower version).
    assert_eq!(*log.borrow(), vec!["/a -> 200", "/b -> 401"]);
}

// ----------------------------------------------------- 8. dependency injection
trait Mailer: Send + Sync {
    fn send(&self, to: &str, body: &str) -> Result<(), String>;
}
/// Generic parameter: zero cost, resolved at compile time, but the type spreads (`Signup<M>`) through callers.
struct Signup<M: Mailer> {
    mailer: M,
}
impl<M: Mailer> Signup<M> {
    fn register(&self, user: &str) -> Result<(), String> {
        self.mailer.send(user, "welcome")
    }
}
/// `Arc<dyn Trait>`: one concrete type, choose the implementation at runtime (config, tests), shareable across threads.
struct SignupDyn {
    mailer: Arc<dyn Mailer>,
}
impl SignupDyn {
    fn register(&self, user: &str) -> Result<(), String> {
        self.mailer.send(user, "welcome")
    }
}
#[derive(Default)]
struct FakeMailer {
    sent: Mutex<Vec<(String, String)>>,
    fail: bool,
}
impl Mailer for FakeMailer {
    fn send(&self, to: &str, body: &str) -> Result<(), String> {
        if self.fail {
            return Err("smtp down".into());
        }
        self.sent.lock().unwrap().push((to.into(), body.into()));
        Ok(())
    }
}
impl<T: Mailer + ?Sized> Mailer for Arc<T> {
    fn send(&self, to: &str, body: &str) -> Result<(), String> {
        (**self).send(to, body)
    }
}
fn demo_di() {
    let fake = Arc::new(FakeMailer::default());
    let svc = Signup {
        mailer: fake.clone(),
    }; // generic, inspect the fake afterwards through our Arc
    svc.register("ann@x.io").unwrap();
    assert_eq!(
        fake.sent.lock().unwrap()[0],
        ("ann@x.io".to_string(), "welcome".to_string())
    );
    let failing: Arc<dyn Mailer> = Arc::new(FakeMailer {
        fail: true,
        ..Default::default()
    });
    assert_eq!(
        SignupDyn { mailer: failing }.register("b").unwrap_err(),
        "smtp down"
    );
}

// ----------------------------------------------------------- 9. plugin registry
trait Plugin {
    fn run(&self, input: &str) -> String;
}
struct Upper;
struct Reverse;
impl Plugin for Upper {
    fn run(&self, i: &str) -> String {
        i.to_uppercase()
    }
}
impl Plugin for Reverse {
    fn run(&self, i: &str) -> String {
        i.chars().rev().collect()
    }
}
/// Explicit registration: boring, greppable, order-controlled. Crates such as `inventory` and `linkme` collect registrations
/// declared next to each plugin automatically (link-time / life-before-main), at the price of some magic and platform caveats.
#[derive(Default)]
struct Registry {
    factories: HashMap<&'static str, fn() -> Box<dyn Plugin>>,
}
impl Registry {
    fn register(&mut self, name: &'static str, f: fn() -> Box<dyn Plugin>) -> &mut Self {
        assert!(
            self.factories.insert(name, f).is_none(),
            "duplicate plugin {name}"
        );
        self
    }
    fn create(&self, name: &str) -> Option<Box<dyn Plugin>> {
        self.factories.get(name).map(|f| f())
    }
}
fn demo_plugins() {
    let mut reg = Registry::default();
    reg.register("upper", || Box::new(Upper))
        .register("reverse", || Box::new(Reverse));
    assert_eq!(reg.create("upper").unwrap().run("abc"), "ABC");
    assert_eq!(reg.create("reverse").unwrap().run("abc"), "cba");
    assert!(reg.create("nope").is_none());
}

// ------------------------------------------------------ 10. newtype + Deref caveats
/// Invariant: always sorted. `Deref<Target=[i32]>` exposes READ-ONLY slice methods (len, iter, binary_search...) for free.
/// Do NOT also implement `DerefMut` or deref to `Vec`: callers could then `push`/`swap` and break the invariant.
/// Also: `Deref` is meant for smart pointers; using it for "inheritance" makes method resolution surprising.
struct Sorted(Vec<i32>);
impl Sorted {
    fn new(mut v: Vec<i32>) -> Self {
        v.sort();
        Sorted(v)
    }
    fn insert(&mut self, x: i32) {
        let i = self.0.partition_point(|&y| y < x);
        self.0.insert(i, x);
    }
}
impl Deref for Sorted {
    type Target = [i32];
    fn deref(&self) -> &[i32] {
        &self.0
    }
}
fn demo_newtype() {
    let mut s = Sorted::new(vec![5, 1, 3]);
    s.insert(4);
    assert_eq!(&*s, &[1, 3, 4, 5]);
    assert_eq!(s.binary_search(&4), Ok(2)); // slice method through Deref
    assert_eq!(s.len(), 4);
    // s.push(0);      // does not compile: slices have no push, so the invariant cannot be broken from outside
    // s[0] = 99;      // does not compile: no DerefMut
}

// ----------------------------------------------------------------- 11. RAII guard
struct DepthGuard<'a>(&'a Cell<u32>);
impl<'a> DepthGuard<'a> {
    fn enter(c: &'a Cell<u32>) -> Self {
        c.set(c.get() + 1);
        DepthGuard(c)
    }
}
impl Drop for DepthGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1); // runs on every exit path: return, `?`, panic unwinding
    }
}
fn recurse(depth: &Cell<u32>, n: u32) -> Result<u32, ()> {
    let _g = DepthGuard::enter(depth);
    if n == 0 {
        return Err(()); // early return still restores the counter
    }
    recurse(depth, n - 1).or(Ok(depth.get()))
}
fn demo_raii() {
    let depth = Cell::new(0);
    assert_eq!(recurse(&depth, 3), Ok(3)); // depth seen by the level above the failing call
    assert_eq!(depth.get(), 0, "guard always restores");
}

// --------------------------------------- 12. make illegal states unrepresentable
/// Bad: `{ connected: bool, session: Option<u64>, retry: Option<u8> }` allows `connected && session.is_none()`.
#[derive(Debug, PartialEq)]
enum Conn {
    Disconnected,
    Connecting { attempt: u8 },
    Connected { session: u64 },
}
impl Conn {
    fn on_event(self, ev: &str) -> Conn {
        match (self, ev) {
            (Conn::Disconnected, "dial") => Conn::Connecting { attempt: 1 },
            (Conn::Connecting { attempt }, "fail") if attempt < 3 => Conn::Connecting {
                attempt: attempt + 1,
            },
            (Conn::Connecting { .. }, "fail") => Conn::Disconnected,
            (Conn::Connecting { attempt }, "ok") => Conn::Connected {
                session: attempt as u64 * 1000,
            },
            (Conn::Connected { .. }, "drop") => Conn::Disconnected,
            (s, _) => s, // ignore events that make no sense in this state
        }
    }
}
fn demo_states() {
    let mut c = Conn::Disconnected;
    for ev in ["dial", "fail", "fail", "ok"] {
        c = c.on_event(ev);
    }
    assert_eq!(c, Conn::Connected { session: 3000 });
    assert_eq!(c.on_event("dial"), Conn::Connected { session: 3000 }); // illegal transition ignored
    assert_eq!(
        Conn::Connecting { attempt: 3 }.on_event("fail"),
        Conn::Disconnected
    );
    assert!(std::mem::size_of::<Conn>() <= 16);
}

fn main() {
    demo_builder();
    println!("builder       ok: consuming + &mut builders, validation in build()");
    demo_strategy();
    println!("strategy      ok: closure=0 bytes, Box<dyn Fn>=16 bytes");
    demo_observer();
    println!("observer      ok: Weak subscribers pruned automatically");
    demo_visitor_interpreter();
    println!("visitor/eval  ok: trait visitor and match agree on the AST");
    demo_command();
    println!("command       ok: 50 random edit sessions satisfy undo/redo invariants");
    demo_chain();
    println!("chain         ok: middleware order observable");
    demo_di();
    println!("DI            ok: fake mailer injected");
    demo_plugins();
    println!("plugins       ok: explicit registry");
    demo_newtype();
    demo_raii();
    demo_states();
    println!("newtype/RAII/states ok");
}
