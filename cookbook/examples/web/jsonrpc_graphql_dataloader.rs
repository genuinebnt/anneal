//! RECIPE 309 (web): JSON-RPC 2.0, a mini GraphQL executor, and the DataLoader fix for N+1
//! LEVEL: advanced
//! Use in: API styles stage (RPC vs query language vs REST).   Run: cargo run --release --example jsonrpc_graphql_dataloader
//!
//! What: three API styles over the same in-memory library (5 authors, 10 books), all behind a real axum server:
//!  1. JSON-RPC 2.0 on `POST /rpc`: request/response objects, notifications (no response, HTTP 204), batches, and the
//!     error codes -32700 parse, -32600 invalid request, -32601 method not found, -32602 invalid params, -32603
//!     internal. The assertions replay the examples from the specification.
//!  2. A tiny GraphQL executor on `POST /graphql`: hand-written lexer + recursive-descent parser (operations,
//!     variables with defaults, nested selection sets, arguments, aliases, `__typename`), resolvers per type/field,
//!     and a result containing EXACTLY the requested shape. Errors come back as `{"errors":[..]}` with HTTP 200 (the
//!     historic GraphQL-over-HTTP convention).
//!  3. The N+1 problem: resolving `books` once per author hits the backend 1+N times. A per-request DATALOADER
//!     collects the keys requested "at the same time", issues ONE batched call, and caches results for the rest of
//!     the request: 1+5 backend calls become 2 (and 1+5+10 become 3). Batching here waits until the scheduler is quiescent
//!     (no new key arrived during a whole polling round), the same idea as the "next tick" of JS DataLoader.
//!     The loader MUST be per request: a global cache would leak data between users and go stale.
//!  And a comparison with REST on the same data: under-fetching (1 + N round trips) and over-fetching (whole objects).
//! COMMON USES: GraphQL gateways (GitHub, Shopify), BFFs for mobile clients, JSON-RPC in Ethereum nodes / LSP / MCP / editors,
//! any resolver-based API where list items trigger per-item lookups.
//! Read: https://www.jsonrpc.org/specification , https://graphql.org/learn/queries/ ,
//! https://developer.mozilla.org/en-US/docs/Web/HTTP
#![allow(dead_code)]
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
};
use futures_util::future::join_all;
use reqwest::{Client, RequestBuilder};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

// ---------- shared helpers: real loopback servers + curl -v style transcripts ----------
// Servers can be given symbolic names so transcripts show `as.test` instead of a random port (deterministic output).
static NAMES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static QUIET: AtomicBool = AtomicBool::new(false);
fn quiet(b: bool) {
    QUIET.store(b, SeqCst);
}
fn mask(s: &str) -> String {
    let mut o = s.to_string();
    for (a, n) in NAMES.lock().unwrap().iter() {
        o = o.replace(a.as_str(), n).replace(&a.replace(':', "%3A"), n);
    }
    o
}

struct Resp {
    status: u16,
    headers: HeaderMap,
    body: String,
}
impl Resp {
    fn h(&self, n: &str) -> String {
        self.headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

fn preview(s: &str) -> String {
    let s = s.replace('\r', "\\r").replace('\n', "\\n");
    if s.chars().count() > 170 {
        format!("{}...", s.chars().take(170).collect::<String>())
    } else {
        s
    }
}

/// Send a request and print a `curl -v` style transcript (Date omitted, Host omitted, ports masked: deterministic).
async fn call(c: &Client, rb: RequestBuilder, note: &str) -> Resp {
    try_call(c, rb, note).await.expect("request failed")
}

/// Like `call`, but a transport error (timeout, reset, truncated body) is returned and printed instead of panicking.
async fn try_call(c: &Client, rb: RequestBuilder, note: &str) -> Result<Resp, reqwest::Error> {
    let req = rb.build().unwrap();
    let show = !QUIET.load(SeqCst);
    let pr = |s: String| {
        if show {
            println!("{s}")
        }
    };
    pr(format!("\n# {note}"));
    let u = req.url();
    pr(format!(
        "> {} {}{} HTTP/1.1",
        req.method(),
        u.path(),
        u.query()
            .map(|q| mask(&format!("?{q}")))
            .unwrap_or_default()
    ));
    for (k, v) in req.headers() {
        pr(format!("> {k}: {}", mask(v.to_str().unwrap_or("<binary>"))));
    }
    if let Some(b) = req.body().and_then(|b| b.as_bytes()) {
        pr(format!(
            ">\n> {}",
            mask(&preview(&String::from_utf8_lossy(b)))
        ));
    }
    let r = match c.execute(req).await {
        Ok(r) => r,
        Err(e) => {
            pr(format!("< (no response: {})", err_kind(&e)));
            return Err(e);
        }
    };
    pr(format!("< HTTP/1.1 {}", r.status()));
    let status = r.status().as_u16();
    let headers = r.headers().clone();
    let mut hs: Vec<_> = headers
        .iter()
        .filter(|(k, _)| k.as_str() != "date")
        .collect();
    hs.sort_by_key(|(k, _)| k.as_str().to_string());
    for (k, v) in hs {
        pr(format!("< {k}: {}", mask(v.to_str().unwrap_or("<binary>"))));
    }
    let body = match r.text().await {
        Ok(b) => b,
        Err(e) => {
            pr(format!("< (body cut off: {})", err_kind(&e)));
            return Err(e);
        }
    };
    if !body.is_empty() {
        pr(format!("<\n< {}", mask(&preview(&body))));
    }
    Ok(Resp {
        status,
        headers,
        body,
    })
}
fn err_kind(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect failed"
    } else if e.is_body() || e.is_decode() {
        "incomplete body"
    } else {
        "connection closed/reset"
    }
}

struct Srv {
    addr: SocketAddr,
    tx: Option<oneshot::Sender<()>>,
    h: JoinHandle<()>,
}
impl Srv {
    async fn start_named(name: &str, app: Router) -> Srv {
        let s = Srv::start(app).await;
        NAMES
            .lock()
            .unwrap()
            .push((s.addr.to_string(), name.to_string()));
        s
    }
    async fn start(app: Router) -> Srv {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let h = tokio::spawn(async move {
            axum::serve(l, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        Srv {
            addr,
            tx: Some(tx),
            h,
        }
    }
    fn url(&self, p: &str) -> String {
        format!("http://{}{}", self.addr, p)
    }
    async fn stop(mut self) {
        if let Some(t) = self.tx.take() {
            let _ = t.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), self.h).await;
    }
}

// ================================================================ data + backend (counts every call)
#[derive(Clone, Debug)]
struct Author {
    id: u64,
    name: String,
    born: u32,
    bio: String,
}
#[derive(Clone, Debug)]
struct Book {
    id: u64,
    title: String,
    year: u32,
    author_id: u64,
    pages: u32,
    summary: String,
}
struct Data {
    authors: Vec<Author>,
    books: Vec<Book>,
}
fn make_data() -> Data {
    let names = [
        "Ada Lovelace",
        "Alan Turing",
        "Grace Hopper",
        "Edsger Dijkstra",
        "Barbara Liskov",
    ];
    let authors = names
        .iter()
        .enumerate()
        .map(|(i, n)| Author {
            id: i as u64 + 1,
            name: n.to_string(),
            born: 1815 + i as u32 * 30,
            bio: format!(
                "{n} was a pioneer of computing. {}",
                "Lorem ipsum dolor sit amet. ".repeat(6)
            ),
        })
        .collect();
    let books = (0..10u64)
        .map(|i| Book {
            id: i + 1,
            title: format!("Book {}", i + 1),
            year: 1930 + i as u32 * 5,
            author_id: i % 5 + 1,
            pages: 100 + i as u32 * 10,
            summary: format!(
                "A long summary of book {}. {}",
                i + 1,
                "More words here. ".repeat(8)
            ),
        })
        .collect();
    Data { authors, books }
}

/// One per request, like a DB session: counts the round trips the resolvers cause.
struct Backend {
    d: Arc<Data>,
    calls: AtomicU32,
    log: Mutex<Vec<String>>,
}
impl Backend {
    fn hit(&self, sql: String) {
        self.calls.fetch_add(1, SeqCst);
        self.log.lock().unwrap().push(sql);
    }
    fn all_authors(&self) -> Vec<Author> {
        self.hit("SELECT * FROM authors".into());
        self.d.authors.clone()
    }
    fn author(&self, id: u64) -> Option<Author> {
        self.hit(format!("SELECT * FROM authors WHERE id = {id}"));
        self.d.authors.iter().find(|a| a.id == id).cloned()
    }
    fn books_for(&self, aid: u64) -> Vec<Book> {
        self.hit(format!("SELECT * FROM books WHERE author_id = {aid}"));
        self.d
            .books
            .iter()
            .filter(|b| b.author_id == aid)
            .cloned()
            .collect()
    }
    fn books_for_many(&self, ids: &[u64]) -> HashMap<u64, Vec<Book>> {
        self.hit(format!("SELECT * FROM books WHERE author_id IN {ids:?}"));
        ids.iter()
            .map(|i| {
                (
                    *i,
                    self.d
                        .books
                        .iter()
                        .filter(|b| b.author_id == *i)
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    }
    fn authors_many(&self, ids: &[u64]) -> HashMap<u64, Option<Author>> {
        self.hit(format!("SELECT * FROM authors WHERE id IN {ids:?}"));
        ids.iter()
            .map(|i| (*i, self.d.authors.iter().find(|a| a.id == *i).cloned()))
            .collect()
    }
}

// ================================================================ the DataLoader
struct LState<V> {
    cache: HashMap<u64, V>,
    pending: Vec<u64>,
    dispatching: bool,
}
struct Loader<V: Clone> {
    f: Box<dyn Fn(&[u64]) -> HashMap<u64, V> + Send + Sync>,
    st: Mutex<LState<V>>,
    batches: AtomicU32,
    cache_hits: AtomicU32,
}
impl<V: Clone> Loader<V> {
    fn new(f: impl Fn(&[u64]) -> HashMap<u64, V> + Send + Sync + 'static) -> Self {
        Loader {
            f: Box::new(f),
            st: Mutex::new(LState {
                cache: HashMap::new(),
                pending: vec![],
                dispatching: false,
            }),
            batches: AtomicU32::new(0),
            cache_hits: AtomicU32::new(0),
        }
    }
    /// Callers just ask for ONE key. Keys requested by sibling resolvers are gathered and fetched together.
    async fn load(&self, k: u64) -> V {
        let mut registered = false;
        loop {
            let become_dispatcher = {
                let mut s = self.st.lock().unwrap();
                if let Some(v) = s.cache.get(&k) {
                    if !registered {
                        self.cache_hits.fetch_add(1, SeqCst);
                    }
                    return v.clone();
                }
                if !registered {
                    if !s.pending.contains(&k) {
                        s.pending.push(k);
                    }
                    registered = true;
                }
                if s.dispatching {
                    false
                } else {
                    s.dispatching = true;
                    true
                }
            };
            if become_dispatcher {
                // wait until a whole scheduling round passes without new keys: every sibling has had its say
                let mut seen = usize::MAX;
                loop {
                    tokio::task::yield_now().await;
                    let n = self.st.lock().unwrap().pending.len();
                    if n == seen {
                        break;
                    }
                    seen = n;
                }
                let batch = std::mem::take(&mut self.st.lock().unwrap().pending);
                let res = (self.f)(&batch); // ONE backend call for the whole batch
                self.batches.fetch_add(1, SeqCst);
                let mut s = self.st.lock().unwrap();
                s.cache.extend(res);
                s.dispatching = false;
            } else {
                tokio::task::yield_now().await;
            }
        }
    }
}

// ================================================================ GraphQL: lexer + parser
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Punct(char),
    Name(String),
    Int(i64),
    Str(String),
    Var(String),
}
fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = src.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
        } else if c == '#' {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else if "{}():[]!=".contains(c) {
            out.push(Tok::Punct(c));
            i += 1;
        } else if c == '$' || c.is_alphabetic() || c == '_' {
            let var = c == '$';
            let s = if var { i + 1 } else { i };
            i = s;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            let w: String = cs[s..i].iter().collect();
            out.push(if var { Tok::Var(w) } else { Tok::Name(w) });
        } else if c.is_ascii_digit() || c == '-' {
            let s = i;
            i += 1;
            while i < cs.len() && cs[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Tok::Int(
                cs[s..i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .map_err(|_| "bad number".to_string())?,
            ));
        } else if c == '"' {
            let mut s = String::new();
            i += 1;
            while i < cs.len() && cs[i] != '"' {
                if cs[i] == '\\' {
                    i += 1;
                }
                s.push(cs[i]);
                i += 1;
            }
            if i >= cs.len() {
                return Err("Syntax Error: unterminated string".into());
            }
            i += 1;
            out.push(Tok::Str(s));
        } else {
            return Err(format!("Syntax Error: unexpected character `{c}`"));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
enum Arg {
    Lit(Value),
    Var(String),
}
#[derive(Debug, Clone)]
struct Field {
    alias: Option<String>,
    name: String,
    args: Vec<(String, Arg)>,
    sel: Vec<Field>,
}
#[derive(Debug)]
struct Doc {
    name: Option<String>,
    defaults: HashMap<String, Value>,
    sel: Vec<Field>,
}

struct P {
    t: Vec<Tok>,
    i: usize,
}
impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        self.i += 1;
        self.t.get(self.i - 1).cloned()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Punct(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!(
                "Syntax Error: expected `{c}`, found {:?}",
                self.peek()
            ))
        }
    }
    fn name(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Tok::Name(n)) => Ok(n),
            o => Err(format!("Syntax Error: expected a name, found {o:?}")),
        }
    }
    fn value(&mut self) -> Result<Arg, String> {
        match self.next() {
            Some(Tok::Var(v)) => Ok(Arg::Var(v)),
            Some(Tok::Int(n)) => Ok(Arg::Lit(json!(n))),
            Some(Tok::Str(s)) => Ok(Arg::Lit(json!(s))),
            Some(Tok::Name(n)) => Ok(Arg::Lit(match n.as_str() {
                "true" => json!(true),
                "false" => json!(false),
                "null" => Value::Null,
                o => json!(o),
            })),
            Some(Tok::Punct('[')) => {
                let mut v = vec![];
                while !self.eat(']') {
                    if let Arg::Lit(x) = self.value()? {
                        v.push(x)
                    } else {
                        return Err("variables inside lists are not supported".into());
                    }
                }
                Ok(Arg::Lit(Value::Array(v)))
            }
            o => Err(format!("Syntax Error: unexpected {o:?} in value")),
        }
    }
    fn selection(&mut self) -> Result<Vec<Field>, String> {
        self.expect('{')?;
        let mut fs = vec![];
        while !self.eat('}') {
            if self.peek().is_none() {
                return Err("Syntax Error: unexpected end of document, missing `}`".into());
            }
            let first = self.name()?;
            let (alias, name) = if self.eat(':') {
                (Some(first), self.name()?)
            } else {
                (None, first)
            };
            let mut args = vec![];
            if self.eat('(') {
                while !self.eat(')') {
                    let k = self.name()?;
                    self.expect(':')?;
                    args.push((k, self.value()?));
                }
            }
            let sel = if self.peek() == Some(&Tok::Punct('{')) {
                self.selection()?
            } else {
                vec![]
            };
            fs.push(Field {
                alias,
                name,
                args,
                sel,
            });
        }
        if fs.is_empty() {
            return Err("Syntax Error: empty selection set".into());
        }
        Ok(fs)
    }
    fn document(&mut self) -> Result<Doc, String> {
        let (mut name, mut defaults) = (None, HashMap::new());
        if matches!(self.peek(), Some(Tok::Name(n)) if n == "query") {
            self.i += 1;
            if let Some(Tok::Name(_)) = self.peek() {
                name = Some(self.name()?);
            }
            if self.eat('(') {
                while !self.eat(')') {
                    let Some(Tok::Var(v)) = self.next() else {
                        return Err("Syntax Error: expected $variable".into());
                    };
                    self.expect(':')?;
                    while !matches!(
                        self.peek(),
                        Some(Tok::Punct('=')) | Some(Tok::Var(_)) | Some(Tok::Punct(')'))
                    ) {
                        self.i += 1;
                    } // skip the type (Int, Int!, [Int!]!)
                    if self.eat('=') {
                        if let Arg::Lit(d) = self.value()? {
                            defaults.insert(v, d);
                        }
                    }
                }
            }
        }
        let sel = self.selection()?;
        if self.peek().is_some() {
            return Err("Syntax Error: unexpected tokens after the selection set".into());
        }
        Ok(Doc {
            name,
            defaults,
            sel,
        })
    }
}
fn parse(src: &str) -> Result<Doc, String> {
    P { t: lex(src)?, i: 0 }.document()
}

// ================================================================ GraphQL: executor
#[derive(Clone)]
enum Node {
    Root,
    Author(Author),
    Book(Book),
}
struct Ctx {
    be: Arc<Backend>,
    vars: HashMap<String, Value>,
    use_loader: bool,
    books: Loader<Vec<Book>>,
    authors: Loader<Option<Author>>,
}
type Fut<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

fn arg(cx: &Ctx, f: &Field, k: &str) -> Option<Value> {
    f.args.iter().find(|(n, _)| n == k).map(|(_, a)| match a {
        Arg::Lit(v) => v.clone(),
        Arg::Var(n) => cx.vars.get(n).cloned().unwrap_or(Value::Null),
    })
}
fn int_arg(cx: &Ctx, f: &Field, k: &str) -> Result<Option<u64>, String> {
    match arg(cx, f, k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or(format!("Argument \"{k}\" of \"{}\" must be an Int", f.name)),
    }
}
fn exec<'a>(cx: &'a Ctx, node: Node, sel: &'a [Field]) -> Fut<'a> {
    Box::pin(async move {
        let mut out = Map::new();
        for f in sel {
            out.insert(
                f.alias.clone().unwrap_or(f.name.clone()),
                resolve(cx, &node, f).await?,
            );
        }
        Ok(Value::Object(out))
    })
}
async fn resolve_list(cx: &Ctx, nodes: Vec<Node>, f: &Field) -> Result<Value, String> {
    if f.sel.is_empty() {
        return Err(format!(
            "Field \"{}\" must have a selection of subfields",
            f.name
        ));
    }
    // all list items are resolved CONCURRENTLY: that is what lets a DataLoader see all their keys at once
    let items = join_all(nodes.into_iter().map(|n| exec(cx, n, &f.sel))).await;
    Ok(Value::Array(
        items.into_iter().collect::<Result<Vec<_>, _>>()?,
    ))
}
fn scalar(f: &Field, v: Value) -> Result<Value, String> {
    if !f.sel.is_empty() {
        return Err(format!(
            "Field \"{}\" must not have a selection (it is a scalar)",
            f.name
        ));
    }
    Ok(v)
}
async fn resolve(cx: &Ctx, node: &Node, f: &Field) -> Result<Value, String> {
    let tname = match node {
        Node::Root => "Query",
        Node::Author(_) => "Author",
        Node::Book(_) => "Book",
    };
    if f.name == "__typename" {
        return Ok(json!(tname));
    }
    match (node, f.name.as_str()) {
        (Node::Root, "authors") => {
            let mut all = cx.be.all_authors();
            if let Some(n) = int_arg(cx, f, "first")? {
                all.truncate(n as usize);
            }
            resolve_list(cx, all.into_iter().map(Node::Author).collect(), f).await
        }
        (Node::Root, "author") => {
            let id = int_arg(cx, f, "id")?
                .ok_or("Field \"author\" argument \"id\" of type \"Int!\" is required")?;
            match cx.be.author(id) {
                Some(a) => exec(cx, Node::Author(a), &f.sel).await,
                None => Ok(Value::Null),
            }
        }
        (Node::Author(a), "id") => scalar(f, json!(a.id)),
        (Node::Author(a), "name") => scalar(f, json!(a.name)),
        (Node::Author(a), "born") => scalar(f, json!(a.born)),
        (Node::Author(a), "bio") => scalar(f, json!(a.bio)),
        (Node::Author(a), "books") => {
            let mut bs = if cx.use_loader {
                cx.books.load(a.id).await
            } else {
                cx.be.books_for(a.id)
            }; // <- the N+1 line
            if let Some(l) = int_arg(cx, f, "limit")? {
                bs.truncate(l as usize);
            }
            resolve_list(cx, bs.into_iter().map(Node::Book).collect(), f).await
        }
        (Node::Book(b), "id") => scalar(f, json!(b.id)),
        (Node::Book(b), "title") => scalar(f, json!(b.title)),
        (Node::Book(b), "year") => scalar(f, json!(b.year)),
        (Node::Book(b), "pages") => scalar(f, json!(b.pages)),
        (Node::Book(b), "summary") => scalar(f, json!(b.summary)),
        (Node::Book(b), "author") => {
            let a = if cx.use_loader {
                cx.authors.load(b.author_id).await
            } else {
                cx.be.author(b.author_id)
            };
            match a {
                Some(a) => exec(cx, Node::Author(a), &f.sel).await,
                None => Ok(Value::Null),
            }
        }
        _ => Err(format!(
            "Cannot query field \"{}\" on type \"{tname}\"",
            f.name
        )),
    }
}

async fn run_query(d: Arc<Data>, query: &str, variables: &Value, use_loader: bool) -> Value {
    let doc = match parse(query) {
        Ok(d) => d,
        Err(e) => return json!({"errors": [{"message": e}]}),
    };
    let be = Arc::new(Backend {
        d,
        calls: AtomicU32::new(0),
        log: Mutex::new(vec![]),
    });
    let mut vars = doc.defaults.clone();
    if let Some(o) = variables.as_object() {
        for (k, v) in o {
            vars.insert(k.clone(), v.clone());
        }
    }
    let (b1, b2) = (be.clone(), be.clone());
    let cx = Ctx {
        be: be.clone(),
        vars,
        use_loader, // loaders are created PER REQUEST: no cross-request cache
        books: Loader::new(move |ids| b1.books_for_many(ids)),
        authors: Loader::new(move |ids| b2.authors_many(ids)),
    };
    let res = exec(&cx, Node::Root, &doc.sel).await;
    let ext = json!({"backend_calls": be.calls.load(SeqCst), "loader_batches": cx.books.batches.load(SeqCst) + cx.authors.batches.load(SeqCst), "operation": doc.name});
    match res {
        Ok(data) => json!({"data": data, "extensions": ext}),
        Err(e) => json!({"data": null, "errors": [{"message": e}], "extensions": ext}),
    }
}

async fn graphql(State(d): State<Arc<Data>>, headers: HeaderMap, body: Bytes) -> Json<Value> {
    let Ok(req) = serde_json::from_slice::<Value>(&body) else {
        return Json(json!({"errors": [{"message": "body must be JSON"}]}));
    };
    let use_loader = headers.get("x-dataloader").and_then(|v| v.to_str().ok()) != Some("off");
    Json(
        run_query(
            d,
            req["query"].as_str().unwrap_or(""),
            &req["variables"],
            use_loader,
        )
        .await,
    )
}

// ================================================================ REST over the same data (for the comparison)
async fn rest_authors(State(d): State<Arc<Data>>) -> Json<Value> {
    Json(Value::Array(
        d.authors
            .iter()
            .map(|a| json!({"id": a.id, "name": a.name, "born": a.born, "bio": a.bio}))
            .collect(),
    ))
}
async fn rest_author_books(State(d): State<Arc<Data>>, Path(id): Path<u64>) -> Json<Value> {
    Json(Value::Array(d.books.iter().filter(|b| b.author_id == id).map(|b| json!({"id": b.id, "title": b.title, "year": b.year, "pages": b.pages, "summary": b.summary, "author_id": b.author_id})).collect()))
}

// ================================================================ JSON-RPC 2.0
fn rpc_err(id: Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "error": {"code": code, "message": msg}, "id": id})
}
fn dispatch(method: &str, params: Option<&Value>) -> Result<Value, (i64, &'static str)> {
    let nums = |p: Option<&Value>| -> Option<Vec<f64>> {
        p?.as_array()?.iter().map(Value::as_f64).collect()
    };
    match method {
        "subtract" => {
            let (a, b) = match params {
                Some(Value::Array(_)) => {
                    let n = nums(params).ok_or((-32602, "Invalid params"))?;
                    if n.len() != 2 {
                        return Err((-32602, "Invalid params"));
                    }
                    (n[0], n[1])
                }
                Some(Value::Object(o)) => (
                    o.get("minuend")
                        .and_then(Value::as_f64)
                        .ok_or((-32602, "Invalid params"))?,
                    o.get("subtrahend")
                        .and_then(Value::as_f64)
                        .ok_or((-32602, "Invalid params"))?,
                ),
                _ => return Err((-32602, "Invalid params")),
            };
            Ok(json!((a - b) as i64))
        }
        "sum" => Ok(json!(
            nums(params)
                .ok_or((-32602, "Invalid params"))?
                .iter()
                .sum::<f64>() as i64
        )),
        "get_data" => Ok(json!(["hello", 5])),
        "update" | "notify_hello" | "notify_sum" => Ok(Value::Null),
        "crash" => Err((-32603, "Internal error")),
        _ => Err((-32601, "Method not found")),
    }
}
fn handle_one(v: &Value) -> Option<Value> {
    let Some(o) = v.as_object() else {
        return Some(rpc_err(Value::Null, -32600, "Invalid Request"));
    };
    let id = o.get("id").cloned();
    let id_ok = matches!(
        id,
        None | Some(Value::Null) | Some(Value::String(_)) | Some(Value::Number(_))
    );
    let params_ok = matches!(
        o.get("params"),
        None | Some(Value::Array(_)) | Some(Value::Object(_))
    );
    let method = o.get("method").and_then(Value::as_str);
    if o.get("jsonrpc") != Some(&json!("2.0")) || method.is_none() || !params_ok || !id_ok {
        return Some(rpc_err(
            if id_ok {
                id.unwrap_or(Value::Null)
            } else {
                Value::Null
            },
            -32600,
            "Invalid Request",
        ));
    }
    let res = dispatch(method.unwrap(), o.get("params"));
    let id = id?; // no "id" member => notification => no response, even on error
    Some(match res {
        Ok(r) => json!({"jsonrpc": "2.0", "result": r, "id": id}),
        Err((c, m)) => rpc_err(id, c, m),
    })
}
async fn rpc(body: Bytes) -> Response {
    let reply = match serde_json::from_slice::<Value>(&body) {
        Err(_) => Some(rpc_err(Value::Null, -32700, "Parse error")),
        Ok(Value::Array(a)) if a.is_empty() => {
            Some(rpc_err(Value::Null, -32600, "Invalid Request"))
        }
        Ok(Value::Array(a)) => {
            let r: Vec<Value> = a.iter().filter_map(handle_one).collect();
            if r.is_empty() {
                None
            } else {
                Some(Value::Array(r))
            }
        }
        Ok(v) => handle_one(&v),
    };
    match reply {
        Some(v) => Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => Response::builder().status(204).body(Body::empty()).unwrap(), // nothing to say: notifications only
    }
}

async fn gql(c: &Client, srv: &Srv, q: &str, vars: Value, loader: bool, note: &str) -> Value {
    let mut rb = c
        .post(srv.url("/graphql"))
        .json(&json!({"query": q, "variables": vars}));
    if !loader {
        rb = rb.header("x-dataloader", "off");
    }
    call(c, rb, note).await.json()
}

#[tokio::main]
async fn main() {
    let data = Arc::new(make_data());
    let app = Router::new()
        .route("/rpc", post(rpc))
        .route("/graphql", post(graphql))
        .route("/rest/authors", get(rest_authors))
        .route("/rest/authors/{id}/books", get(rest_author_books))
        .with_state(data.clone());
    let srv = Srv::start(app).await;
    let c = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    println!("==== 1. JSON-RPC 2.0: the examples from the specification ====");
    let rpc_call = |body: &str, note: &str| {
        let (c, url, body) = (c.clone(), srv.url("/rpc"), body.to_string());
        let note = note.to_string();
        async move {
            call(
                &c,
                c.post(url)
                    .header("content-type", "application/json")
                    .body(body),
                &note,
            )
            .await
        }
    };
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1}"#,
        "positional params",
    )
    .await;
    assert_eq!(r.json(), json!({"jsonrpc": "2.0", "result": 19, "id": 1}));
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "subtract", "params": [23, 42], "id": 2}"#,
        "positional params, swapped",
    )
    .await;
    assert_eq!(r.json(), json!({"jsonrpc": "2.0", "result": -19, "id": 2}));
    let r = rpc_call(r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 3}"#, "named params").await;
    assert_eq!(r.json(), json!({"jsonrpc": "2.0", "result": 19, "id": 3}));
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "update", "params": [1,2,3,4,5]}"#,
        "notification: no id => no response body",
    )
    .await;
    assert_eq!((r.status, r.body.len()), (204, 0));
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "foobar", "id": "1"}"#,
        "method not found",
    )
    .await;
    assert_eq!(
        r.json(),
        json!({"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": "1"})
    );
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "foobar, "params": "bar", "baz]"#,
        "invalid JSON",
    )
    .await;
    assert_eq!(
        r.json(),
        json!({"jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error"}, "id": null})
    );
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": 1, "params": "bar"}"#,
        "invalid Request object",
    )
    .await;
    assert_eq!(
        r.json(),
        json!({"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}, "id": null})
    );
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "subtract", "params": [1], "id": 4}"#,
        "invalid params",
    )
    .await;
    assert_eq!(r.json()["error"]["code"], -32602);
    let r = rpc_call(
        r#"{"jsonrpc": "2.0", "method": "crash", "id": 5}"#,
        "internal error",
    )
    .await;
    assert_eq!(r.json()["error"]["code"], -32603);
    let r = rpc_call(r#"[{"jsonrpc": "2.0", "method": "sum", "params": [1,2,4], "id": "1"}, {"jsonrpc": "2.0", "method"]"#, "batch with invalid JSON").await;
    assert_eq!(r.json()["error"]["code"], -32700);
    let r = rpc_call("[]", "empty batch").await;
    assert_eq!(
        r.json(),
        json!({"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}, "id": null})
    );
    let r = rpc_call("[1]", "batch with one invalid element").await;
    assert_eq!(
        r.json(),
        json!([{"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}, "id": null}])
    );
    let r = rpc_call("[1,2,3]", "batch with three invalid elements").await;
    assert_eq!(r.json().as_array().unwrap().len(), 3);
    let batch = r#"[
        {"jsonrpc": "2.0", "method": "sum", "params": [1,2,4], "id": "1"},
        {"jsonrpc": "2.0", "method": "notify_hello", "params": [7]},
        {"jsonrpc": "2.0", "method": "subtract", "params": [42,23], "id": "2"},
        {"foo": "boo"},
        {"jsonrpc": "2.0", "method": "foo.get", "params": {"name": "myself"}, "id": "5"},
        {"jsonrpc": "2.0", "method": "get_data", "id": "9"}]"#;
    let r = rpc_call(
        batch,
        "the spec's mixed batch: 6 requests, 1 notification => 5 responses",
    )
    .await;
    assert_eq!(
        r.json(),
        json!([
        {"jsonrpc": "2.0", "result": 7, "id": "1"},
        {"jsonrpc": "2.0", "result": 19, "id": "2"},
        {"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}, "id": null},
        {"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": "5"},
        {"jsonrpc": "2.0", "result": ["hello", 5], "id": "9"}])
    );
    let r = rpc_call(r#"[{"jsonrpc": "2.0", "method": "notify_sum", "params": [1,2,4]}, {"jsonrpc": "2.0", "method": "notify_hello", "params": [7]}]"#, "batch of only notifications").await;
    assert_eq!((r.status, r.body.len()), (204, 0));

    println!("\n==== 2. GraphQL: parser + exact-shape results ====");
    let d = parse("query Q($n: Int = 2, $id: Int!) { top: authors(first: $n) { id } one: author(id: $id) { name titles: books(limit: 1) { title } } }").unwrap();
    assert_eq!(
        (d.name.as_deref(), d.sel.len(), d.defaults["n"].clone()),
        (Some("Q"), 2, json!(2))
    );
    assert_eq!(d.sel[1].sel[1].alias.as_deref(), Some("titles"));
    assert_eq!(lex("{ a(x: \"hi\") } # comment").unwrap().len(), 8);
    let q = "query Q($n: Int = 2, $id: Int!) {\n  top: authors(first: $n) { id __typename }\n  one: author(id: $id) { name titles: books(limit: 1) { title year } }\n}";
    let r = gql(
        &c,
        &srv,
        q,
        json!({"id": 3}),
        true,
        "aliases, arguments, variables (default $n = 2), nested selection",
    )
    .await;
    assert_eq!(
        r["data"],
        json!({"top": [{"id": 1, "__typename": "Author"}, {"id": 2, "__typename": "Author"}], "one": {"name": "Grace Hopper", "titles": [{"title": "Book 3", "year": 1940}]}})
    );
    let r = gql(
        &c,
        &srv,
        q,
        json!({"id": 9, "n": 1}),
        true,
        "variables override the default; unknown author => null",
    )
    .await;
    assert_eq!(
        r["data"],
        json!({"top": [{"id": 1, "__typename": "Author"}], "one": null})
    );
    let r = gql(
        &c,
        &srv,
        "{ authors { name nope } }",
        json!({}),
        true,
        "unknown field => errors, HTTP 200",
    )
    .await;
    assert_eq!(
        r["errors"][0]["message"],
        "Cannot query field \"nope\" on type \"Author\""
    );
    let r = gql(
        &c,
        &srv,
        "{ authors { name ",
        json!({}),
        true,
        "syntax error",
    )
    .await;
    assert!(
        r["errors"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("Syntax Error")
    );
    let r = gql(
        &c,
        &srv,
        "{ authors { name { x } } }",
        json!({}),
        true,
        "selection on a scalar",
    )
    .await;
    assert!(
        r["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("scalar")
    );

    println!("\n==== 3. the N+1 problem and the DataLoader fix ====");
    let q1 = "{ authors { name books { title } } }";
    let naive = gql(
        &c,
        &srv,
        q1,
        json!({}),
        false,
        "WITHOUT dataloader: books resolved once per author",
    )
    .await;
    let fixed = gql(
        &c,
        &srv,
        q1,
        json!({}),
        true,
        "WITH dataloader: one batched call",
    )
    .await;
    println!(
        "\n   backend calls: naive = {}, dataloader = {}",
        naive["extensions"]["backend_calls"], fixed["extensions"]["backend_calls"]
    );
    assert_eq!(naive["extensions"]["backend_calls"], 6, "1 + N with N = 5");
    assert_eq!(
        fixed["extensions"]["backend_calls"], 2,
        "authors + ONE batched books query"
    );
    assert_eq!(
        naive["data"], fixed["data"],
        "same answer, fewer round trips"
    );
    assert_eq!(
        fixed["data"]["authors"][0],
        json!({"name": "Ada Lovelace", "books": [{"title": "Book 1"}, {"title": "Book 6"}]})
    );
    // what the backend actually saw
    let be = Arc::new(Backend {
        d: data.clone(),
        calls: AtomicU32::new(0),
        log: Mutex::new(vec![]),
    });
    let _ = be.books_for_many(&[1, 2, 3, 4, 5]);
    println!("   one batched statement: {}", be.log.lock().unwrap()[0]);
    let q3 = "{ authors { name books { title author { name } } } }";
    let naive = gql(&c, &srv, q3, json!({}), false, "three levels, naive").await;
    let fixed = gql(&c, &srv, q3, json!({}), true, "three levels, dataloaders").await;
    println!(
        "\n   three levels: naive = {} calls, dataloader = {} calls",
        naive["extensions"]["backend_calls"], fixed["extensions"]["backend_calls"]
    );
    assert_eq!(naive["extensions"]["backend_calls"], 16, "1 + 5 + 10");
    assert_eq!(
        fixed["extensions"]["backend_calls"], 3,
        "authors + books batch + authors batch (5 distinct ids for 10 books)"
    );
    assert_eq!(naive["data"], fixed["data"]);
    // per-request cache: the same key twice in one request costs one load
    let dup = gql(
        &c,
        &srv,
        "{ a: author(id: 1) { books { title } } b: author(id: 1) { books { title } } }",
        json!({}),
        true,
        "same author twice: the loader cache answers the second",
    )
    .await;
    assert_eq!(
        dup["extensions"]["backend_calls"], 3,
        "author(1) x2 + books batch x1 (naive would be 4)"
    );
    assert_eq!(dup["extensions"]["loader_batches"], 1);
    let again = gql(
        &c,
        &srv,
        "{ author(id: 1) { books { title } } }",
        json!({}),
        true,
        "a NEW request starts with a cold cache (per-request loaders)",
    )
    .await;
    assert_eq!(again["extensions"]["backend_calls"], 2);

    println!("\n==== 4. REST vs GraphQL on the same data: names + book titles ====");
    quiet(true);
    let (mut reqs, mut rest_bytes) = (0, 0);
    let authors = call(&c, c.get(srv.url("/rest/authors")), "").await;
    reqs += 1;
    rest_bytes += authors.body.len();
    let mut titles = vec![];
    for a in authors.json().as_array().unwrap() {
        let b = call(
            &c,
            c.get(srv.url(&format!("/rest/authors/{}/books", a["id"]))),
            "",
        )
        .await; // under-fetching: one more round trip per author
        reqs += 1;
        rest_bytes += b.body.len();
        titles.push(json!({"name": a["name"], "books": b.json().as_array().unwrap().iter().map(|x| json!({"title": x["title"]})).collect::<Vec<_>>()}));
    }
    let g = call(
        &c,
        c.post(srv.url("/graphql")).json(&json!({"query": q1})),
        "",
    )
    .await;
    quiet(false);
    let gql_bytes = g.body.len();
    println!(
        "   REST:    {reqs} HTTP requests, {rest_bytes} bytes (full author bios and book summaries we never use: OVER-fetching)"
    );
    println!("   GraphQL: 1 HTTP request,  {gql_bytes} bytes (exactly the requested fields)");
    assert_eq!(reqs, 6);
    assert_eq!(
        Value::Array(titles),
        g.json()["data"]["authors"],
        "identical information"
    );
    assert!(gql_bytes * 4 < rest_bytes);
    println!(
        "   (REST can narrow this with `?fields=` / `?expand=books` / a purpose-built endpoint: GraphQL moves that choice to the client,"
    );
    println!(
        "    at the price of query-cost limits, no HTTP caching by URL, and N+1 risks that the DataLoader above has to remove.)"
    );

    srv.stop().await;
    println!("\nOK: all JSON-RPC / GraphQL / DataLoader assertions passed");
}
