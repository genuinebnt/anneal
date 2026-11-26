//! CAPSTONE 15 (database): a mini Redis: RESP2 over TCP, five data types, expiry, pub/sub, transactions, AOF + snapshots
//! LEVEL: capstone
//! Use in: any in-memory data server, cache/queue/leaderboard backends, protocol servers with persistence; combines tokio sockets, skip lists, WAL-style logs, model tests.   Run: cargo run --release --example mini_redis
#![allow(dead_code)]
//!
//! ARCHITECTURE
//! ```text
//!   TCP clients (tokio, one task per connection)                 ONE executor thread (std::thread, owns ALL data)
//!  +----------------------------+  Msg::Req{cmds, oneshot}   +-----------------------------------------------------+
//!  | read -> incremental RESP   |--------------------------->| crossbeam channel -> drain queued msgs (group commit)|
//!  | parse ALL buffered cmds    |                            |   dispatch: subscriber gate / MULTI queue / execute  |
//!  | (pipelining, inline, limits|<---- BatchReply (frames) --|   Engine { BTreeMap<key,Value>, Expires, watchers,   |
//!  |  protocol error => -ERR +  |      sent AFTER the AOF     |            pub/sub tables, blocked-client FIFOs }    |
//!  |  close) -> ONE write       |      flush + fsync          |   then: append AOF records (fsync policy), reply     |
//!  |  select! { socket read,    |<--- bounded mpsc push ------|   PUBLISH -> try_send into each subscriber's queue   |
//!  |   push queue, kill, stop } |     (full queue => kill)    |   tick every 10 ms: BLPOP timeouts, active expiry,   |
//!  +----------------------------+                            |   everysec fsync;  BGSAVE/BGREWRITE on helper threads|
//!                                                            +-----------------------------------------------------+
//!   Value::{Str, List(VecDeque), Hash(BTreeMap), Set(BTreeSet), ZSet{HashMap member->score + SkipList with spans}}
//!   Persistence: appendonly.aof (RESP commands, replayed on start) and dump.mrdb (binary + CRC32); both atomic-rename.
//! ```
//! WHAT: a Redis-compatible server (~85 commands, `redis-cli` would talk to it) and a small async client in one file,
//! tested only through real loopback sockets. WHY: Redis is the best small study of how a database trades generality
//! for speed: one thread, one in-memory structure per type, a text protocol, and a log for durability. HOW:
//!  * CONCURRENCY MODEL (chosen: single-threaded executor, like real Redis). Connection tasks only do I/O and parsing; every
//!    command runs on one thread, so each command, MULTI/EXEC block and BLPOP wake-up is trivially atomic and the AOF order is the
//!    execution order. No per-key locks, no lock ordering, no torn multi-key commands (SINTER, RENAME, EXEC). The alternative,
//!    a sharded store with one lock per shard, scales single-key commands across cores (see the measured table) but makes
//!    every multi-key command and transaction a lock-ordering problem. The measurement shows both effects: a channel
//!    round trip per op costs ~20x an uncontended lock, but batching (pipelining) amortizes it, which is why Redis
//!    clients pipeline. Replies are released only after the AOF write, and queued requests share one fsync (group commit).
//!  * RESP2 parsing is incremental (`Ok(None)` = need more bytes), every length is untrusted and checked BEFORE waiting
//!    for or allocating the bytes, and a protocol error replies `-ERR Protocol error: ...` then closes.
//!  * EXPIRY: lazy (every access checks the key's deadline) plus ACTIVE: each cycle samples 20 random keys that have a TTL (a Vec
//!    + position map gives O(1) random choice), deletes the expired, and repeats while more than 25% were expired. The clock is a
//!    trait, so tests drive time with `ManualClock`. Expiry is logged as an explicit DEL so AOF replay never needs a clock.
//!  * BLOCKING POPS: a paused batch is parked with its remaining commands; waiters per key are a FIFO queue; a push wakes the
//!    oldest waiter first and the batch resumes, so pipelined replies stay in order.
//!  * PUB/SUB: subscriber mode is per-connection state; messages are pushed through a BOUNDED queue so one stuck reader can never
//!    grow server memory: overflow disconnects it.  TRANSACTIONS: WATCH registers the client on keys; any write marks it dirty; EXEC then
//!    answers nil. PERSISTENCE: AOF with always/everysec/no, background rewrite (snapshot + buffered tail), torn-tail
//!    truncation on replay, and an RDB-style snapshot with a checksum. Simplifications vs real Redis: single database, RESP2 only,
//!    no replication/eviction/Lua/ACL, BGSAVE clones instead of fork(), hash/set replies are sorted (Redis: unspecified order).
//! COMMON USES: Redis is used as a cache (GET/SET with TTL), a session store, a job/message queue (lists, BLPOP), a rate limiter
//! (INCR + EXPIRE), a leaderboard (sorted sets), and a lightweight pub/sub bus; MULTI/WATCH give optimistic transactions.
//! Read: https://redis.io/docs/latest/develop/reference/protocol-spec/ https://redis.io/docs/latest/commands/ https://redis.io/docs/latest/operate/oss_and_stack/management/persistence/ https://redis.io/docs/latest/develop/interact/pubsub/
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel as xc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tokio::task::JoinSet;

type B = Vec<u8>;

// ================================================================ 1. RESP2 frames, incremental parser, encoder

#[derive(Debug, Clone, PartialEq)]
enum Frame {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(B),
    Null,
    Array(Vec<Frame>),
    NullArray,
}

impl Frame {
    fn ok() -> Frame {
        Frame::Simple("OK".into())
    }
    fn err(s: impl Into<String>) -> Frame {
        Frame::Error(s.into())
    }
    fn bulk(b: impl AsRef<[u8]>) -> Frame {
        Frame::Bulk(b.as_ref().to_vec())
    }
    fn bulks<I: IntoIterator<Item = B>>(it: I) -> Frame {
        Frame::Array(it.into_iter().map(Frame::Bulk).collect())
    }
    fn text(&self) -> String {
        match self {
            Frame::Simple(s) | Frame::Error(s) => s.clone(),
            Frame::Bulk(b) => String::from_utf8_lossy(b).into_owned(),
            Frame::Int(n) => n.to_string(),
            other => format!("{other:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ProtoError {
    BadInteger,
    BadLength,
    BadTerminator,
    InvalidUtf8,
    TooLarge(&'static str),
    TooDeep,
    LineTooLong,
    BadType(u8),
    ExpectedBulk(u8),
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ProtoError::BadInteger => write!(f, "invalid integer"),
            ProtoError::BadLength => write!(f, "invalid length"),
            ProtoError::BadTerminator => write!(f, "expected CRLF after bulk payload"),
            ProtoError::InvalidUtf8 => write!(f, "invalid UTF-8 in a line frame"),
            ProtoError::TooLarge(w) => write!(f, "{w} exceeds the configured limit"),
            ProtoError::TooDeep => write!(f, "nesting too deep"),
            ProtoError::LineTooLong => write!(f, "too big inline request"),
            ProtoError::BadType(b) => write!(f, "unknown frame type byte {b}"),
            ProtoError::ExpectedBulk(b) => write!(f, "expected '$', got '{}'", *b as char),
        }
    }
}

/// Every length on the wire is untrusted. Real Redis allows 512 MiB bulks and 1M-element arrays; we default lower.
#[derive(Debug, Clone)]
struct Limits {
    max_line: usize,
    max_bulk: usize,
    max_array: usize,
    max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_line: 64 * 1024,
            max_bulk: 1 << 20,
            max_array: 1 << 16,
            max_depth: 8,
        }
    }
}

type Parsed = Result<Option<(Frame, usize)>, ProtoError>;

/// A CRLF-terminated line. The limit applies even before the CRLF arrives, or a peer could make us buffer forever.
fn take_line<'a>(buf: &'a [u8], lim: &Limits) -> Result<Option<(&'a [u8], usize)>, ProtoError> {
    match buf.windows(2).position(|w| w == b"\r\n") {
        Some(i) if i > lim.max_line => Err(ProtoError::LineTooLong),
        Some(i) => Ok(Some((&buf[..i], i + 2))),
        None if buf.len() > lim.max_line => Err(ProtoError::LineTooLong),
        None => Ok(None),
    }
}

/// Strict decimal: Rust's `parse` accepts a leading '+', RESP (and Redis' string2ll) does not.
fn parse_dec(b: &[u8]) -> Option<i64> {
    if b.first() == Some(&b'+') {
        return None;
    }
    std::str::from_utf8(b).ok()?.parse().ok()
}

/// Incremental parse of ONE frame at the start of `buf`: `Ok(None)` = need more bytes, `Err` = poisoned stream.
fn parse_at(buf: &[u8], depth: usize, lim: &Limits) -> Parsed {
    let Some(&first) = buf.first() else {
        return Ok(None);
    };
    if !matches!(first, b'+' | b'-' | b':' | b'$' | b'*') {
        return Err(ProtoError::BadType(first));
    }
    let Some((line, used)) = take_line(buf, lim)? else {
        return Ok(None);
    };
    let body = &line[1..];
    let text = || String::from_utf8(body.to_vec()).map_err(|_| ProtoError::InvalidUtf8);
    match first {
        b'+' => Ok(Some((Frame::Simple(text()?), used))),
        b'-' => Ok(Some((Frame::Error(text()?), used))),
        b':' => Ok(Some((
            Frame::Int(parse_dec(body).ok_or(ProtoError::BadInteger)?),
            used,
        ))),
        b'$' => {
            let n = parse_dec(body).ok_or(ProtoError::BadInteger)?;
            if n == -1 {
                return Ok(Some((Frame::Null, used)));
            }
            let n = usize::try_from(n).map_err(|_| ProtoError::BadLength)?;
            if n > lim.max_bulk {
                return Err(ProtoError::TooLarge("bulk string")); // checked BEFORE waiting for the payload
            }
            if buf.len() < used + n + 2 {
                return Ok(None);
            }
            if &buf[used + n..used + n + 2] != b"\r\n" {
                return Err(ProtoError::BadTerminator);
            }
            Ok(Some((
                Frame::Bulk(buf[used..used + n].to_vec()),
                used + n + 2,
            )))
        }
        _ => {
            let n = parse_dec(body).ok_or(ProtoError::BadInteger)?;
            if n == -1 {
                return Ok(Some((Frame::NullArray, used)));
            }
            let n = usize::try_from(n).map_err(|_| ProtoError::BadLength)?;
            if n > lim.max_array {
                return Err(ProtoError::TooLarge("array"));
            }
            if depth >= lim.max_depth {
                return Err(ProtoError::TooDeep);
            }
            let mut items = Vec::with_capacity(n.min(64)); // never allocate from a declared count
            let mut pos = used;
            for _ in 0..n {
                match parse_at(&buf[pos..], depth + 1, lim)? {
                    Some((f, c)) => {
                        items.push(f);
                        pos += c;
                    }
                    None => return Ok(None),
                }
            }
            Ok(Some((Frame::Array(items), pos)))
        }
    }
}

fn parse(buf: &[u8], lim: &Limits) -> Parsed {
    parse_at(buf, 0, lim)
}

/// Server side: a command is `*n` + n bulk strings, or an "inline" line (`PING\r\n`) as typed into telnet.
fn parse_command(buf: &[u8], lim: &Limits) -> Result<Option<(Vec<B>, usize)>, ProtoError> {
    let Some(&first) = buf.first() else {
        return Ok(None);
    };
    if first != b'*' {
        let Some(nl) = buf.iter().position(|&b| b == b'\n') else {
            return if buf.len() > lim.max_line {
                Err(ProtoError::LineTooLong)
            } else {
                Ok(None)
            };
        };
        let line = buf[..nl].strip_suffix(b"\r").unwrap_or(&buf[..nl]);
        let words = line
            .split(|b| b.is_ascii_whitespace())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_vec())
            .collect();
        return Ok(Some((words, nl + 1)));
    }
    match parse(buf, lim)? {
        None => Ok(None),
        Some((Frame::NullArray, used)) => Ok(Some((vec![], used))),
        Some((Frame::Array(items), used)) => {
            let mut args = Vec::with_capacity(items.len());
            for it in items {
                match it {
                    Frame::Bulk(b) => args.push(b),
                    Frame::Simple(_) => return Err(ProtoError::ExpectedBulk(b'+')),
                    Frame::Error(_) => return Err(ProtoError::ExpectedBulk(b'-')),
                    Frame::Int(_) => return Err(ProtoError::ExpectedBulk(b':')),
                    _ => return Err(ProtoError::ExpectedBulk(b'*')),
                }
            }
            Ok(Some((args, used)))
        }
        Some(_) => Err(ProtoError::BadType(first)),
    }
}

fn encode(frame: &Frame, out: &mut Vec<u8>) {
    let line = |out: &mut Vec<u8>, tag: u8, body: &[u8]| {
        out.push(tag);
        out.extend_from_slice(body);
        out.extend_from_slice(b"\r\n");
    };
    match frame {
        Frame::Simple(s) => line(out, b'+', s.as_bytes()),
        Frame::Error(s) => line(out, b'-', s.as_bytes()),
        Frame::Int(n) => line(out, b':', n.to_string().as_bytes()),
        Frame::Null => line(out, b'$', b"-1"),
        Frame::NullArray => line(out, b'*', b"-1"),
        Frame::Bulk(b) => {
            line(out, b'$', b.len().to_string().as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        Frame::Array(items) => {
            line(out, b'*', items.len().to_string().as_bytes());
            items.iter().for_each(|f| encode(f, out));
        }
    }
}

fn encode_cmd<S: AsRef<[u8]>>(args: &[S]) -> Vec<u8> {
    let mut out = Vec::new();
    encode(
        &Frame::Array(args.iter().map(|a| Frame::bulk(a)).collect()),
        &mut out,
    );
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &[u8]) -> Option<B> {
    if s.len() % 2 != 0 {
        return None;
    }
    s.chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

// ================================================================ 2. clock, rng, glob

/// Injectable time: the server never calls `SystemTime` directly, so tests drive expiry with a `ManualClock`.
trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

struct ManualClock(AtomicU64);
impl ManualClock {
    fn new(start: u64) -> Arc<ManualClock> {
        Arc::new(ManualClock(AtomicU64::new(start)))
    }
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, SeqCst);
    }
}
impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(SeqCst)
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
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

#[derive(Debug, Clone)]
enum Tok {
    Star,
    Any,
    Lit(u8),
    Class(bool, Vec<(u8, u8)>),
}

impl Tok {
    fn matches(&self, c: u8) -> bool {
        match self {
            Tok::Star => false,
            Tok::Any => true,
            Tok::Lit(x) => *x == c,
            Tok::Class(neg, ranges) => ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi) != *neg,
        }
    }
}

/// Redis-style glob: `*  ?  [abc]  [a-z]  [^x]  \x`. Compiled once, matched with the classic greedy-star
/// two-pointer algorithm (O(n*m) worst case, never exponential).
struct Glob(Vec<Tok>);

impl Glob {
    fn new(p: &[u8]) -> Glob {
        let (mut toks, mut i) = (Vec::new(), 0);
        while i < p.len() {
            match p[i] {
                b'*' => {
                    if !matches!(toks.last(), Some(Tok::Star)) {
                        toks.push(Tok::Star)
                    }
                }
                b'?' => toks.push(Tok::Any),
                b'\\' if i + 1 < p.len() => {
                    i += 1;
                    toks.push(Tok::Lit(p[i]));
                }
                b'[' => {
                    i += 1;
                    let neg = p.get(i) == Some(&b'^');
                    if neg {
                        i += 1;
                    }
                    let mut ranges = Vec::new();
                    while i < p.len() && p[i] != b']' {
                        let mut lo = p[i];
                        if lo == b'\\' && i + 1 < p.len() {
                            i += 1;
                            lo = p[i];
                        }
                        if p.get(i + 1) == Some(&b'-') && i + 2 < p.len() && p[i + 2] != b']' {
                            ranges.push((lo.min(p[i + 2]), lo.max(p[i + 2])));
                            i += 3;
                        } else {
                            ranges.push((lo, lo));
                            i += 1;
                        }
                    }
                    toks.push(Tok::Class(neg, ranges));
                }
                c => toks.push(Tok::Lit(c)),
            }
            i += 1;
        }
        Glob(toks)
    }

    fn is_match(&self, s: &[u8]) -> bool {
        let t = &self.0;
        let (mut ti, mut si, mut star): (usize, usize, Option<(usize, usize)>) = (0, 0, None);
        while si < s.len() {
            match t.get(ti) {
                Some(Tok::Star) => {
                    star = Some((ti, si));
                    ti += 1;
                    continue;
                }
                Some(tok) if tok.matches(s[si]) => {
                    ti += 1;
                    si += 1;
                    continue;
                }
                _ => {}
            }
            match star {
                Some((st, ss)) => {
                    ti = st + 1;
                    si = ss + 1;
                    star = Some((st, ss + 1));
                }
                None => return false,
            }
        }
        t[ti.min(t.len())..].iter().all(|x| matches!(x, Tok::Star))
    }

    /// Exponential-time reference matcher, used only by the tests to cross-check `is_match`.
    fn is_match_slow(&self, s: &[u8]) -> bool {
        fn rec(t: &[Tok], s: &[u8]) -> bool {
            match t.first() {
                None => s.is_empty(),
                Some(Tok::Star) => (0..=s.len()).any(|i| rec(&t[1..], &s[i..])),
                Some(tok) => !s.is_empty() && tok.matches(s[0]) && rec(&t[1..], &s[1..]),
            }
        }
        rec(&self.0, s)
    }
}

// ================================================================ 3. sorted set: skip list with spans (rank queries)

const NIL: usize = usize::MAX;
const MAXL: usize = 32;

#[derive(Clone)]
struct ZNode {
    member: B,
    score: f64,
    next: Vec<usize>, // next[l] = successor at level l
    span: Vec<usize>, // span[l] = how many level-0 steps that link jumps over (this is what makes rank O(log n))
}

/// Arena skip list ordered by (score, member). Node 0 is the head sentinel. Same design as Redis' `zskiplist`:
/// `span` lets `rank` and `get_by_rank` sum jump lengths on the way down instead of walking level 0.
#[derive(Clone)]
struct SkipList {
    nodes: Vec<ZNode>,
    free: Vec<usize>,
    level: usize,
    len: usize,
    rng: u64,
}

impl Default for SkipList {
    fn default() -> Self {
        let head = ZNode {
            member: vec![],
            score: 0.0,
            next: vec![NIL; MAXL],
            span: vec![0; MAXL],
        };
        SkipList {
            nodes: vec![head],
            free: vec![],
            level: 1,
            len: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl SkipList {
    fn random_level(&mut self) -> usize {
        let mut l = 1;
        while l < MAXL {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            if (self.rng >> 33) & 3 != 0 {
                break; // promote with p = 1/4, Redis' ZSKIPLIST_P
            }
            l += 1;
        }
        l
    }

    fn before(&self, i: usize, score: f64, member: &[u8]) -> bool {
        let n = &self.nodes[i];
        n.score < score || (n.score == score && n.member.as_slice() < member)
    }

    fn insert(&mut self, score: f64, member: B) {
        let (mut update, mut rank, mut x) = ([0usize; MAXL], [0usize; MAXL], 0usize);
        for i in (0..self.level).rev() {
            rank[i] = if i == self.level - 1 { 0 } else { rank[i + 1] };
            loop {
                let nx = self.nodes[x].next[i];
                if nx != NIL && self.before(nx, score, &member) {
                    rank[i] += self.nodes[x].span[i];
                    x = nx;
                } else {
                    break;
                }
            }
            update[i] = x;
        }
        let lvl = self.random_level();
        if lvl > self.level {
            for i in self.level..lvl {
                rank[i] = 0;
                update[i] = 0;
                self.nodes[0].span[i] = self.len;
            }
            self.level = lvl;
        }
        let node = ZNode {
            member,
            score,
            next: vec![NIL; lvl],
            span: vec![0; lvl],
        };
        let idx = match self.free.pop() {
            Some(f) => {
                self.nodes[f] = node;
                f
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        for i in 0..lvl {
            let u = update[i];
            self.nodes[idx].next[i] = self.nodes[u].next[i];
            self.nodes[u].next[i] = idx;
            self.nodes[idx].span[i] = self.nodes[u].span[i] - (rank[0] - rank[i]);
            self.nodes[u].span[i] = (rank[0] - rank[i]) + 1;
        }
        for i in lvl..self.level {
            self.nodes[update[i]].span[i] += 1; // lanes that pass over the new node get one step longer
        }
        self.len += 1;
    }

    fn delete(&mut self, score: f64, member: &[u8]) -> bool {
        let (mut update, mut x) = ([0usize; MAXL], 0usize);
        for i in (0..self.level).rev() {
            loop {
                let nx = self.nodes[x].next[i];
                if nx != NIL && self.before(nx, score, member) {
                    x = nx;
                } else {
                    break;
                }
            }
            update[i] = x;
        }
        let t = self.nodes[x].next[0];
        if t == NIL || self.nodes[t].score != score || self.nodes[t].member != member {
            return false;
        }
        for i in 0..self.level {
            let u = update[i];
            if self.nodes[u].next[i] == t {
                self.nodes[u].span[i] += self.nodes[t].span[i] - 1;
                self.nodes[u].next[i] = self.nodes[t].next[i];
            } else {
                self.nodes[u].span[i] -= 1;
            }
        }
        while self.level > 1 && self.nodes[0].next[self.level - 1] == NIL {
            self.level -= 1;
        }
        self.nodes[t] = ZNode {
            member: vec![],
            score: 0.0,
            next: vec![],
            span: vec![],
        };
        self.free.push(t);
        self.len -= 1;
        true
    }

    /// 0-based rank of (score, member), summing spans on the way down.
    fn rank(&self, score: f64, member: &[u8]) -> Option<usize> {
        let (mut rank, mut x) = (0, 0);
        for i in (0..self.level).rev() {
            loop {
                let nx = self.nodes[x].next[i];
                if nx != NIL && {
                    let n = &self.nodes[nx];
                    n.score < score || (n.score == score && n.member.as_slice() <= member)
                } {
                    rank += self.nodes[x].span[i];
                    x = nx;
                } else {
                    break;
                }
            }
            if x != 0 && self.nodes[x].member == member {
                return Some(rank - 1);
            }
        }
        None
    }

    fn get_by_rank(&self, r: usize) -> Option<usize> {
        let (target, mut seen, mut x) = (r + 1, 0, 0);
        for i in (0..self.level).rev() {
            loop {
                let nx = self.nodes[x].next[i];
                if nx != NIL && seen + self.nodes[x].span[i] <= target {
                    seen += self.nodes[x].span[i];
                    x = nx;
                } else {
                    break;
                }
            }
            if seen == target {
                return Some(x);
            }
        }
        None
    }

    /// First node with score >= min (or > min when exclusive); NIL if none.
    fn first_from(&self, min: f64, excl: bool) -> usize {
        let mut x = 0;
        for i in (0..self.level).rev() {
            loop {
                let nx = self.nodes[x].next[i];
                if nx != NIL
                    && (if excl {
                        self.nodes[nx].score <= min
                    } else {
                        self.nodes[nx].score < min
                    })
                {
                    x = nx;
                } else {
                    break;
                }
            }
        }
        self.nodes[x].next[0]
    }

    /// Structural self-check used by the tests: level-0 order, and every span equals the real distance.
    fn check(&self) {
        let mut pos = vec![usize::MAX; self.nodes.len()];
        let (mut x, mut n) = (self.nodes[0].next[0], 0);
        while x != NIL {
            pos[x] = n;
            n += 1;
            let nx = self.nodes[x].next[0];
            assert!(nx == NIL || !self.before(nx, self.nodes[x].score, &self.nodes[x].member));
            x = nx;
        }
        assert_eq!(n, self.len);
        for l in 0..self.level {
            let mut x = 0;
            while self.nodes[x].next[l] != NIL {
                let nx = self.nodes[x].next[l];
                let from = if x == 0 { 0 } else { pos[x] + 1 };
                assert_eq!(
                    pos[nx] + 1 - from,
                    self.nodes[x].span[l],
                    "span at level {l}"
                );
                x = nx;
            }
        }
    }
}

#[derive(Clone, Default)]
struct ZSet {
    dict: HashMap<B, f64>, // member -> score for O(1) ZSCORE; the skip list answers order and rank
    sl: SkipList,
}

impl ZSet {
    fn len(&self) -> usize {
        self.dict.len()
    }
    /// true if the member is new
    fn add(&mut self, m: &[u8], score: f64) -> bool {
        match self.dict.insert(m.to_vec(), score) {
            Some(old) if old == score => false,
            Some(old) => {
                self.sl.delete(old, m);
                self.sl.insert(score, m.to_vec());
                false
            }
            None => {
                self.sl.insert(score, m.to_vec());
                true
            }
        }
    }
    fn remove(&mut self, m: &[u8]) -> bool {
        match self.dict.remove(m) {
            Some(s) => self.sl.delete(s, m),
            None => false,
        }
    }
    fn rank(&self, m: &[u8], rev: bool) -> Option<usize> {
        let r = self.sl.rank(*self.dict.get(m)?, m)?;
        Some(if rev { self.len() - 1 - r } else { r })
    }
    fn walk(&self, mut x: usize, mut n: usize) -> Vec<(B, f64)> {
        let mut out = Vec::new();
        while x != NIL && n > 0 {
            out.push((self.sl.nodes[x].member.clone(), self.sl.nodes[x].score));
            x = self.sl.nodes[x].next[0];
            n -= 1;
        }
        out
    }
    /// inclusive 0-based indices in output order
    fn range(&self, start: usize, stop: usize, rev: bool) -> Vec<(B, f64)> {
        let (a, b) = if rev {
            (self.len() - 1 - stop, self.len() - 1 - start)
        } else {
            (start, stop)
        };
        let mut v = self.walk(self.sl.get_by_rank(a).unwrap_or(NIL), b - a + 1);
        if rev {
            v.reverse();
        }
        v
    }
    fn range_by_score(&self, min: (f64, bool), max: (f64, bool)) -> Vec<(B, f64)> {
        let mut out = Vec::new();
        let mut x = self.sl.first_from(min.0, min.1);
        while x != NIL {
            let n = &self.sl.nodes[x];
            if n.score > max.0 || (max.1 && n.score == max.0) {
                break;
            }
            out.push((n.member.clone(), n.score));
            x = n.next[0];
        }
        out
    }
    fn items(&self) -> Vec<(B, f64)> {
        self.walk(self.sl.nodes[0].next[0], usize::MAX)
    }
}

/// Redis prints scores with %.17g; integral values print without a decimal point.
fn fmt_score(s: f64) -> String {
    if s == f64::INFINITY {
        "inf".into()
    } else if s == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        format!("{s}")
    }
}

// ================================================================ 4. values and the expiry index

#[derive(Clone)]
enum Value {
    Str(B),
    List(VecDeque<B>),
    Hash(BTreeMap<B, B>), // BTree* instead of hash tables: deterministic reply order (real Redis: unspecified)
    Set(BTreeSet<B>),
    ZSet(ZSet),
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Hash(_) => "hash",
            Value::Set(_) => "set",
            Value::ZSet(_) => "zset",
        }
    }
    /// Redis never stores an empty aggregate: the key disappears with its last element.
    fn is_empty(&self) -> bool {
        match self {
            Value::Str(_) => false,
            Value::List(l) => l.is_empty(),
            Value::Hash(h) => h.is_empty(),
            Value::Set(s) => s.is_empty(),
            Value::ZSet(z) => z.len() == 0,
        }
    }
}

/// Keys with a TTL in a Vec (O(1) uniform random sampling for the active-expiry cycle) plus a position index.
#[derive(Clone, Default)]
struct Expires {
    items: Vec<(B, u64)>,
    pos: HashMap<B, usize>,
}

impl Expires {
    fn len(&self) -> usize {
        self.items.len()
    }
    fn get(&self, key: &[u8]) -> Option<u64> {
        self.pos.get(key).map(|&i| self.items[i].1)
    }
    fn set(&mut self, key: &[u8], at: u64) {
        match self.pos.get(key) {
            Some(&i) => self.items[i].1 = at,
            None => {
                self.pos.insert(key.to_vec(), self.items.len());
                self.items.push((key.to_vec(), at));
            }
        }
    }
    fn remove(&mut self, key: &[u8]) -> bool {
        let Some(i) = self.pos.remove(key) else {
            return false;
        };
        self.items.swap_remove(i);
        if i < self.items.len() {
            self.pos.insert(self.items[i].0.clone(), i);
        }
        true
    }
    fn clear(&mut self) {
        self.items.clear();
        self.pos.clear();
    }
}

// ================================================================ 5. the engine: state, dispatch, expiry

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fsync {
    Always,
    EverySec,
    No,
}

#[derive(Clone)]
struct Config {
    aof: Option<(PathBuf, Fsync)>,
    rdb: Option<PathBuf>,
    active_expire_every: Option<u32>, // run the sampling cycle every N executor ticks (10 ms each); None = off
    push_buffer: usize, // per-subscriber bounded queue; overflow disconnects the subscriber
    limits: Limits,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            aof: None,
            rdb: None,
            active_expire_every: Some(10),
            push_buffer: 4096,
            limits: Limits::default(),
        }
    }
}

#[derive(Default, Clone, Debug)]
struct Stats {
    commands: u64,
    connections: u64,
    expired: u64,
    slow_kills: u64,
    fsyncs: u64,
    rewrites: u64,
    saves: u64,
    aof_truncated: u64,
}

#[derive(Default)]
struct ClientState {
    push: Option<mpsc::Sender<Frame>>, // bounded queue of pub/sub pushes to this connection's writer
    kill: Option<Arc<Notify>>,         // wakes the connection task to close the socket
    subs: BTreeSet<B>,
    psubs: BTreeSet<B>,
    multi: Option<Vec<Vec<B>>>,
    multi_err: bool,
    watched: Vec<B>,
}

struct BatchReply {
    frames: Vec<Frame>,
    close: bool,
}

/// One pipelined batch from one connection. It can PAUSE in the middle (BLPOP) and be resumed later with the
/// commands after the blocking one still queued, which keeps replies in order.
struct Batch {
    conn: u64,
    cmds: VecDeque<Vec<B>>,
    frames: Vec<Frame>,
    close: bool,
    tx: Option<oneshot::Sender<BatchReply>>,
}

struct BlockSpec {
    keys: Vec<B>,
    left: bool,
    timeout_ms: u64,
}

struct Blocked {
    keys: Vec<B>,
    left: bool,
    deadline: Option<u64>,
    st: Batch,
}

enum Out {
    R(Frame),
    Multi(Vec<Frame>),
    Block(BlockSpec),
    Quit,
}

enum Bg {
    Save,
    Rewrite,
}

enum Msg {
    Connect {
        id: u64,
        push: mpsc::Sender<Frame>,
        kill: Arc<Notify>,
    },
    Req {
        conn: u64,
        cmds: Vec<Vec<B>>,
        reply: oneshot::Sender<BatchReply>,
    },
    Disconnect(u64),
    BgDone(Bg, Result<(), String>),
    Shutdown(oneshot::Sender<()>),
}

type R = Result<Out, Frame>;
type Handler = fn(&mut Engine, u64, &[B]) -> R;

struct Cmd {
    name: &'static str,
    arity: i32, // Redis convention: n > 0 exact argc (name included), n < 0 at least -n
    write: bool,
    f: Handler,
}

struct Aof {
    file: File,
    policy: Fsync,
    size: u64,
    unsynced: bool,
    last_sync: Instant,
}

struct Engine {
    map: BTreeMap<B, Value>, // BTreeMap: sorted KEYS and a SCAN cursor that is just "the last key I returned"
    expires: Expires,
    clock: Arc<dyn Clock>,
    now: u64,
    dirty: u64,
    custom: bool,  // a handler already pushed its own (rewritten) AOF record
    loading: bool, // replaying the AOF/RDB: expiry is suspended, like Redis
    in_exec: bool,
    log: Vec<Vec<B>>, // records to append to the AOF at the end of this loop iteration
    aof: Option<Aof>,
    rewrite_buf: Option<Vec<u8>>, // while a background rewrite runs, writes accumulate here and are appended at the end
    clients: HashMap<u64, ClientState>,
    watchers: HashMap<B, BTreeSet<u64>>,
    dirty_clients: BTreeSet<u64>,
    channels: HashMap<B, BTreeSet<u64>>,
    patterns: HashMap<B, BTreeSet<u64>>,
    waiters: HashMap<B, VecDeque<u64>>, // per list key, FIFO queue of blocked clients
    blocked: HashMap<u64, Blocked>,
    ready: Vec<B>,
    outbox: Vec<(oneshot::Sender<BatchReply>, BatchReply)>,
    stats: Stats,
    rng: Rng,
    cfg: Config,
    tx: xc::Sender<Msg>,
    bg_save: bool,
    bg_rewrite: bool,
    saved_at_dirty: u64,
    tick_n: u32,
    started: Instant,
    shutdown: Option<oneshot::Sender<()>>,
}

const SYNTAX: &str = "ERR syntax error";
const NOT_INT: &str = "ERR value is not an integer or out of range";
const NOT_FLOAT: &str = "ERR value is not a valid float";
const WRONGTYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";

fn wrongtype() -> Frame {
    Frame::err(WRONGTYPE)
}
fn syntax() -> Frame {
    Frame::err(SYNTAX)
}
fn fr(f: Frame) -> R {
    Ok(Out::R(f))
}
fn int(n: i64) -> R {
    fr(Frame::Int(n))
}
fn pi64(a: &[u8]) -> Result<i64, Frame> {
    parse_dec(a).ok_or_else(|| Frame::err(NOT_INT))
}
fn pf64(a: &[u8]) -> Result<f64, Frame> {
    let s = std::str::from_utf8(a).map_err(|_| Frame::err(NOT_FLOAT))?;
    match s.to_ascii_lowercase().as_str() {
        "inf" | "+inf" => Ok(f64::INFINITY),
        "-inf" => Ok(f64::NEG_INFINITY),
        t => t
            .parse::<f64>()
            .ok()
            .filter(|x| !x.is_nan() && !t.contains("nan"))
            .ok_or_else(|| Frame::err(NOT_FLOAT)),
    }
}
fn up(a: &[u8]) -> String {
    String::from_utf8_lossy(a).to_ascii_uppercase()
}
/// Redis range semantics: negative indexes count from the end, out-of-range ends are clamped.
fn norm_range(start: i64, stop: i64, len: usize) -> Option<(usize, usize)> {
    let n = len as i64;
    let s = if start < 0 { (n + start).max(0) } else { start };
    let e = if stop < 0 { n + stop } else { stop.min(n - 1) };
    (s <= e && s < n).then_some((s as usize, e as usize))
}
fn pbound(a: &[u8]) -> Result<(f64, bool), Frame> {
    let (excl, body) = match a.first() {
        Some(b'(') => (true, &a[1..]),
        _ => (false, a),
    };
    pf64(body)
        .map(|v| (v, excl))
        .map_err(|_| Frame::err("ERR min or max is not a float"))
}

macro_rules! typed {
    ($name:ident, $var:ident, $t:ty, $new:expr) => {
        /// typed accessor: WRONGTYPE on mismatch; `create` inserts an empty value of the right type
        fn $name(&mut self, key: &[u8], create: bool) -> Result<Option<&mut $t>, Frame> {
            self.expire_if_needed(key);
            if create && !self.map.contains_key(key) {
                self.map.insert(key.to_vec(), Value::$var($new));
            }
            match self.map.get_mut(key) {
                None => Ok(None),
                Some(Value::$var(x)) => Ok(Some(x)),
                Some(_) => Err(wrongtype()),
            }
        }
    };
}

impl Engine {
    typed!(get_str, Str, B, Vec::new());
    typed!(get_list, List, VecDeque<B>, VecDeque::new());
    typed!(get_hash, Hash, BTreeMap<B, B>, BTreeMap::new());
    typed!(get_set, Set, BTreeSet<B>, BTreeSet::new());
    typed!(get_zset, ZSet, ZSet, ZSet::default());

    fn new(cfg: Config, clock: Arc<dyn Clock>, tx: xc::Sender<Msg>) -> io::Result<Engine> {
        let mut e = Engine {
            map: BTreeMap::new(),
            expires: Expires::default(),
            now: clock.now_ms(),
            clock,
            dirty: 0,
            custom: false,
            loading: false,
            in_exec: false,
            log: vec![],
            aof: None,
            rewrite_buf: None,
            clients: HashMap::new(),
            watchers: HashMap::new(),
            dirty_clients: BTreeSet::new(),
            channels: HashMap::new(),
            patterns: HashMap::new(),
            waiters: HashMap::new(),
            blocked: HashMap::new(),
            ready: vec![],
            outbox: vec![],
            stats: Stats::default(),
            rng: Rng(0x2545_F491_4F6C_DD1D),
            cfg: cfg.clone(),
            tx,
            bg_save: false,
            bg_rewrite: false,
            saved_at_dirty: 0,
            tick_n: 0,
            started: Instant::now(),
            shutdown: None,
        };
        e.load_persistence()?;
        Ok(e)
    }

    // ---------------------------------------------------------- expiry

    /// LAZY expiry: every access path calls this first, so an expired key is never observable.
    fn expire_if_needed(&mut self, key: &[u8]) -> bool {
        if !self.loading && self.expires.get(key).is_some_and(|t| t <= self.now) {
            self.delete_expired(key);
            return true;
        }
        false
    }

    /// Expiry is replicated to the AOF as an explicit DEL, so replay never has to agree with a wall clock.
    fn delete_expired(&mut self, key: &[u8]) {
        self.map.remove(key);
        self.expires.remove(key);
        self.touch(key);
        self.stats.expired += 1;
        self.log.push(vec![b"DEL".to_vec(), key.to_vec()]);
    }

    /// ACTIVE expiry, the Redis algorithm: sample 20 random keys that have a TTL, delete the expired ones, and repeat
    /// while more than 25% of the sample was expired (the keyspace is then probably full of garbage). The bound on
    /// iterations/time stops a huge expiry storm from stalling the single executor thread.
    fn active_expire_cycle(&mut self, max_rounds: u32, budget: Option<Duration>) -> usize {
        self.now = self.clock.now_ms();
        let (start, mut deleted) = (Instant::now(), 0);
        for _ in 0..max_rounds {
            let n = self.expires.len().min(20);
            if n == 0 {
                break;
            }
            let mut hit = 0;
            for _ in 0..n {
                if self.expires.len() == 0 {
                    break;
                }
                let i = self.rng.below(self.expires.len() as u64) as usize;
                let (k, t) = self.expires.items[i].clone();
                if t <= self.now {
                    self.delete_expired(&k);
                    hit += 1;
                }
            }
            deleted += hit;
            if hit * 4 <= n || budget.is_some_and(|b| start.elapsed() > b) {
                break;
            }
        }
        deleted
    }

    fn touch(&mut self, key: &[u8]) {
        self.dirty += 1;
        if let Some(ws) = self.watchers.get(key) {
            self.dirty_clients.extend(ws.iter().copied());
        }
    }

    /// call after every real mutation of `key`: bump dirty/watchers, drop the key if the aggregate became empty
    fn after_write(&mut self, key: &[u8]) {
        self.touch(key);
        if self.map.get(key).is_some_and(|v| v.is_empty()) {
            self.map.remove(key);
            self.expires.remove(key);
        }
    }

    fn propagate(&mut self, rec: Vec<B>) {
        self.custom = true;
        self.log.push(rec);
    }

    fn set_expire_at(&mut self, key: &[u8], at: i64) -> R {
        self.expire_if_needed(key);
        if !self.map.contains_key(key) {
            return int(0);
        }
        if at <= self.now as i64 {
            self.map.remove(key);
            self.expires.remove(key);
            self.touch(key);
            self.propagate(vec![b"DEL".to_vec(), key.to_vec()]);
        } else {
            self.expires.set(key, at as u64);
            self.touch(key);
            self.propagate(vec![
                b"PEXPIREAT".to_vec(),
                key.to_vec(),
                at.to_string().into_bytes(),
            ]);
        }
        int(1)
    }

    // ---------------------------------------------------------- dispatch

    fn lookup(name: &str) -> Option<&'static Cmd> {
        static IDX: OnceLock<HashMap<&'static str, &'static Cmd>> = OnceLock::new();
        IDX.get_or_init(|| TABLE.iter().map(|c| (c.name, c)).collect())
            .get(name)
            .copied()
    }

    fn validate(args: &[B]) -> Result<&'static Cmd, Frame> {
        let name = up(&args[0]);
        let Some(cmd) = Engine::lookup(&name) else {
            return Err(Frame::err(format!(
                "ERR unknown command '{}'",
                String::from_utf8_lossy(&args[0])
            )));
        };
        let n = args.len() as i32;
        if (cmd.arity > 0 && n != cmd.arity) || (cmd.arity < 0 && n < -cmd.arity) {
            return Err(Frame::err(format!(
                "ERR wrong number of arguments for '{}' command",
                name.to_lowercase()
            )));
        }
        Ok(cmd)
    }

    fn client(&mut self, id: u64) -> &mut ClientState {
        self.clients.entry(id).or_default()
    }

    /// Connection-state gating (subscriber mode, MULTI queueing) then real execution.
    fn dispatch(&mut self, conn: u64, args: &[B]) -> Out {
        let name = up(&args[0]);
        let c = self.client(conn);
        if c.subs.len() + c.psubs.len() > 0
            && !matches!(
                name.as_str(),
                "SUBSCRIBE" | "UNSUBSCRIBE" | "PSUBSCRIBE" | "PUNSUBSCRIBE" | "PING" | "QUIT"
            )
        {
            return Out::R(Frame::err(format!(
                "ERR Can't execute '{}': only (P)SUBSCRIBE / (P)UNSUBSCRIBE / PING / QUIT are allowed in this context",
                name.to_lowercase()
            )));
        }
        if c.multi.is_some()
            && !matches!(
                name.as_str(),
                "EXEC" | "DISCARD" | "MULTI" | "WATCH" | "QUIT"
            )
        {
            return Out::R(match Engine::validate(args) {
                Ok(_) => {
                    self.client(conn)
                        .multi
                        .as_mut()
                        .unwrap()
                        .push(args.to_vec());
                    Frame::Simple("QUEUED".into())
                }
                Err(e) => {
                    self.client(conn).multi_err = true; // EXEC will refuse to run a transaction with a bad command
                    e
                }
            });
        }
        self.execute(conn, args)
    }

    fn execute(&mut self, conn: u64, args: &[B]) -> Out {
        self.now = self.clock.now_ms();
        self.stats.commands += 1;
        let cmd = match Engine::validate(args) {
            Ok(c) => c,
            Err(e) => return Out::R(e),
        };
        let before = self.dirty;
        self.custom = false;
        let out = match (cmd.f)(self, conn, args) {
            Ok(o) => o,
            Err(e) => Out::R(e),
        };
        if cmd.write && self.dirty != before && !self.custom {
            self.log.push(args.to_vec()); // default: replicate the command as received
        }
        out
    }

    fn run_batch(&mut self, mut st: Batch) {
        while let Some(cmd) = st.cmds.pop_front() {
            match self.dispatch(st.conn, &cmd) {
                Out::R(f) => st.frames.push(f),
                Out::Multi(v) => st.frames.extend(v),
                Out::Quit => {
                    st.frames.push(Frame::ok());
                    st.close = true;
                    break;
                }
                Out::Block(spec) => {
                    self.block(st, spec);
                    return;
                }
            }
        }
        if let Some(tx) = st.tx {
            self.outbox.push((
                tx,
                BatchReply {
                    frames: st.frames,
                    close: st.close,
                },
            ));
        }
    }

    fn block(&mut self, st: Batch, spec: BlockSpec) {
        let deadline = (spec.timeout_ms > 0).then(|| self.now + spec.timeout_ms);
        for k in &spec.keys {
            self.waiters
                .entry(k.clone())
                .or_default()
                .push_back(st.conn);
        }
        self.blocked.insert(
            st.conn,
            Blocked {
                keys: spec.keys,
                left: spec.left,
                deadline,
                st,
            },
        );
    }

    fn unblock(&mut self, conn: u64) -> Option<Blocked> {
        let b = self.blocked.remove(&conn)?;
        for k in &b.keys {
            if let Some(q) = self.waiters.get_mut(k) {
                q.retain(|c| *c != conn);
                if q.is_empty() {
                    self.waiters.remove(k);
                }
            }
        }
        Some(b)
    }

    /// A list got new elements: hand them to the longest-waiting blocked clients, FIFO, and resume their batches.
    fn serve_waiters(&mut self, key: &[u8]) {
        loop {
            let Some(&conn) = self.waiters.get(key).and_then(|q| q.front()) else {
                break;
            };
            let left = self.blocked[&conn].left;
            let popped = match self.get_list(key, false) {
                Ok(Some(l)) => {
                    if left {
                        l.pop_front()
                    } else {
                        l.pop_back()
                    }
                }
                _ => None,
            };
            let Some(v) = popped else { break };
            self.after_write(key);
            let name: &[u8] = if left { b"LPOP" } else { b"RPOP" };
            self.log.push(vec![name.to_vec(), key.to_vec()]); // a served BLPOP is persisted as the plain pop it was
            let mut b = self.unblock(conn).expect("waiter is blocked");
            b.st.frames.push(Frame::bulks([key.to_vec(), v]));
            self.run_batch(b.st);
        }
    }

    fn settle(&mut self) {
        while let Some(k) = self.ready.pop() {
            self.serve_waiters(&k);
        }
    }

    fn drop_client(&mut self, id: u64) {
        self.unblock(id);
        self.unwatch(id);
        self.unsubscribe_all(id);
        self.clients.remove(&id);
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Connect { id, push, kill } => {
                self.stats.connections += 1;
                let c = self.client(id);
                c.push = Some(push);
                c.kill = Some(kill);
            }
            Msg::Req { conn, cmds, reply } => {
                let st = Batch {
                    conn,
                    cmds: cmds.into(),
                    frames: vec![],
                    close: false,
                    tx: Some(reply),
                };
                self.run_batch(st);
            }
            Msg::Disconnect(id) => self.drop_client(id),
            Msg::BgDone(kind, res) => self.bg_done(kind, res),
            Msg::Shutdown(ack) => self.shutdown = Some(ack),
        }
        self.settle();
    }

    fn on_tick(&mut self) {
        self.now = self.clock.now_ms();
        let mut due: Vec<(u64, u64)> = self
            .blocked
            .iter()
            .filter_map(|(c, b)| b.deadline.filter(|d| *d <= self.now).map(|d| (d, *c)))
            .collect();
        due.sort(); // earliest deadline first
        for (_, c) in due {
            if let Some(mut b) = self.unblock(c) {
                b.st.frames.push(Frame::NullArray); // BLPOP timeout reply
                self.run_batch(b.st);
            }
        }
        self.tick_n = self.tick_n.wrapping_add(1);
        if let Some(n) = self.cfg.active_expire_every
            && self.tick_n % n == 0
        {
            self.active_expire_cycle(64, Some(Duration::from_millis(2)));
        }
        self.everysec_sync();
        self.settle();
    }

    /// The executor thread: block on the channel, drain everything queued (so one fsync covers many clients: group
    /// commit), execute, persist, THEN release the replies. Real Redis does the same in `beforeSleep`.
    fn run(mut self, rx: xc::Receiver<Msg>) {
        let mut last_tick = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(m) => {
                    self.handle(m);
                    for _ in 0..1024 {
                        match rx.try_recv() {
                            Ok(m) => self.handle(m),
                            Err(_) => break,
                        }
                    }
                }
                Err(xc::RecvTimeoutError::Timeout) => {}
                Err(xc::RecvTimeoutError::Disconnected) => break,
            }
            if last_tick.elapsed() >= Duration::from_millis(10) {
                last_tick = Instant::now();
                self.on_tick();
            }
            self.flush_aof();
            for (tx, reply) in self.outbox.drain(..) {
                let _ = tx.send(reply);
            }
            if self.shutdown.is_some() {
                break;
            }
        }
        if let Some(a) = &mut self.aof {
            let _ = a.file.sync_all();
        }
        if let Some(ack) = self.shutdown.take() {
            let _ = ack.send(());
        }
    }
}

// ================================================================ 6. the commands

macro_rules! table {
    ($( ($name:literal, $arity:expr, $w:expr, $f:ident) ),* $(,)?) => {
        static TABLE: &[Cmd] = &[ $( Cmd { name: $name, arity: $arity, write: $w, f: Engine::$f } ),* ];
    };
}

table! {
    // strings
    ("GET", 2, false, c_get), ("SET", -3, true, c_set), ("SETNX", 3, true, c_setnx), ("SETEX", 4, true, c_setex),
    ("GETSET", 3, true, c_getset), ("APPEND", 3, true, c_append), ("STRLEN", 2, false, c_strlen),
    ("INCR", 2, true, c_incr), ("DECR", 2, true, c_decr), ("INCRBY", 3, true, c_incrby), ("DECRBY", 3, true, c_decrby),
    ("MGET", -2, false, c_mget), ("MSET", -3, true, c_mset), ("GETRANGE", 4, false, c_getrange),
    // keys
    ("DEL", -2, true, c_del), ("EXISTS", -2, false, c_exists), ("EXPIRE", 3, true, c_expire), ("PEXPIRE", 3, true, c_pexpire),
    ("PEXPIREAT", 3, true, c_pexpireat), ("TTL", 2, false, c_ttl), ("PTTL", 2, false, c_pttl), ("PERSIST", 2, true, c_persist),
    ("TYPE", 2, false, c_type), ("KEYS", 2, false, c_keys), ("RENAME", 3, true, c_rename), ("SCAN", -2, false, c_scan),
    ("DBSIZE", 1, false, c_dbsize), ("FLUSHALL", 1, true, c_flushall),
    // lists
    ("LPUSH", -3, true, c_lpush), ("RPUSH", -3, true, c_rpush), ("LPOP", -2, true, c_lpop), ("RPOP", -2, true, c_rpop),
    ("LLEN", 2, false, c_llen), ("LRANGE", 4, false, c_lrange), ("LINDEX", 3, false, c_lindex), ("LTRIM", 4, true, c_ltrim),
    ("BLPOP", -3, true, c_blpop), ("BRPOP", -3, true, c_brpop),
    // hashes
    ("HSET", -4, true, c_hset), ("HGET", 3, false, c_hget), ("HDEL", -3, true, c_hdel), ("HGETALL", 2, false, c_hgetall),
    ("HINCRBY", 4, true, c_hincrby), ("HLEN", 2, false, c_hlen), ("HEXISTS", 3, false, c_hexists),
    ("HKEYS", 2, false, c_hkeys), ("HVALS", 2, false, c_hvals),
    // sets
    ("SADD", -3, true, c_sadd), ("SREM", -3, true, c_srem), ("SMEMBERS", 2, false, c_smembers),
    ("SISMEMBER", 3, false, c_sismember), ("SCARD", 2, false, c_scard), ("SINTER", -2, false, c_sinter),
    ("SUNION", -2, false, c_sunion), ("SDIFF", -2, false, c_sdiff),
    // sorted sets
    ("ZADD", -4, true, c_zadd), ("ZREM", -3, true, c_zrem), ("ZRANGE", -4, false, c_zrange), ("ZREVRANGE", -4, false, c_zrevrange),
    ("ZRANGEBYSCORE", -4, false, c_zrangebyscore), ("ZRANK", 3, false, c_zrank), ("ZREVRANK", 3, false, c_zrevrank),
    ("ZSCORE", 3, false, c_zscore), ("ZINCRBY", 4, true, c_zincrby), ("ZCARD", 2, false, c_zcard), ("ZCOUNT", 4, false, c_zcount),
    // pub/sub, transactions
    ("SUBSCRIBE", -2, false, c_subscribe), ("UNSUBSCRIBE", -1, false, c_unsubscribe), ("PSUBSCRIBE", -2, false, c_psubscribe),
    ("PUNSUBSCRIBE", -1, false, c_punsubscribe), ("PUBLISH", 3, false, c_publish),
    ("MULTI", 1, false, c_multi), ("EXEC", 1, false, c_exec), ("DISCARD", 1, false, c_discard),
    ("WATCH", -2, false, c_watch), ("UNWATCH", 1, false, c_unwatch),
    // server
    ("PING", -1, false, c_ping), ("ECHO", 2, false, c_echo), ("INFO", -1, false, c_info), ("COMMAND", -1, false, c_command),
    ("QUIT", -1, false, c_quit), ("DEBUG", -2, false, c_debug), ("SAVE", 1, false, c_save), ("BGSAVE", 1, false, c_bgsave),
    ("BGREWRITEAOF", 1, false, c_bgrewriteaof),
}

impl Engine {
    // ---------------------------------------------------------- strings

    fn c_get(&mut self, _: u64, a: &[B]) -> R {
        fr(match self.get_str(&a[1], false)? {
            Some(v) => Frame::bulk(&v[..]),
            None => Frame::Null,
        })
    }

    fn put_str(&mut self, key: &[u8], val: B, keep_ttl: bool, exp: Option<u64>) {
        self.map.insert(key.to_vec(), Value::Str(val));
        if !keep_ttl {
            self.expires.remove(key);
        }
        if let Some(t) = exp {
            self.expires.set(key, t);
        }
        self.touch(key);
    }

    fn c_set(&mut self, _: u64, a: &[B]) -> R {
        let (mut nx, mut xx, mut keep, mut exp) = (false, false, false, None::<u64>);
        let mut i = 3;
        while i < a.len() {
            match up(&a[i]).as_str() {
                "NX" => nx = true,
                "XX" => xx = true,
                "KEEPTTL" => keep = true,
                o @ ("EX" | "PX" | "EXAT" | "PXAT") => {
                    i += 1;
                    let n = pi64(a.get(i).ok_or_else(syntax)?)?;
                    let bad = || Frame::err("ERR invalid expire time in 'set' command");
                    if n <= 0 {
                        return Err(bad());
                    }
                    let ms = match o {
                        "EX" => n
                            .checked_mul(1000)
                            .and_then(|x| x.checked_add(self.now as i64)),
                        "PX" => n.checked_add(self.now as i64),
                        "EXAT" => n.checked_mul(1000),
                        _ => Some(n),
                    };
                    if exp.replace(ms.ok_or_else(bad)? as u64).is_some() {
                        return Err(syntax());
                    }
                }
                _ => return Err(syntax()),
            }
            i += 1;
        }
        if (nx && xx) || (keep && exp.is_some()) {
            return Err(syntax());
        }
        self.expire_if_needed(&a[1]);
        let exists = self.map.contains_key(&a[1]);
        if (nx && exists) || (xx && !exists) {
            return fr(Frame::Null);
        }
        self.put_str(&a[1], a[2].clone(), keep, exp);
        if let Some(t) = exp {
            self.propagate(vec![
                b"SET".to_vec(),
                a[1].clone(),
                a[2].clone(),
                b"PXAT".to_vec(),
                t.to_string().into_bytes(),
            ]);
        }
        fr(Frame::ok())
    }

    fn c_setnx(&mut self, _: u64, a: &[B]) -> R {
        self.expire_if_needed(&a[1]);
        if self.map.contains_key(&a[1]) {
            return int(0);
        }
        self.put_str(&a[1], a[2].clone(), false, None);
        int(1)
    }

    fn c_setex(&mut self, _: u64, a: &[B]) -> R {
        let n = pi64(&a[2])?;
        let at = n
            .checked_mul(1000)
            .and_then(|x| x.checked_add(self.now as i64))
            .filter(|_| n > 0);
        let at = at.ok_or_else(|| Frame::err("ERR invalid expire time in 'setex' command"))? as u64;
        self.expire_if_needed(&a[1]);
        self.put_str(&a[1], a[3].clone(), false, Some(at));
        self.propagate(vec![
            b"SET".to_vec(),
            a[1].clone(),
            a[3].clone(),
            b"PXAT".to_vec(),
            at.to_string().into_bytes(),
        ]);
        fr(Frame::ok())
    }

    fn c_getset(&mut self, _: u64, a: &[B]) -> R {
        let old = self.get_str(&a[1], false)?.map(|v| v.clone());
        self.put_str(&a[1], a[2].clone(), false, None);
        fr(old.map_or(Frame::Null, Frame::Bulk))
    }

    fn c_append(&mut self, _: u64, a: &[B]) -> R {
        let v = self.get_str(&a[1], true)?.unwrap();
        v.extend_from_slice(&a[2]);
        let n = v.len();
        self.touch(&a[1]);
        int(n as i64)
    }

    fn c_strlen(&mut self, _: u64, a: &[B]) -> R {
        int(self.get_str(&a[1], false)?.map_or(0, |v| v.len()) as i64)
    }

    fn incr_by(&mut self, key: &[u8], d: i64) -> R {
        let cur = match self.get_str(key, false)? {
            Some(v) => pi64(v)?,
            None => 0,
        };
        let n = cur
            .checked_add(d)
            .ok_or_else(|| Frame::err("ERR increment or decrement would overflow"))?;
        self.map
            .insert(key.to_vec(), Value::Str(n.to_string().into_bytes())); // TTL is kept: only the value changes
        self.touch(key);
        int(n)
    }
    fn c_incr(&mut self, _: u64, a: &[B]) -> R {
        self.incr_by(&a[1], 1)
    }
    fn c_decr(&mut self, _: u64, a: &[B]) -> R {
        self.incr_by(&a[1], -1)
    }
    fn c_incrby(&mut self, _: u64, a: &[B]) -> R {
        self.incr_by(&a[1], pi64(&a[2])?)
    }
    fn c_decrby(&mut self, _: u64, a: &[B]) -> R {
        let d = pi64(&a[2])?
            .checked_neg()
            .ok_or_else(|| Frame::err("ERR decrement would overflow"))?;
        self.incr_by(&a[1], d)
    }

    fn c_mget(&mut self, _: u64, a: &[B]) -> R {
        let items = a[1..].iter().map(|k| match self.get_str(k, false) {
            Ok(Some(v)) => Frame::bulk(&v[..]),
            _ => Frame::Null, // MGET answers nil for wrong-typed keys instead of failing
        });
        fr(Frame::Array(items.collect()))
    }

    fn c_mset(&mut self, _: u64, a: &[B]) -> R {
        if a.len() % 2 != 1 {
            return Err(Frame::err(
                "ERR wrong number of arguments for 'mset' command",
            ));
        }
        for kv in a[1..].chunks(2) {
            self.expire_if_needed(&kv[0]);
            self.put_str(&kv[0], kv[1].clone(), false, None);
        }
        fr(Frame::ok())
    }

    fn c_getrange(&mut self, _: u64, a: &[B]) -> R {
        let (s, e) = (pi64(&a[2])?, pi64(&a[3])?);
        let v = self.get_str(&a[1], false)?.cloned().unwrap_or_default();
        fr(Frame::Bulk(
            norm_range(s, e, v.len()).map_or(vec![], |(s, e)| v[s..=e].to_vec()),
        ))
    }

    // ---------------------------------------------------------- keys

    fn c_del(&mut self, _: u64, a: &[B]) -> R {
        let mut n = 0;
        for k in &a[1..] {
            self.expire_if_needed(k);
            if self.map.remove(k.as_slice()).is_some() {
                self.expires.remove(k);
                self.touch(k);
                n += 1;
            }
        }
        int(n)
    }

    fn c_exists(&mut self, _: u64, a: &[B]) -> R {
        let mut n = 0;
        for k in &a[1..] {
            self.expire_if_needed(k);
            n += self.map.contains_key(k.as_slice()) as i64;
        }
        int(n)
    }

    fn c_expire(&mut self, _: u64, a: &[B]) -> R {
        let at = pi64(&a[2])?
            .checked_mul(1000)
            .and_then(|x| x.checked_add(self.now as i64));
        self.set_expire_at(
            &a[1],
            at.ok_or_else(|| Frame::err("ERR invalid expire time in 'expire' command"))?,
        )
    }
    fn c_pexpire(&mut self, _: u64, a: &[B]) -> R {
        let at = pi64(&a[2])?.checked_add(self.now as i64);
        self.set_expire_at(
            &a[1],
            at.ok_or_else(|| Frame::err("ERR invalid expire time in 'pexpire' command"))?,
        )
    }
    fn c_pexpireat(&mut self, _: u64, a: &[B]) -> R {
        self.set_expire_at(&a[1], pi64(&a[2])?)
    }

    /// -2 no such key, -1 no TTL, else remaining milliseconds
    fn pttl(&mut self, key: &[u8]) -> i64 {
        self.expire_if_needed(key);
        match (self.map.contains_key(key), self.expires.get(key)) {
            (false, _) => -2,
            (true, None) => -1,
            (true, Some(t)) => (t - self.now) as i64,
        }
    }
    fn c_ttl(&mut self, _: u64, a: &[B]) -> R {
        let ms = self.pttl(&a[1]);
        int(if ms < 0 { ms } else { (ms + 500) / 1000 })
    }
    fn c_pttl(&mut self, _: u64, a: &[B]) -> R {
        int(self.pttl(&a[1]))
    }

    fn c_persist(&mut self, _: u64, a: &[B]) -> R {
        self.expire_if_needed(&a[1]);
        if self.expires.remove(&a[1]) {
            self.touch(&a[1]);
            return int(1);
        }
        int(0)
    }

    fn c_type(&mut self, _: u64, a: &[B]) -> R {
        self.expire_if_needed(&a[1]);
        fr(Frame::Simple(
            self.map.get(&a[1]).map_or("none", |v| v.type_name()).into(),
        ))
    }

    fn c_keys(&mut self, _: u64, a: &[B]) -> R {
        let g = Glob::new(&a[1]);
        let now = self.now;
        let live = |k: &B| !self.expires.get(k).is_some_and(|t| t <= now) && g.is_match(k);
        fr(Frame::bulks(self.map.keys().filter(|k| live(k)).cloned()))
    }

    fn c_rename(&mut self, _: u64, a: &[B]) -> R {
        self.expire_if_needed(&a[1]);
        self.expire_if_needed(&a[2]);
        let ttl = self.expires.get(&a[1]);
        let Some(v) = self.map.remove(&a[1]) else {
            return Err(Frame::err("ERR no such key"));
        };
        self.expires.remove(&a[1]);
        self.touch(&a[1]);
        self.map.insert(a[2].clone(), v);
        self.expires.remove(&a[2]);
        if let Some(t) = ttl {
            self.expires.set(&a[2], t);
        }
        self.touch(&a[2]);
        fr(Frame::ok())
    }

    /// SCAN: the cursor is "1" + hex(last key examined); "0" starts and ends. Because the keyspace is ordered, a key
    /// that exists for the whole iteration is returned exactly once (Redis' hash-table cursor may repeat keys).
    fn c_scan(&mut self, _: u64, a: &[B]) -> R {
        let after = match a[1].as_slice() {
            b"0" => None,
            [b'1', rest @ ..] => Some(unhex(rest).ok_or_else(|| Frame::err("ERR invalid cursor"))?),
            _ => return Err(Frame::err("ERR invalid cursor")),
        };
        let (mut count, mut pat, mut i) = (10usize, None, 2);
        while i < a.len() {
            let v = a.get(i + 1).ok_or_else(syntax)?;
            match up(&a[i]).as_str() {
                "COUNT" => count = pi64(v)?.clamp(1, 100_000) as usize,
                "MATCH" => pat = Some(Glob::new(v)),
                _ => return Err(syntax()),
            }
            i += 2;
        }
        let lo = after.as_deref().map_or(Bound::Unbounded, Bound::Excluded);
        let (mut out, mut last, mut seen) = (vec![], None, 0);
        for (k, _) in self.map.range::<[u8], _>((lo, Bound::Unbounded)) {
            if seen == count {
                break;
            }
            seen += 1;
            last = Some(k.clone());
            if !self.expires.get(k).is_some_and(|t| t <= self.now)
                && pat.as_ref().is_none_or(|g| g.is_match(k))
            {
                out.push(k.clone());
            }
        }
        let more = last.as_ref().is_some_and(|l| {
            self.map
                .range::<[u8], _>((Bound::Excluded(l.as_slice()), Bound::Unbounded))
                .next()
                .is_some()
        });
        let cursor = match (more, last) {
            (true, Some(l)) => format!("1{}", hex(&l)),
            _ => "0".into(),
        };
        fr(Frame::Array(vec![Frame::bulk(cursor), Frame::bulks(out)]))
    }

    fn c_dbsize(&mut self, _: u64, _: &[B]) -> R {
        int(self.map.len() as i64) // like Redis: counts keys that expired but were not reclaimed yet
    }

    fn c_flushall(&mut self, _: u64, _: &[B]) -> R {
        self.map.clear();
        self.expires.clear();
        self.dirty += 1;
        let all: Vec<u64> = self.watchers.values().flatten().copied().collect();
        self.dirty_clients.extend(all);
        fr(Frame::ok())
    }

    // ---------------------------------------------------------- lists

    fn push(&mut self, a: &[B], left: bool) -> R {
        let l = self.get_list(&a[1], true)?.unwrap();
        for v in &a[2..] {
            if left {
                l.push_front(v.clone())
            } else {
                l.push_back(v.clone())
            }
        }
        let n = l.len();
        if self.waiters.contains_key(&a[1]) {
            self.ready.push(a[1].clone());
        }
        self.after_write(&a[1]);
        int(n as i64)
    }
    fn c_lpush(&mut self, _: u64, a: &[B]) -> R {
        self.push(a, true)
    }
    fn c_rpush(&mut self, _: u64, a: &[B]) -> R {
        self.push(a, false)
    }

    fn pop(&mut self, a: &[B], left: bool) -> R {
        if a.len() > 3 {
            return Err(syntax());
        }
        let count = match a.get(2) {
            Some(c) => Some(
                usize::try_from(pi64(c)?)
                    .map_err(|_| Frame::err("ERR value is out of range, must be positive"))?,
            ),
            None => None,
        };
        let Some(l) = self.get_list(&a[1], false)? else {
            return fr(if count.is_some() {
                Frame::NullArray
            } else {
                Frame::Null
            });
        };
        let take = count.unwrap_or(1).min(l.len());
        let out: Vec<B> = (0..take)
            .filter_map(|_| if left { l.pop_front() } else { l.pop_back() })
            .collect();
        if take > 0 {
            self.after_write(&a[1]);
        }
        fr(match count {
            None => Frame::Bulk(out.into_iter().next().unwrap()),
            Some(_) => Frame::bulks(out),
        })
    }
    fn c_lpop(&mut self, _: u64, a: &[B]) -> R {
        self.pop(a, true)
    }
    fn c_rpop(&mut self, _: u64, a: &[B]) -> R {
        self.pop(a, false)
    }

    fn c_llen(&mut self, _: u64, a: &[B]) -> R {
        int(self.get_list(&a[1], false)?.map_or(0, |l| l.len()) as i64)
    }

    fn c_lrange(&mut self, _: u64, a: &[B]) -> R {
        let (s, e) = (pi64(&a[2])?, pi64(&a[3])?);
        let Some(l) = self.get_list(&a[1], false)? else {
            return fr(Frame::Array(vec![]));
        };
        fr(Frame::bulks(
            norm_range(s, e, l.len())
                .into_iter()
                .flat_map(|(s, e)| l.range(s..=e).cloned())
                .collect::<Vec<_>>(),
        ))
    }

    fn c_lindex(&mut self, _: u64, a: &[B]) -> R {
        let i = pi64(&a[2])?;
        let Some(l) = self.get_list(&a[1], false)? else {
            return fr(Frame::Null);
        };
        let i = if i < 0 { l.len() as i64 + i } else { i };
        fr(usize::try_from(i)
            .ok()
            .and_then(|i| l.get(i))
            .map_or(Frame::Null, Frame::bulk))
    }

    fn c_ltrim(&mut self, _: u64, a: &[B]) -> R {
        let (s, e) = (pi64(&a[2])?, pi64(&a[3])?);
        let Some(l) = self.get_list(&a[1], false)? else {
            return fr(Frame::ok());
        };
        match norm_range(s, e, l.len()) {
            Some((s, e)) => {
                l.truncate(e + 1);
                l.drain(..s);
            }
            None => l.clear(),
        }
        self.after_write(&a[1]);
        fr(Frame::ok())
    }

    /// BLPOP/BRPOP: serve immediately from the first non-empty list, else park this client in the FIFO waiter queue.
    /// Inside MULTI/EXEC (and when replaying) they never block, they return nil, as in Redis.
    fn bpop(&mut self, a: &[B], left: bool) -> R {
        let t = std::str::from_utf8(&a[a.len() - 1])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|t| t.is_finite());
        let t = t.ok_or_else(|| Frame::err("ERR timeout is not a float or out of range"))?;
        if t < 0.0 {
            return Err(Frame::err("ERR timeout is negative"));
        }
        for k in &a[1..a.len() - 1] {
            let popped = match self.get_list(k, false)? {
                Some(l) => {
                    if left {
                        l.pop_front()
                    } else {
                        l.pop_back()
                    }
                }
                None => None,
            };
            if let Some(v) = popped {
                self.after_write(k);
                let name: &[u8] = if left { b"LPOP" } else { b"RPOP" };
                self.propagate(vec![name.to_vec(), k.clone()]);
                return fr(Frame::bulks([k.clone(), v]));
            }
        }
        if self.in_exec {
            return fr(Frame::NullArray);
        }
        let mut keys = a[1..a.len() - 1].to_vec();
        keys.dedup();
        Ok(Out::Block(BlockSpec {
            keys,
            left,
            timeout_ms: (t * 1000.0).ceil() as u64,
        }))
    }
    fn c_blpop(&mut self, _: u64, a: &[B]) -> R {
        self.bpop(a, true)
    }
    fn c_brpop(&mut self, _: u64, a: &[B]) -> R {
        self.bpop(a, false)
    }

    // ---------------------------------------------------------- hashes

    fn c_hset(&mut self, _: u64, a: &[B]) -> R {
        if a.len() % 2 != 0 {
            return Err(Frame::err(
                "ERR wrong number of arguments for 'hset' command",
            ));
        }
        let h = self.get_hash(&a[1], true)?.unwrap();
        let added = a[2..]
            .chunks(2)
            .filter(|kv| h.insert(kv[0].clone(), kv[1].clone()).is_none())
            .count();
        self.after_write(&a[1]);
        int(added as i64)
    }
    fn c_hget(&mut self, _: u64, a: &[B]) -> R {
        fr(self
            .get_hash(&a[1], false)?
            .and_then(|h| h.get(&a[2]))
            .map_or(Frame::Null, Frame::bulk))
    }
    fn c_hdel(&mut self, _: u64, a: &[B]) -> R {
        let Some(h) = self.get_hash(&a[1], false)? else {
            return int(0);
        };
        let n = a[2..]
            .iter()
            .filter(|f| h.remove(f.as_slice()).is_some())
            .count();
        if n > 0 {
            self.after_write(&a[1]);
        }
        int(n as i64)
    }
    fn c_hgetall(&mut self, _: u64, a: &[B]) -> R {
        let Some(h) = self.get_hash(&a[1], false)? else {
            return fr(Frame::Array(vec![]));
        };
        fr(Frame::bulks(
            h.iter()
                .flat_map(|(k, v)| [k.clone(), v.clone()])
                .collect::<Vec<_>>(),
        ))
    }
    fn c_hincrby(&mut self, _: u64, a: &[B]) -> R {
        let d = pi64(&a[3])?;
        let cur = match self.get_hash(&a[1], false)?.and_then(|h| h.get(&a[2])) {
            Some(v) => {
                parse_dec(v).ok_or_else(|| Frame::err("ERR hash value is not an integer"))?
            }
            None => 0,
        };
        let n = cur
            .checked_add(d)
            .ok_or_else(|| Frame::err("ERR increment or decrement would overflow"))?;
        self.get_hash(&a[1], true)?
            .unwrap()
            .insert(a[2].clone(), n.to_string().into_bytes());
        self.after_write(&a[1]);
        int(n)
    }
    fn c_hlen(&mut self, _: u64, a: &[B]) -> R {
        int(self.get_hash(&a[1], false)?.map_or(0, |h| h.len()) as i64)
    }
    fn c_hexists(&mut self, _: u64, a: &[B]) -> R {
        int(self
            .get_hash(&a[1], false)?
            .is_some_and(|h| h.contains_key(&a[2])) as i64)
    }
    fn c_hkeys(&mut self, _: u64, a: &[B]) -> R {
        fr(Frame::bulks(
            self.get_hash(&a[1], false)?
                .map(|h| h.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        ))
    }
    fn c_hvals(&mut self, _: u64, a: &[B]) -> R {
        fr(Frame::bulks(
            self.get_hash(&a[1], false)?
                .map(|h| h.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        ))
    }

    // ---------------------------------------------------------- sets

    fn c_sadd(&mut self, _: u64, a: &[B]) -> R {
        let s = self.get_set(&a[1], true)?.unwrap();
        let n = a[2..].iter().filter(|m| s.insert((*m).clone())).count();
        self.after_write(&a[1]);
        int(n as i64)
    }
    fn c_srem(&mut self, _: u64, a: &[B]) -> R {
        let Some(s) = self.get_set(&a[1], false)? else {
            return int(0);
        };
        let n = a[2..].iter().filter(|m| s.remove(m.as_slice())).count();
        if n > 0 {
            self.after_write(&a[1]);
        }
        int(n as i64)
    }
    fn c_smembers(&mut self, _: u64, a: &[B]) -> R {
        fr(Frame::bulks(
            self.get_set(&a[1], false)?
                .map(|s| s.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        ))
    }
    fn c_sismember(&mut self, _: u64, a: &[B]) -> R {
        int(self
            .get_set(&a[1], false)?
            .is_some_and(|s| s.contains(&a[2])) as i64)
    }
    fn c_scard(&mut self, _: u64, a: &[B]) -> R {
        int(self.get_set(&a[1], false)?.map_or(0, |s| s.len()) as i64)
    }
    fn set_op(&mut self, a: &[B], op: u8) -> R {
        let mut sets = Vec::new();
        for k in &a[1..] {
            sets.push(self.get_set(k, false)?.cloned().unwrap_or_default());
        }
        let mut it = sets.into_iter();
        let first = it.next().unwrap();
        let res = it.fold(first, |acc, s| match op {
            b'i' => acc.intersection(&s).cloned().collect(),
            b'u' => acc.union(&s).cloned().collect(),
            _ => acc.difference(&s).cloned().collect(),
        });
        fr(Frame::bulks(res))
    }
    fn c_sinter(&mut self, _: u64, a: &[B]) -> R {
        self.set_op(a, b'i')
    }
    fn c_sunion(&mut self, _: u64, a: &[B]) -> R {
        self.set_op(a, b'u')
    }
    fn c_sdiff(&mut self, _: u64, a: &[B]) -> R {
        self.set_op(a, b'd')
    }

    // ---------------------------------------------------------- sorted sets

    fn c_zadd(&mut self, _: u64, a: &[B]) -> R {
        if a.len() % 2 != 0 {
            return Err(syntax());
        }
        let pairs = a[2..]
            .chunks(2)
            .map(|p| Ok((pf64(&p[0])?, &p[1])))
            .collect::<Result<Vec<_>, Frame>>()?;
        let z = self.get_zset(&a[1], true)?.unwrap();
        let added = pairs.iter().filter(|(s, m)| z.add(m, *s)).count();
        self.after_write(&a[1]);
        int(added as i64)
    }
    fn c_zrem(&mut self, _: u64, a: &[B]) -> R {
        let Some(z) = self.get_zset(&a[1], false)? else {
            return int(0);
        };
        let n = a[2..].iter().filter(|m| z.remove(m)).count();
        if n > 0 {
            self.after_write(&a[1]);
        }
        int(n as i64)
    }
    fn zrange(&mut self, a: &[B], rev: bool) -> R {
        let (s, e) = (pi64(&a[2])?, pi64(&a[3])?);
        let withscores = match a.get(4).map(|x| up(x)) {
            None => false,
            Some(w) if w == "WITHSCORES" && a.len() == 5 => true,
            _ => return Err(syntax()),
        };
        let Some(z) = self.get_zset(&a[1], false)? else {
            return fr(Frame::Array(vec![]));
        };
        let items = norm_range(s, e, z.len())
            .map(|(s, e)| z.range(s, e, rev))
            .unwrap_or_default();
        fr(Frame::bulks(items.into_iter().flat_map(|(m, sc)| {
            if withscores {
                vec![m, fmt_score(sc).into_bytes()]
            } else {
                vec![m]
            }
        })))
    }
    fn c_zrange(&mut self, _: u64, a: &[B]) -> R {
        self.zrange(a, false)
    }
    fn c_zrevrange(&mut self, _: u64, a: &[B]) -> R {
        self.zrange(a, true)
    }
    fn c_zrangebyscore(&mut self, _: u64, a: &[B]) -> R {
        let (min, max) = (pbound(&a[2])?, pbound(&a[3])?);
        let (mut withscores, mut limit, mut i) = (false, (0usize, usize::MAX), 4);
        while i < a.len() {
            match up(&a[i]).as_str() {
                "WITHSCORES" => withscores = true,
                "LIMIT" if i + 2 < a.len() => {
                    let (off, cnt) = (pi64(&a[i + 1])?, pi64(&a[i + 2])?);
                    limit = (
                        off.max(0) as usize,
                        if cnt < 0 { usize::MAX } else { cnt as usize },
                    );
                    i += 2;
                }
                _ => return Err(syntax()),
            }
            i += 1;
        }
        let Some(z) = self.get_zset(&a[1], false)? else {
            return fr(Frame::Array(vec![]));
        };
        let items = z
            .range_by_score(min, max)
            .into_iter()
            .skip(limit.0)
            .take(limit.1);
        fr(Frame::bulks(items.flat_map(|(m, sc)| {
            if withscores {
                vec![m, fmt_score(sc).into_bytes()]
            } else {
                vec![m]
            }
        })))
    }
    fn c_zcount(&mut self, _: u64, a: &[B]) -> R {
        let (min, max) = (pbound(&a[2])?, pbound(&a[3])?);
        int(self
            .get_zset(&a[1], false)?
            .map_or(0, |z| z.range_by_score(min, max).len()) as i64)
    }
    fn c_zrank(&mut self, _: u64, a: &[B]) -> R {
        fr(self
            .get_zset(&a[1], false)?
            .and_then(|z| z.rank(&a[2], false))
            .map_or(Frame::Null, |r| Frame::Int(r as i64)))
    }
    fn c_zrevrank(&mut self, _: u64, a: &[B]) -> R {
        fr(self
            .get_zset(&a[1], false)?
            .and_then(|z| z.rank(&a[2], true))
            .map_or(Frame::Null, |r| Frame::Int(r as i64)))
    }
    fn c_zscore(&mut self, _: u64, a: &[B]) -> R {
        fr(self
            .get_zset(&a[1], false)?
            .and_then(|z| z.dict.get(&a[2]).copied())
            .map_or(Frame::Null, |s| Frame::bulk(fmt_score(s))))
    }
    fn c_zincrby(&mut self, _: u64, a: &[B]) -> R {
        let d = pf64(&a[2])?;
        let old = self
            .get_zset(&a[1], false)?
            .and_then(|z| z.dict.get(&a[3]).copied())
            .unwrap_or(0.0);
        let n = old + d;
        if n.is_nan() {
            return Err(Frame::err("ERR resulting score is not a number (NaN)"));
        }
        self.get_zset(&a[1], true)?.unwrap().add(&a[3], n);
        self.after_write(&a[1]);
        fr(Frame::bulk(fmt_score(n)))
    }
    fn c_zcard(&mut self, _: u64, a: &[B]) -> R {
        int(self.get_zset(&a[1], false)?.map_or(0, |z| z.len()) as i64)
    }

    // ---------------------------------------------------------- pub/sub

    fn sub_count(&mut self, c: u64) -> i64 {
        let s = self.client(c);
        (s.subs.len() + s.psubs.len()) as i64
    }

    fn c_subscribe(&mut self, c: u64, a: &[B]) -> R {
        let mut out = vec![];
        for ch in &a[1..] {
            self.client(c).subs.insert(ch.clone());
            self.channels.entry(ch.clone()).or_default().insert(c);
            let n = self.sub_count(c);
            out.push(Frame::Array(vec![
                Frame::bulk("subscribe"),
                Frame::bulk(ch),
                Frame::Int(n),
            ]));
        }
        Ok(Out::Multi(out))
    }
    fn c_psubscribe(&mut self, c: u64, a: &[B]) -> R {
        let mut out = vec![];
        for p in &a[1..] {
            self.client(c).psubs.insert(p.clone());
            self.patterns.entry(p.clone()).or_default().insert(c);
            let n = self.sub_count(c);
            out.push(Frame::Array(vec![
                Frame::bulk("psubscribe"),
                Frame::bulk(p),
                Frame::Int(n),
            ]));
        }
        Ok(Out::Multi(out))
    }
    fn unsub(&mut self, c: u64, a: &[B], pattern: bool) -> R {
        let held = if pattern {
            self.client(c).psubs.clone()
        } else {
            self.client(c).subs.clone()
        };
        let targets: Vec<B> = if a.len() > 1 {
            a[1..].to_vec()
        } else {
            held.into_iter().collect()
        };
        let kind = if pattern {
            "punsubscribe"
        } else {
            "unsubscribe"
        };
        let mut out = vec![];
        for t in &targets {
            let cs = self.clients.entry(c).or_default();
            let (set, idx) = if pattern {
                (&mut cs.psubs, &mut self.patterns)
            } else {
                (&mut cs.subs, &mut self.channels)
            };
            set.remove(t);
            if let Some(m) = idx.get_mut(t) {
                m.remove(&c);
                if m.is_empty() {
                    idx.remove(t);
                }
            }
            let n = self.sub_count(c);
            out.push(Frame::Array(vec![
                Frame::bulk(kind),
                Frame::bulk(t),
                Frame::Int(n),
            ]));
        }
        if targets.is_empty() {
            out.push(Frame::Array(vec![
                Frame::bulk(kind),
                Frame::Null,
                Frame::Int(0),
            ]));
        }
        Ok(Out::Multi(out))
    }
    fn c_unsubscribe(&mut self, c: u64, a: &[B]) -> R {
        self.unsub(c, a, false)
    }
    fn c_punsubscribe(&mut self, c: u64, a: &[B]) -> R {
        self.unsub(c, a, true)
    }
    fn unsubscribe_all(&mut self, c: u64) {
        for idx in [&mut self.channels, &mut self.patterns] {
            idx.retain(|_, set| {
                set.remove(&c);
                !set.is_empty()
            });
        }
        if let Some(cl) = self.clients.get_mut(&c) {
            cl.subs.clear();
            cl.psubs.clear();
        }
    }

    /// Fan out. `try_send` into each subscriber's BOUNDED queue never blocks the executor; a full queue means the
    /// subscriber (or its socket) cannot keep up, so it is disconnected instead of letting memory grow without bound.
    fn c_publish(&mut self, _: u64, a: &[B]) -> R {
        let mut targets: Vec<(u64, Frame)> = vec![];
        for c in self.channels.get(&a[1]).into_iter().flatten() {
            targets.push((
                *c,
                Frame::Array(vec![
                    Frame::bulk("message"),
                    Frame::bulk(&a[1]),
                    Frame::bulk(&a[2]),
                ]),
            ));
        }
        for (pat, cs) in &self.patterns {
            if Glob::new(pat).is_match(&a[1]) {
                for c in cs {
                    let m = Frame::Array(vec![
                        Frame::bulk("pmessage"),
                        Frame::bulk(pat),
                        Frame::bulk(&a[1]),
                        Frame::bulk(&a[2]),
                    ]);
                    targets.push((*c, m));
                }
            }
        }
        let (mut delivered, mut kill) = (0, vec![]);
        for (c, msg) in targets {
            match self
                .clients
                .get(&c)
                .and_then(|s| s.push.as_ref())
                .map(|p| p.try_send(msg))
            {
                Some(Ok(())) => delivered += 1,
                Some(Err(_)) => kill.push(c),
                None => {}
            }
        }
        for c in kill {
            self.kill_client(c);
        }
        int(delivered)
    }

    fn kill_client(&mut self, c: u64) {
        self.unsubscribe_all(c);
        if let Some(cl) = self.clients.get_mut(&c) {
            cl.push = None;
            if let Some(k) = cl.kill.take() {
                k.notify_one();
            }
            self.stats.slow_kills += 1;
        }
    }

    // ---------------------------------------------------------- transactions

    fn c_multi(&mut self, c: u64, _: &[B]) -> R {
        let cl = self.client(c);
        if cl.multi.is_some() {
            return Err(Frame::err("ERR MULTI calls can not be nested"));
        }
        cl.multi = Some(vec![]);
        fr(Frame::ok())
    }

    fn unwatch(&mut self, c: u64) -> bool {
        let keys = std::mem::take(&mut self.client(c).watched);
        for k in keys {
            if let Some(s) = self.watchers.get_mut(&k) {
                s.remove(&c);
                if s.is_empty() {
                    self.watchers.remove(&k);
                }
            }
        }
        self.dirty_clients.remove(&c)
    }

    /// WATCH is optimistic locking: any write to a watched key (even by this client's own earlier command) marks the
    /// client dirty and the next EXEC replies nil without running anything.
    fn c_watch(&mut self, c: u64, a: &[B]) -> R {
        if self.client(c).multi.is_some() {
            return Err(Frame::err("ERR WATCH inside MULTI is not allowed"));
        }
        for k in &a[1..] {
            self.expire_if_needed(k);
            self.watchers.entry(k.clone()).or_default().insert(c);
            self.client(c).watched.push(k.clone());
        }
        fr(Frame::ok())
    }
    fn c_unwatch(&mut self, c: u64, _: &[B]) -> R {
        self.unwatch(c);
        fr(Frame::ok())
    }
    fn c_discard(&mut self, c: u64, _: &[B]) -> R {
        let cl = self.client(c);
        if cl.multi.take().is_none() {
            return Err(Frame::err("ERR DISCARD without MULTI"));
        }
        cl.multi_err = false;
        self.unwatch(c);
        fr(Frame::ok())
    }

    /// EXEC runs the queue on the single executor thread, so nothing can interleave: isolation for free.
    /// There is no rollback; a command that fails inside the transaction fails alone (like Redis).
    fn c_exec(&mut self, c: u64, _: &[B]) -> R {
        let cl = self.client(c);
        let Some(queue) = cl.multi.take() else {
            return Err(Frame::err("ERR EXEC without MULTI"));
        };
        let bad = std::mem::take(&mut cl.multi_err);
        let was_dirty = self.unwatch(c);
        if bad {
            return Err(Frame::err(
                "EXECABORT Transaction discarded because of previous errors.",
            ));
        }
        if was_dirty {
            return fr(Frame::NullArray);
        }
        let mark = self.log.len();
        self.in_exec = true;
        let mut replies = Vec::with_capacity(queue.len());
        for cmd in &queue {
            replies.push(match self.execute(c, cmd) {
                Out::R(f) => f,
                Out::Multi(v) => Frame::Array(v),
                _ => Frame::Null,
            });
        }
        self.in_exec = false;
        if self.log.len() - mark > 1 {
            // persist the transaction as one unit: a torn tail then drops all of it (replay discards an unfinished MULTI)
            self.log.insert(mark, vec![b"MULTI".to_vec()]);
            self.log.push(vec![b"EXEC".to_vec()]);
        }
        self.custom = true;
        fr(Frame::Array(replies))
    }

    // ---------------------------------------------------------- server

    fn c_ping(&mut self, c: u64, a: &[B]) -> R {
        if a.len() > 2 {
            return Err(Frame::err(
                "ERR wrong number of arguments for 'ping' command",
            ));
        }
        if self.sub_count(c) > 0 {
            return fr(Frame::bulks([
                b"pong".to_vec(),
                a.get(1).cloned().unwrap_or_default(),
            ]));
        }
        fr(match a.get(1) {
            Some(m) => Frame::bulk(m),
            None => Frame::Simple("PONG".into()),
        })
    }
    fn c_echo(&mut self, _: u64, a: &[B]) -> R {
        fr(Frame::bulk(&a[1]))
    }
    fn c_quit(&mut self, _: u64, _: &[B]) -> R {
        Ok(Out::Quit)
    }
    fn c_command(&mut self, _: u64, a: &[B]) -> R {
        match a.get(1).map(|x| up(x)).as_deref() {
            None => fr(Frame::bulks(
                TABLE.iter().map(|c| c.name.to_lowercase().into_bytes()),
            )),
            Some("COUNT") => int(TABLE.len() as i64),
            _ => Err(Frame::err("ERR unknown subcommand for COMMAND")),
        }
    }

    fn c_info(&mut self, _: u64, _: &[B]) -> R {
        let s = &self.stats;
        let (aof_on, aof_size) = self.aof.as_ref().map_or((0, 0), |a| (1, a.size));
        let text = format!(
            "# Server\r\nredis_version:0.1.0-mini\r\nuptime_in_seconds:{}\r\n\
             # Clients\r\nconnected_clients:{}\r\nblocked_clients:{}\r\ntotal_connections_received:{}\r\n\
             # Stats\r\ntotal_commands_processed:{}\r\nexpired_keys:{}\r\nslow_subscriber_disconnects:{}\r\n\
             # Persistence\r\naof_enabled:{aof_on}\r\naof_current_size:{aof_size}\r\naof_fsyncs:{}\r\naof_rewrites:{}\r\n\
             aof_rewrite_in_progress:{}\r\naof_truncated_bytes:{}\r\nrdb_saves:{}\r\nrdb_bgsave_in_progress:{}\r\n\
             rdb_changes_since_last_save:{}\r\n# Keyspace\r\ndb0:keys={},expires={}\r\n",
            self.started.elapsed().as_secs(),
            self.clients.len(),
            self.blocked.len(),
            s.connections,
            s.commands,
            s.expired,
            s.slow_kills,
            s.fsyncs,
            s.rewrites,
            self.bg_rewrite as u8,
            s.aof_truncated,
            s.saves,
            self.bg_save as u8,
            self.dirty - self.saved_at_dirty,
            self.map.len(),
            self.expires.len(),
        );
        fr(Frame::bulk(text))
    }

    /// DEBUG SWEEP: delete every expired key now. DEBUG EXPIRE-CYCLE: one uncapped sampling cycle (tests use these
    /// to run the active-expiry algorithm deterministically instead of racing a timer).
    fn c_debug(&mut self, _: u64, a: &[B]) -> R {
        match up(&a[1]).as_str() {
            "SWEEP" => {
                let dead: Vec<B> = self
                    .expires
                    .items
                    .iter()
                    .filter(|(_, t)| *t <= self.now)
                    .map(|(k, _)| k.clone())
                    .collect();
                dead.iter().for_each(|k| self.delete_expired(k));
                int(dead.len() as i64)
            }
            "EXPIRE-CYCLE" => {
                let n = self.active_expire_cycle(10_000, None);
                int(n as i64)
            }
            _ => Err(Frame::err("ERR unknown DEBUG subcommand")),
        }
    }
}

// ================================================================ 7. persistence: AOF (log + rewrite) and RDB-style snapshots

struct Snapshot {
    map: BTreeMap<B, Value>,
    exp: HashMap<B, u64>,
}

/// Clone the live (non-expired) keyspace. Real Redis `fork()`s and lets the kernel copy-on-write the pages; a Rust
/// process without fork pays an O(n) clone on the executor thread, then serializes on a background thread.
fn snapshot_of(e: &Engine) -> Snapshot {
    let live = |k: &B| !e.expires.get(k).is_some_and(|t| t <= e.now);
    Snapshot {
        map: e
            .map
            .iter()
            .filter(|(k, _)| live(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        exp: e
            .expires
            .items
            .iter()
            .filter(|(k, _)| live(k))
            .cloned()
            .collect(),
    }
}

/// The smallest command stream that rebuilds the snapshot: this is what AOF REWRITE writes.
fn snapshot_to_aof(s: &Snapshot) -> Vec<u8> {
    let mut o = Vec::new();
    let mut put = |name: &str, key: &B, rest: Vec<B>| {
        let mut args = vec![name.as_bytes().to_vec(), key.clone()];
        args.extend(rest);
        o.extend(encode_cmd(&args));
    };
    for (k, v) in &s.map {
        match v {
            Value::Str(x) => put("SET", k, vec![x.clone()]),
            Value::List(l) => l
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .chunks(64)
                .for_each(|c| put("RPUSH", k, c.to_vec())),
            Value::Hash(h) => h.iter().collect::<Vec<_>>().chunks(32).for_each(|c| {
                put(
                    "HSET",
                    k,
                    c.iter()
                        .flat_map(|(f, x)| [(*f).clone(), (*x).clone()])
                        .collect(),
                )
            }),
            Value::Set(m) => m
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .chunks(64)
                .for_each(|c| put("SADD", k, c.to_vec())),
            Value::ZSet(z) => z.items().chunks(32).for_each(|c| {
                put(
                    "ZADD",
                    k,
                    c.iter()
                        .flat_map(|(m, sc)| [fmt_score(*sc).into_bytes(), m.clone()])
                        .collect(),
                )
            }),
        }
        if let Some(t) = s.exp.get(k) {
            put("PEXPIREAT", k, vec![t.to_string().into_bytes()]);
        }
    }
    o
}

const RDB_MAGIC: &[u8] = b"MRDB\x00\x01";

/// Binary dump: magic+version, entries `[type][has_ttl][ttl u64]?[key][value]`, 0xFF, then CRC32 of everything before it.
fn rdb_encode(s: &Snapshot) -> Vec<u8> {
    fn blob(o: &mut Vec<u8>, b: &[u8]) {
        o.extend((b.len() as u32).to_le_bytes());
        o.extend_from_slice(b);
    }
    let mut o = RDB_MAGIC.to_vec();
    for (k, v) in &s.map {
        let ty = match v {
            Value::Str(_) => 0,
            Value::List(_) => 1,
            Value::Hash(_) => 2,
            Value::Set(_) => 3,
            Value::ZSet(_) => 4,
        };
        o.push(ty);
        match s.exp.get(k) {
            Some(t) => {
                o.push(1);
                o.extend(t.to_le_bytes());
            }
            None => o.push(0),
        }
        blob(&mut o, k);
        match v {
            Value::Str(x) => blob(&mut o, x),
            Value::List(l) => {
                o.extend((l.len() as u32).to_le_bytes());
                l.iter().for_each(|x| blob(&mut o, x));
            }
            Value::Hash(h) => {
                o.extend((h.len() as u32).to_le_bytes());
                h.iter().for_each(|(f, x)| {
                    blob(&mut o, f);
                    blob(&mut o, x)
                });
            }
            Value::Set(m) => {
                o.extend((m.len() as u32).to_le_bytes());
                m.iter().for_each(|x| blob(&mut o, x));
            }
            Value::ZSet(z) => {
                o.extend((z.len() as u32).to_le_bytes());
                z.items().iter().for_each(|(m, sc)| {
                    o.extend(sc.to_bits().to_le_bytes());
                    blob(&mut o, m)
                });
            }
        }
    }
    o.push(0xFF);
    o.extend(crc32(&o).to_le_bytes());
    o
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .p
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or("truncated snapshot")?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn blob(&mut self) -> Result<B, String> {
        let n = self.u32()? as usize; // a lying length fails in `take` before anything is allocated
        Ok(self.take(n)?.to_vec())
    }
}

fn rdb_decode(data: &[u8]) -> Result<Snapshot, String> {
    if data.len() < RDB_MAGIC.len() + 5 || &data[..RDB_MAGIC.len()] != RDB_MAGIC {
        return Err("bad magic".into());
    }
    let (body, tail) = data.split_at(data.len() - 4);
    if crc32(body).to_le_bytes() != tail {
        return Err("checksum mismatch".into());
    }
    let mut r = Rd {
        b: body,
        p: RDB_MAGIC.len(),
    };
    let mut s = Snapshot {
        map: BTreeMap::new(),
        exp: HashMap::new(),
    };
    loop {
        let ty = r.take(1)?[0];
        if ty == 0xFF {
            break;
        }
        let ttl = (r.take(1)?[0] == 1).then(|| r.u64()).transpose()?;
        let key = r.blob()?;
        let v = match ty {
            0 => Value::Str(r.blob()?),
            1 => Value::List((0..r.u32()?).map(|_| r.blob()).collect::<Result<_, _>>()?),
            2 => Value::Hash(
                (0..r.u32()?)
                    .map(|_| Ok((r.blob()?, r.blob()?)))
                    .collect::<Result<_, String>>()?,
            ),
            3 => Value::Set((0..r.u32()?).map(|_| r.blob()).collect::<Result<_, _>>()?),
            4 => {
                let mut z = ZSet::default();
                for _ in 0..r.u32()? {
                    let sc = f64::from_bits(r.u64()?);
                    z.add(&r.blob()?, sc);
                }
                Value::ZSet(z)
            }
            t => return Err(format!("unknown type {t}")),
        };
        if let Some(t) = ttl {
            s.exp.insert(key.clone(), t);
        }
        s.map.insert(key, v);
    }
    Ok(s)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(&tmp, path) // readers see the old file or the complete new one, never a torn mix
}

impl Engine {
    fn load_persistence(&mut self) -> io::Result<()> {
        let mut loaded = false;
        if let Some((p, _)) = self.cfg.aof.clone()
            && p.exists()
            && fs::metadata(&p)?.len() > 0
        {
            self.load_aof(&p)?;
            loaded = true;
        }
        if !loaded
            && let Some(r) = self.cfg.rdb.clone()
            && r.exists()
        {
            let snap = rdb_decode(&fs::read(&r)?)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            for (k, t) in &snap.exp {
                self.expires.set(k, *t);
            }
            self.map = snap.map;
            if let Some((p, _)) = &self.cfg.aof {
                fs::write(p, snapshot_to_aof(&snapshot_of(self)))?; // seed the AOF from the snapshot
            }
        }
        if let Some((p, policy)) = self.cfg.aof.clone() {
            let file = OpenOptions::new().create(true).append(true).open(&p)?;
            let size = file.metadata()?.len();
            self.aof = Some(Aof {
                file,
                policy,
                size,
                unsynced: false,
                last_sync: Instant::now(),
            });
        }
        self.dirty = 0;
        self.stats = Stats {
            aof_truncated: self.stats.aof_truncated,
            ..Stats::default()
        };
        Ok(())
    }

    /// Replay: run each record through the normal dispatcher on a fake client (id 0). A torn tail (crash mid-append)
    /// ends the replay; the file is truncated back to the last complete record so new appends start clean. An
    /// unfinished MULTI at the tail is dropped too, so a transaction is all-or-nothing across crashes.
    fn load_aof(&mut self, path: &Path) -> io::Result<()> {
        let data = fs::read(path)?;
        let lim = Limits {
            max_bulk: 512 << 20,
            max_array: 1 << 24,
            ..Limits::default()
        };
        self.loading = true;
        let (mut off, mut multi_start) = (0usize, None);
        while data.get(off) == Some(&b'*') {
            let Ok(Some((args, used))) = parse_command(&data[off..], &lim) else {
                break;
            };
            if let Some(first) = args.first() {
                match up(first).as_str() {
                    "MULTI" => multi_start = Some(off),
                    "EXEC" | "DISCARD" => multi_start = None,
                    _ => {}
                }
                self.now = self.clock.now_ms();
                self.dispatch(0, &args);
            }
            self.log.clear();
            self.ready.clear();
            off += used;
        }
        self.loading = false;
        self.clients.remove(&0);
        let valid = multi_start.unwrap_or(off);
        if valid < data.len() {
            OpenOptions::new()
                .write(true)
                .open(path)?
                .set_len(valid as u64)?;
            self.stats.aof_truncated = (data.len() - valid) as u64;
        }
        Ok(())
    }

    /// End of every loop iteration: append this iteration's records, then fsync according to the policy. Because
    /// replies are released only AFTER this, `Always` means "acknowledged implies durable".
    fn flush_aof(&mut self) {
        if self.log.is_empty() {
            return;
        }
        let mut bytes = Vec::new();
        for rec in self.log.drain(..) {
            bytes.extend(encode_cmd(&rec));
        }
        if let Some(rb) = &mut self.rewrite_buf {
            rb.extend_from_slice(&bytes);
        }
        if let Some(a) = &mut self.aof
            && a.file.write_all(&bytes).is_ok()
        {
            a.size += bytes.len() as u64;
            match a.policy {
                Fsync::Always => {
                    let _ = a.file.sync_data();
                    self.stats.fsyncs += 1;
                }
                Fsync::EverySec => a.unsynced = true,
                Fsync::No => {}
            }
        }
    }

    /// `everysec`: at most one fsync per second, on a helper thread so the executor never waits for the disk.
    fn everysec_sync(&mut self) {
        if let Some(a) = &mut self.aof
            && a.policy == Fsync::EverySec
            && a.unsynced
            && a.last_sync.elapsed() >= Duration::from_secs(1)
        {
            (a.unsynced, a.last_sync) = (false, Instant::now());
            self.stats.fsyncs += 1;
            if let Ok(f) = a.file.try_clone() {
                std::thread::spawn(move || {
                    let _ = f.sync_data();
                });
            }
        }
    }

    fn c_save(&mut self, _: u64, _: &[B]) -> R {
        let Some(p) = self.cfg.rdb.clone() else {
            return Err(Frame::err("ERR RDB persistence is not configured"));
        };
        write_atomic(&p, &rdb_encode(&snapshot_of(self)))
            .map_err(|e| Frame::err(format!("ERR {e}")))?;
        self.stats.saves += 1;
        self.saved_at_dirty = self.dirty;
        fr(Frame::ok())
    }

    fn c_bgsave(&mut self, _: u64, _: &[B]) -> R {
        let Some(p) = self.cfg.rdb.clone() else {
            return Err(Frame::err("ERR RDB persistence is not configured"));
        };
        if self.bg_save {
            return Err(Frame::err("ERR Background save already in progress"));
        }
        self.bg_save = true;
        self.saved_at_dirty = self.dirty;
        let (snap, tx) = (snapshot_of(self), self.tx.clone());
        std::thread::spawn(move || {
            let r = write_atomic(&p, &rdb_encode(&snap)).map_err(|e| e.to_string());
            let _ = tx.send(Msg::BgDone(Bg::Save, r));
        });
        fr(Frame::Simple("Background saving started".into()))
    }

    /// Background AOF rewrite: snapshot now, write the compact file on a thread, and meanwhile keep appending every new
    /// write to BOTH the old AOF and `rewrite_buf`. When the thread finishes, the buffered tail is appended to the new
    /// file and it is renamed over the old one. Nothing is lost whatever the interleaving.
    fn c_bgrewriteaof(&mut self, _: u64, _: &[B]) -> R {
        let Some(path) = self.cfg.aof.as_ref().map(|(p, _)| p.clone()) else {
            return Err(Frame::err("ERR AOF is not enabled"));
        };
        if self.bg_rewrite {
            return Err(Frame::err(
                "ERR Background append only file rewriting already in progress",
            ));
        }
        self.flush_aof(); // records executed before this command belong to the OLD file only (they are in the snapshot)
        self.bg_rewrite = true;
        self.rewrite_buf = Some(Vec::new());
        let (snap, tx) = (snapshot_of(self), self.tx.clone());
        std::thread::spawn(move || {
            let r = fs::write(path.with_extension("rewrite"), snapshot_to_aof(&snap))
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::BgDone(Bg::Rewrite, r));
        });
        fr(Frame::Simple(
            "Background append only file rewriting started".into(),
        ))
    }

    fn bg_done(&mut self, kind: Bg, res: Result<(), String>) {
        match kind {
            Bg::Save => {
                self.bg_save = false;
                self.stats.saves += res.is_ok() as u64;
            }
            Bg::Rewrite => {
                self.bg_rewrite = false;
                let tail = self.rewrite_buf.take().unwrap_or_default();
                let Some((path, _)) = self.cfg.aof.clone() else {
                    return;
                };
                let tmp = path.with_extension("rewrite");
                let swap = || -> io::Result<File> {
                    res.clone().map_err(io::Error::other)?;
                    let mut f = OpenOptions::new().append(true).open(&tmp)?;
                    f.write_all(&tail)?;
                    f.sync_all()?;
                    fs::rename(&tmp, &path)?;
                    OpenOptions::new().append(true).open(&path)
                };
                if let Ok(file) = swap() {
                    let size = file.metadata().map_or(0, |m| m.len());
                    if let Some(a) = &mut self.aof {
                        (a.file, a.size) = (file, size);
                    }
                    self.stats.rewrites += 1;
                }
            }
        }
    }
}

// ================================================================ 8. TCP server: one task per connection, one executor thread

struct Server {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    tx: xc::Sender<Msg>,
    accept: tokio::task::JoinHandle<()>,
    exec: std::thread::JoinHandle<()>,
}

async fn start(cfg: Config, clock: Arc<dyn Clock>) -> io::Result<Server> {
    let (tx, rx) = xc::unbounded();
    let engine = Engine::new(cfg.clone(), clock, tx.clone())?; // loads the AOF/RDB before accepting connections
    let exec = std::thread::Builder::new()
        .name("executor".into())
        .spawn(move || engine.run(rx))?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (stop, stop_rx) = watch::channel(false);
    let accept = tokio::spawn(accept_loop(listener, tx.clone(), stop_rx, cfg));
    Ok(Server {
        addr,
        stop,
        tx,
        accept,
        exec,
    })
}

impl Server {
    /// Graceful: stop accepting, close connections, let the executor flush + fsync the AOF, join its thread.
    async fn shutdown(self) -> io::Result<()> {
        let _ = self.stop.send(true);
        tokio::time::timeout(Duration::from_secs(8), self.accept)
            .await
            .map_err(|_| io::Error::other("accept drain timed out"))??;
        let (ack, ack_rx) = oneshot::channel();
        let _ = self.tx.send(Msg::Shutdown(ack));
        tokio::time::timeout(Duration::from_secs(5), ack_rx)
            .await
            .map_err(|_| io::Error::other("executor ack timed out"))?
            .ok();
        let exec = self.exec;
        tokio::task::spawn_blocking(move || exec.join())
            .await?
            .map_err(|_| io::Error::other("executor panicked"))
    }
}

async fn accept_loop(
    listener: TcpListener,
    tx: xc::Sender<Msg>,
    mut stop: watch::Receiver<bool>,
    cfg: Config,
) {
    let (mut conns, mut next_id) = (JoinSet::new(), 1u64);
    loop {
        tokio::select! {
            r = listener.accept() => {
                let Ok((s, _)) = r else { continue };
                conns.spawn(serve_conn(s, next_id, tx.clone(), stop.clone(), cfg.clone()));
                next_id += 1;
            }
            _ = stop.changed() => break,
            Some(_) = conns.join_next(), if !conns.is_empty() => {}
        }
    }
    drop(listener);
    let drain = async { while conns.join_next().await.is_some() {} };
    if tokio::time::timeout(Duration::from_secs(3), drain)
        .await
        .is_err()
    {
        conns.abort_all();
    }
}

async fn serve_conn(
    stream: TcpStream,
    id: u64,
    tx: xc::Sender<Msg>,
    mut stop: watch::Receiver<bool>,
    cfg: Config,
) {
    let _ = stream.set_nodelay(true);
    let (push_tx, mut push_rx) = mpsc::channel(cfg.push_buffer);
    let kill = Arc::new(Notify::new());
    let _ = tx.send(Msg::Connect {
        id,
        push: push_tx,
        kill: kill.clone(),
    });
    let _ = conn_loop(stream, id, &tx, &mut push_rx, &kill, &mut stop, &cfg.limits).await;
    let _ = tx.send(Msg::Disconnect(id));
}

async fn write_guarded(
    wr: &mut tokio::net::tcp::OwnedWriteHalf,
    out: &[u8],
    kill: &Notify,
) -> io::Result<()> {
    tokio::select! {
        r = wr.write_all(out) => r,
        _ = kill.notified() => Err(io::ErrorKind::ConnectionAborted.into()),
    }
}

/// Per connection: read -> parse every complete command in the buffer (pipelining) -> ONE message to the executor ->
/// write all replies in one `write`. While the executor works (or the client is blocked in BLPOP) we keep buffering
/// input, up to a cap, and keep watching for disconnect.
async fn conn_loop(
    stream: TcpStream,
    id: u64,
    tx: &xc::Sender<Msg>,
    push_rx: &mut mpsc::Receiver<Frame>,
    kill: &Notify,
    stop: &mut watch::Receiver<bool>,
    lim: &Limits,
) -> io::Result<()> {
    const MAX_PENDING: usize = 1 << 20;
    let (mut rd, mut wr) = stream.into_split();
    let mut inbuf: Vec<u8> = Vec::with_capacity(4096);
    loop {
        let (mut cmds, mut used_total, mut bad) = (Vec::new(), 0, None);
        loop {
            match parse_command(&inbuf[used_total..], lim) {
                Ok(Some((args, used))) => {
                    used_total += used;
                    if !args.is_empty() {
                        cmds.push(args);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    bad = Some(format!("ERR Protocol error: {e}"));
                    break;
                }
            }
        }
        inbuf.drain(..used_total);
        if !cmds.is_empty() {
            let (rtx, mut rrx) = oneshot::channel();
            tx.send(Msg::Req {
                conn: id,
                cmds,
                reply: rtx,
            })
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
            let reply = loop {
                tokio::select! {
                    r = &mut rrx => break r.map_err(|_| io::ErrorKind::BrokenPipe)?,
                    n = rd.read_buf(&mut inbuf), if inbuf.len() < MAX_PENDING => {
                        if n? == 0 { return Ok(()); }
                    }
                    _ = kill.notified() => return Ok(()),
                    _ = stop.changed() => return Ok(()),
                }
            };
            let mut out = Vec::new();
            reply.frames.iter().for_each(|f| encode(f, &mut out));
            write_guarded(&mut wr, &out, kill).await?;
            if reply.close {
                return Ok(());
            }
            continue;
        }
        if let Some(msg) = bad {
            let mut out = Vec::new();
            encode(&Frame::Error(msg), &mut out);
            write_guarded(&mut wr, &out, kill).await?;
            return Ok(()); // a protocol error poisons the stream: reply, then close
        }
        tokio::select! {
            n = rd.read_buf(&mut inbuf) => {
                if n? == 0 { return Ok(()); }
            }
            Some(m) = push_rx.recv() => {
                let mut out = Vec::new();
                encode(&m, &mut out);
                while out.len() < MAX_PENDING && let Ok(m) = push_rx.try_recv() {
                    encode(&m, &mut out);
                }
                write_guarded(&mut wr, &out, kill).await?;
            }
            _ = kill.notified() => return Ok(()),
            _ = stop.changed() => return Ok(()),
        }
    }
}

// ================================================================ 9. a small async client library (also used by every test)

const IO_TIMEOUT: Duration = Duration::from_secs(10);

fn timed_out(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, format!("{what} timed out"))
}

struct Client {
    s: TcpStream,
    buf: Vec<u8>,
}

impl Client {
    async fn connect(addr: SocketAddr) -> io::Result<Client> {
        let s = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| timed_out("connect"))??;
        s.set_nodelay(true)?;
        Ok(Client { s, buf: Vec::new() })
    }

    async fn send_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        tokio::time::timeout(IO_TIMEOUT, self.s.write_all(bytes))
            .await
            .map_err(|_| timed_out("write"))?
    }

    /// Read exactly one reply frame (replies are parsed incrementally from whatever the socket delivers).
    async fn read_frame(&mut self) -> io::Result<Frame> {
        let lim = Limits {
            max_bulk: 64 << 20,
            max_array: 1 << 24,
            max_depth: 16,
            ..Limits::default()
        };
        loop {
            match parse(&self.buf, &lim) {
                Ok(Some((f, used))) => {
                    self.buf.drain(..used);
                    return Ok(f);
                }
                Ok(None) => {}
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string())),
            }
            let n = tokio::time::timeout(IO_TIMEOUT, self.s.read_buf(&mut self.buf))
                .await
                .map_err(|_| timed_out("read"))??;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }
    }

    async fn cmd<S: AsRef<[u8]>>(&mut self, args: &[S]) -> io::Result<Frame> {
        self.send_raw(&encode_cmd(args)).await?;
        self.read_frame().await
    }

    /// Send every command in ONE write, then read all replies: that is pipelining.
    async fn pipeline<S: AsRef<[u8]>>(&mut self, cmds: &[Vec<S>]) -> io::Result<Vec<Frame>> {
        let mut out = Vec::new();
        cmds.iter().for_each(|c| out.extend(encode_cmd(c)));
        self.send_raw(&out).await?;
        let mut replies = Vec::with_capacity(cmds.len());
        for _ in cmds {
            replies.push(self.read_frame().await?);
        }
        Ok(replies)
    }

    /// true when the server closes the connection (EOF or reset) within the timeout
    async fn closed_by_peer(&mut self) -> bool {
        let mut b = [0u8; 64];
        matches!(
            tokio::time::timeout(Duration::from_secs(5), self.s.read(&mut b)).await,
            Ok(Ok(0)) | Ok(Err(_))
        )
    }

    async fn info(&mut self) -> io::Result<HashMap<String, String>> {
        let text = self.cmd(&["INFO"]).await?.text();
        Ok(text
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect())
    }
    async fn info_u64(&mut self, key: &str) -> u64 {
        self.info()
            .await
            .unwrap()
            .get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("INFO field {key}"))
    }
    /// poll INFO until `key` satisfies `pred` (bounded: panics after 10 s instead of hanging)
    async fn wait_info(&mut self, key: &str, pred: impl Fn(u64) -> bool) -> u64 {
        for _ in 0..1000 {
            let v = self.info_u64(key).await;
            if pred(v) {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("INFO {key} never satisfied the condition");
    }
}

// ---------------------------------------------------------------- tiny helpers for building expected frames

fn b(s: &str) -> Frame {
    Frame::bulk(s)
}
fn n(v: i64) -> Frame {
    Frame::Int(v)
}
fn arr(items: &[&str]) -> Frame {
    Frame::Array(items.iter().map(|s| b(s)).collect())
}
fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("mini_redis_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn join(&self, f: &str) -> PathBuf {
        self.0.join(f)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A plain in-memory server on a controllable clock (no timers, no persistence): the workhorse of the tests.
async fn mem_server() -> (Server, Arc<ManualClock>) {
    let clock = ManualClock::new(1_000_000);
    let cfg = Config {
        active_expire_every: None,
        ..Config::default()
    };
    (start(cfg, clock.clone()).await.unwrap(), clock)
}

/// redis-cli style rendering for the sample session.
fn render(f: &Frame, indent: usize) -> String {
    match f {
        Frame::Simple(s) => s.clone(),
        Frame::Error(e) => format!("(error) {e}"),
        Frame::Int(v) => format!("(integer) {v}"),
        Frame::Bulk(x) => format!("{:?}", String::from_utf8_lossy(x)),
        Frame::Null | Frame::NullArray => "(nil)".into(),
        Frame::Array(v) if v.is_empty() => "(empty array)".into(),
        Frame::Array(v) => v
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let pre = format!("{}) ", i + 1);
                let pad = if i == 0 {
                    String::new()
                } else {
                    " ".repeat(indent)
                };
                format!("{pad}{pre}{}", render(x, indent + pre.len()))
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Canonical fingerprint of the whole database, read back through the public protocol.
async fn dump_all(c: &mut Client) -> Vec<String> {
    let Frame::Array(keys) = c.cmd(&["KEYS", "*"]).await.unwrap() else {
        panic!("KEYS")
    };
    let mut out = vec![];
    for k in keys.iter().map(|k| k.text()) {
        let ty = c.cmd(&["TYPE", &k]).await.unwrap().text();
        let val = match ty.as_str() {
            "string" => c.cmd(&["GET", &k]).await,
            "list" => c.cmd(&["LRANGE", &k, "0", "-1"]).await,
            "hash" => c.cmd(&["HGETALL", &k]).await,
            "set" => c.cmd(&["SMEMBERS", &k]).await,
            _ => c.cmd(&["ZRANGE", &k, "0", "-1", "WITHSCORES"]).await,
        }
        .unwrap();
        let ttl = c.cmd(&["PTTL", &k]).await.unwrap().text();
        out.push(format!(
            "{k} {ty} {} ttl={ttl}",
            render(&val, 0).replace('\n', " ")
        ));
    }
    out
}

// ================================================================ 10. tests: pure components

fn t_glob() {
    let cases: &[(&str, &str, bool)] = &[
        ("h?llo", "hello", true),
        ("h*llo", "hllo", true),
        ("h*llo", "heeeello", true),
        ("h[ae]llo", "hallo", true),
        ("h[^e]llo", "hello", false),
        ("h[a-b]llo", "hbllo", true),
        ("*", "", true),
        ("a*b*c", "aXXbYYc", true),
        ("a*b*c", "aXXbYY", false),
        ("\\*", "*", true),
        ("\\*", "x", false),
        ("news.*", "news.sport", true),
        ("[a-c]?", "bz", true),
    ];
    for (p, s, want) in cases {
        assert_eq!(
            Glob::new(p.as_bytes()).is_match(s.as_bytes()),
            *want,
            "glob {p:?} vs {s:?}"
        );
    }
    let (mut rng, alpha, text) = (Rng(7), b"ab*?[]^-\\c", b"abc");
    for _ in 0..60_000 {
        let p: Vec<u8> = (0..rng.below(8))
            .map(|_| alpha[rng.below(alpha.len() as u64) as usize])
            .collect();
        let t: Vec<u8> = (0..rng.below(9))
            .map(|_| text[rng.below(3) as usize])
            .collect();
        let g = Glob::new(&p);
        assert_eq!(
            g.is_match(&t),
            g.is_match_slow(&t),
            "pattern {:?} text {:?}",
            String::from_utf8_lossy(&p),
            String::from_utf8_lossy(&t)
        );
    }
    // a pathological pattern stays polynomial
    let text = vec![b'a'; 5000];
    assert!(!Glob::new(b"*a*a*a*a*a*a*a*a*b").is_match(&text));
    println!(
        "  glob: 13 fixed cases + 60k fuzz vs exponential reference, pathological '*a*a*...b' on 5000 bytes instant"
    );
}

fn t_skiplist() {
    let (mut rng, mut z, mut model) = (Rng(99), ZSet::default(), BTreeMap::<B, f64>::new());
    let sorted = |m: &BTreeMap<B, f64>| -> Vec<(B, f64)> {
        let mut v: Vec<_> = m.iter().map(|(k, s)| (k.clone(), *s)).collect();
        v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)));
        v
    };
    for step in 0..30_000 {
        let member = format!("m{}", rng.below(300)).into_bytes();
        match rng.below(10) {
            0..=5 => {
                let s = rng.below(41) as f64 - 20.0;
                assert_eq!(z.add(&member, s), model.insert(member.clone(), s).is_none());
            }
            6 | 7 => assert_eq!(z.remove(&member), model.remove(&member).is_some()),
            _ => {
                let order = sorted(&model);
                let want = order.iter().position(|(m, _)| *m == member);
                assert_eq!(z.rank(&member, false), want);
                assert_eq!(z.rank(&member, true), want.map(|r| order.len() - 1 - r));
                if !order.is_empty() {
                    let a = rng.below(order.len() as u64) as usize;
                    let c = a + rng.below((order.len() - a) as u64) as usize;
                    assert_eq!(z.range(a, c, false), order[a..=c].to_vec());
                    let lo = rng.below(41) as f64 - 20.0;
                    let want: Vec<_> = order
                        .iter()
                        .filter(|(_, s)| *s > lo && *s <= lo + 5.0)
                        .cloned()
                        .collect();
                    assert_eq!(z.range_by_score((lo, true), (lo + 5.0, false)), want);
                }
            }
        }
        if step % 1000 == 0 {
            z.sl.check();
        }
    }
    z.sl.check();
    assert_eq!(z.items(), sorted(&model));
    println!(
        "  skiplist: 30k random add/remove/rank/range ops == sorted-vec model; spans verified; {} members, {} levels",
        z.len(),
        z.sl.level
    );
}

fn t_resp_parser() {
    let lim = Limits::default();
    let mut rng = Rng(5);
    fn rand_frame(rng: &mut Rng, depth: u32) -> Frame {
        match rng.below(if depth > 2 { 5 } else { 7 }) {
            0 => Frame::Simple(format!("s{}", rng.below(100))),
            1 => Frame::Error(format!("ERR e{}", rng.below(100))),
            2 => Frame::Int(rng.next() as i64 >> rng.below(60)),
            3 => Frame::Bulk((0..rng.below(40)).map(|_| rng.next() as u8).collect()), // arbitrary bytes incl. CRLF
            4 => Frame::Null,
            5 => Frame::NullArray,
            _ => Frame::Array(
                (0..rng.below(5))
                    .map(|_| rand_frame(rng, depth + 1))
                    .collect(),
            ),
        }
    }
    let mut chunks = 0;
    for _ in 0..500 {
        let f = rand_frame(&mut rng, 0);
        let mut wire = Vec::new();
        encode(&f, &mut wire);
        for cut in 0..wire.len() {
            assert_eq!(
                parse(&wire[..cut], &lim),
                Ok(None),
                "a proper prefix must ask for more bytes"
            );
        }
        // feed in random-sized chunks, exactly like a socket would
        let (mut buf, mut pos, mut got) = (Vec::new(), 0, None);
        while pos < wire.len() {
            let step = 1 + rng.below(7) as usize;
            buf.extend_from_slice(&wire[pos..(pos + step).min(wire.len())]);
            pos += step;
            chunks += 1;
            if let Some((fr, used)) = parse(&buf, &lim).unwrap() {
                assert_eq!(used, buf.len());
                got = Some(fr);
            }
        }
        assert_eq!(got, Some(f));
    }
    // hostile input
    assert_eq!(
        parse(b"$99999999999\r\n", &lim),
        Err(ProtoError::TooLarge("bulk string"))
    );
    assert_eq!(
        parse(b"*99999999\r\n", &lim),
        Err(ProtoError::TooLarge("array"))
    );
    assert_eq!(
        parse(&b"*1\r\n".repeat(100_000), &lim),
        Err(ProtoError::TooDeep)
    );
    assert_eq!(
        parse(&vec![b'+'; 70_000], &lim),
        Err(ProtoError::LineTooLong)
    );
    assert_eq!(parse(b":+5\r\n", &lim), Err(ProtoError::BadInteger));
    assert_eq!(
        parse(b"$3\r\nabcde\r\n", &lim),
        Err(ProtoError::BadTerminator)
    );
    assert_eq!(parse(b"$-7\r\n", &lim), Err(ProtoError::BadLength));
    assert_eq!(parse(b"!x\r\n", &lim), Err(ProtoError::BadType(b'!')));
    // server-side command parsing: arrays of bulks, and inline
    let (args, used) = parse_command(b"*2\r\n$4\r\nECHO\r\n$2\r\nhi\r\nrest", &lim)
        .unwrap()
        .unwrap();
    assert_eq!((args, used), (vec![b"ECHO".to_vec(), b"hi".to_vec()], 22));
    assert_eq!(
        parse_command(b"  set   a  1 \r\nX", &lim)
            .unwrap()
            .unwrap()
            .0,
        words("set a 1")
            .iter()
            .map(|w| w.as_bytes().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        parse_command(b"*2\r\n$1\r\na\r\n:5\r\n", &lim),
        Err(ProtoError::ExpectedBulk(b':'))
    );
    println!(
        "  resp: 500 random frames x every prefix + {chunks} random chunk feeds round-trip; 8 hostile inputs rejected without allocating"
    );
}

// ================================================================ 11. the reference model + command generators

/// A deliberately naive second implementation: plain std collections, scores as integers, everything recomputed from
/// scratch. If the server and the model disagree on any of 120k random commands, one of them is wrong.
#[derive(Clone)]
enum M {
    S(String),
    L(Vec<String>),
    H(BTreeMap<String, String>),
    Set(BTreeSet<String>),
    Z(BTreeMap<String, i64>),
}

struct Model {
    keys: BTreeMap<String, (M, Option<u64>)>,
    now: u64,
}

fn mb(s: &str) -> (f64, bool) {
    let (ex, body) = s.strip_prefix('(').map_or((false, s), |r| (true, r));
    (
        match body {
            "-inf" => f64::NEG_INFINITY,
            "+inf" | "inf" => f64::INFINITY,
            x => x.parse().unwrap(),
        },
        ex,
    )
}

fn zsorted(z: &BTreeMap<String, i64>) -> Vec<(i64, String)> {
    let mut v: Vec<_> = z.iter().map(|(m, s)| (*s, m.clone())).collect();
    v.sort();
    v
}

impl Model {
    fn live(&mut self, k: &str) -> Option<&mut (M, Option<u64>)> {
        let now = self.now;
        if self
            .keys
            .get(k)
            .is_some_and(|(_, e)| e.is_some_and(|t| t <= now))
        {
            self.keys.remove(k);
        }
        self.keys.get_mut(k)
    }

    fn apply(&mut self, c: &[String]) -> Frame {
        let name = c[0].to_ascii_uppercase();
        let k = c.get(1).map(|s| s.as_str()).unwrap_or("");
        let now = self.now;
        // peek!: the typed value if the key exists (early WRONGTYPE for another type); make!: create it if absent
        macro_rules! peek {
            ($v:ident) => {
                match self.live(k) {
                    None => None,
                    Some((M::$v(x), _)) => Some(x),
                    Some(_) => return Frame::err(WRONGTYPE),
                }
            };
        }
        macro_rules! make {
            ($v:ident, $init:expr) => {{
                self.live(k);
                if !self.keys.contains_key(k) {
                    self.keys.insert(k.to_string(), (M::$v($init), None));
                }
                match self.keys.get_mut(k) {
                    Some((M::$v(x), _)) => x,
                    _ => return Frame::err(WRONGTYPE),
                }
            }};
        }
        let num = |i: usize| c[i].parse::<i64>().unwrap();
        let prune = |m: &mut Model, k: &str| {
            if m.keys.get(k).is_some_and(|(v, _)| match v {
                M::S(_) => false,
                M::L(x) => x.is_empty(),
                M::H(x) => x.is_empty(),
                M::Set(x) => x.is_empty(),
                M::Z(x) => x.is_empty(),
            }) {
                m.keys.remove(k);
            }
        };
        let rng_idx = |len: usize, s: i64, e: i64| norm_range(s, e, len);
        let flat = |v: Vec<(i64, String)>, ws: bool| -> Frame {
            Frame::Array(
                v.into_iter()
                    .flat_map(|(s, m)| {
                        if ws {
                            vec![b(&m), b(&s.to_string())]
                        } else {
                            vec![b(&m)]
                        }
                    })
                    .collect(),
            )
        };
        match name.as_str() {
            // ---- strings
            "SET" => {
                let (mut nx, mut xx, mut keep, mut exp, mut i) = (false, false, false, None, 3);
                while i < c.len() {
                    match c[i].as_str() {
                        "NX" => nx = true,
                        "XX" => xx = true,
                        "KEEPTTL" => keep = true,
                        "EX" => {
                            i += 1;
                            exp = Some(now + num(i) as u64 * 1000)
                        }
                        "PX" => {
                            i += 1;
                            exp = Some(now + num(i) as u64)
                        }
                        _ => unreachable!(),
                    }
                    i += 1;
                }
                let exists = self.live(k).is_some();
                if (nx && exists) || (xx && !exists) {
                    return Frame::Null;
                }
                let old = self.keys.get(k).and_then(|e| e.1);
                self.keys.insert(
                    k.to_string(),
                    (M::S(c[2].clone()), if keep { old } else { exp }),
                );
                Frame::ok()
            }
            "GET" => peek!(S).map_or(Frame::Null, |s| b(s)),
            "GETSET" => {
                let old = peek!(S).cloned();
                self.keys.insert(k.to_string(), (M::S(c[2].clone()), None));
                old.map_or(Frame::Null, |s| b(&s))
            }
            "SETNX" => {
                if self.live(k).is_some() {
                    return n(0);
                }
                self.keys.insert(k.to_string(), (M::S(c[2].clone()), None));
                n(1)
            }
            "APPEND" => {
                let s = make!(S, String::new());
                s.push_str(&c[2]);
                n(s.len() as i64)
            }
            "STRLEN" => n(peek!(S).map_or(0, |s| s.len() as i64)),
            "INCR" | "DECR" | "INCRBY" | "DECRBY" => {
                let d = match name.as_str() {
                    "INCR" => 1,
                    "DECR" => -1,
                    "INCRBY" => num(2),
                    _ => -num(2),
                };
                let cur = match peek!(S) {
                    Some(s) => match s.parse::<i64>() {
                        Ok(v) => v,
                        Err(_) => return Frame::err(NOT_INT),
                    },
                    None => 0,
                };
                let Some(v) = cur.checked_add(d) else {
                    return Frame::err("ERR increment or decrement would overflow");
                };
                match self.keys.get_mut(k) {
                    Some((M::S(s), _)) => *s = v.to_string(),
                    _ => {
                        self.keys.insert(k.to_string(), (M::S(v.to_string()), None));
                    }
                }
                n(v)
            }
            "MSET" => {
                for kv in c[1..].chunks(2) {
                    self.keys.insert(kv[0].clone(), (M::S(kv[1].clone()), None));
                }
                Frame::ok()
            }
            "MGET" => Frame::Array(
                c[1..]
                    .iter()
                    .map(|k| match self.live(k) {
                        Some((M::S(s), _)) => b(s),
                        _ => Frame::Null,
                    })
                    .collect(),
            ),
            "GETRANGE" => {
                let s = peek!(S).cloned().unwrap_or_default();
                b(&rng_idx(s.len(), num(2), num(3))
                    .map_or(String::new(), |(a, e)| s[a..=e].to_string()))
            }
            // ---- generic keys
            "DEL" => n(c[1..]
                .iter()
                .filter(|k| self.live(k).is_some() && self.keys.remove(k.as_str()).is_some())
                .count() as i64),
            "EXISTS" => n(c[1..].iter().filter(|k| self.live(k).is_some()).count() as i64),
            "EXPIRE" | "PEXPIRE" => {
                let at = now as i64 + num(2) * if name == "EXPIRE" { 1000 } else { 1 };
                match self.live(k) {
                    None => n(0),
                    Some(_) if at <= now as i64 => {
                        self.keys.remove(k);
                        n(1)
                    }
                    Some(e) => {
                        e.1 = Some(at as u64);
                        n(1)
                    }
                }
            }
            "TTL" | "PTTL" => match self.live(k) {
                None => n(-2),
                Some((_, None)) => n(-1),
                Some((_, Some(t))) => {
                    let ms = (*t - now) as i64;
                    n(if name == "TTL" { (ms + 500) / 1000 } else { ms })
                }
            },
            "PERSIST" => n(self.live(k).and_then(|e| e.1.take()).is_some() as i64),
            "TYPE" => Frame::Simple(
                match self.live(k) {
                    None => "none",
                    Some((M::S(_), _)) => "string",
                    Some((M::L(_), _)) => "list",
                    Some((M::H(_), _)) => "hash",
                    Some((M::Set(_), _)) => "set",
                    Some((M::Z(_), _)) => "zset",
                }
                .into(),
            ),
            "KEYS" => {
                let g = Glob::new(k.as_bytes());
                self.keys.retain(|_, (_, e)| !e.is_some_and(|t| t <= now));
                Frame::Array(
                    self.keys
                        .keys()
                        .filter(|x| g.is_match(x.as_bytes()))
                        .map(|x| b(x))
                        .collect(),
                )
            }
            "RENAME" => {
                self.live(&c[2]);
                match self.live(k).cloned() {
                    None => Frame::err("ERR no such key"),
                    Some(e) => {
                        self.keys.remove(k);
                        self.keys.insert(c[2].clone(), e);
                        Frame::ok()
                    }
                }
            }
            // ---- lists
            "LPUSH" | "RPUSH" => {
                let l = make!(L, vec![]);
                for v in &c[2..] {
                    if name == "LPUSH" {
                        l.insert(0, v.clone())
                    } else {
                        l.push(v.clone())
                    }
                }
                n(l.len() as i64)
            }
            "LPOP" | "RPOP" => {
                let count = c.get(2).map(|_| num(2) as usize);
                let Some(l) = peek!(L) else {
                    return if count.is_some() {
                        Frame::NullArray
                    } else {
                        Frame::Null
                    };
                };
                let mut out = vec![];
                for _ in 0..count.unwrap_or(1).min(l.len()) {
                    out.push(if name == "LPOP" {
                        l.remove(0)
                    } else {
                        l.pop().unwrap()
                    });
                }
                prune(self, k);
                match count {
                    None => b(&out[0]),
                    Some(_) => Frame::Array(out.iter().map(|s| b(s)).collect()),
                }
            }
            "LLEN" => n(peek!(L).map_or(0, |l| l.len() as i64)),
            "LRANGE" => {
                let l = peek!(L).cloned().unwrap_or_default();
                Frame::Array(
                    rng_idx(l.len(), num(2), num(3))
                        .map_or(vec![], |(a, e)| l[a..=e].iter().map(|s| b(s)).collect()),
                )
            }
            "LINDEX" => {
                let l = peek!(L).cloned().unwrap_or_default();
                let i = if num(2) < 0 {
                    l.len() as i64 + num(2)
                } else {
                    num(2)
                };
                if i < 0 || i as usize >= l.len() {
                    Frame::Null
                } else {
                    b(&l[i as usize])
                }
            }
            "LTRIM" => {
                if let Some(l) = peek!(L) {
                    *l =
                        rng_idx(l.len(), num(2), num(3)).map_or(vec![], |(a, e)| l[a..=e].to_vec());
                }
                prune(self, k);
                Frame::ok()
            }
            // ---- hashes
            "HSET" => {
                let h = make!(H, BTreeMap::new());
                n(c[2..]
                    .chunks(2)
                    .filter(|kv| h.insert(kv[0].clone(), kv[1].clone()).is_none())
                    .count() as i64)
            }
            "HGET" => peek!(H)
                .and_then(|h| h.get(&c[2]).cloned())
                .map_or(Frame::Null, |s| b(&s)),
            "HDEL" => {
                let cnt = peek!(H).map_or(0, |h| {
                    c[2..]
                        .iter()
                        .filter(|f| h.remove(f.as_str()).is_some())
                        .count()
                });
                prune(self, k);
                n(cnt as i64)
            }
            "HGETALL" => Frame::Array(peek!(H).map_or(vec![], |h| {
                h.iter().flat_map(|(f, v)| [b(f), b(v)]).collect()
            })),
            "HINCRBY" => {
                let cur = match peek!(H).and_then(|h| h.get(&c[2]).cloned()) {
                    Some(s) => match s.parse::<i64>() {
                        Ok(v) => v,
                        Err(_) => return Frame::err("ERR hash value is not an integer"),
                    },
                    None => 0,
                };
                let v = cur + num(3);
                make!(H, BTreeMap::new()).insert(c[2].clone(), v.to_string());
                n(v)
            }
            "HLEN" => n(peek!(H).map_or(0, |h| h.len() as i64)),
            "HEXISTS" => n(peek!(H).is_some_and(|h| h.contains_key(&c[2])) as i64),
            // ---- sets
            "SADD" => {
                let s = make!(Set, BTreeSet::new());
                n(c[2..].iter().filter(|m| s.insert((*m).clone())).count() as i64)
            }
            "SREM" => {
                let cnt = peek!(Set).map_or(0, |s| {
                    c[2..].iter().filter(|m| s.remove(m.as_str())).count()
                });
                prune(self, k);
                n(cnt as i64)
            }
            "SMEMBERS" => {
                Frame::Array(peek!(Set).map_or(vec![], |s| s.iter().map(|m| b(m)).collect()))
            }
            "SISMEMBER" => n(peek!(Set).is_some_and(|s| s.contains(&c[2])) as i64),
            "SCARD" => n(peek!(Set).map_or(0, |s| s.len() as i64)),
            "SINTER" | "SUNION" | "SDIFF" => {
                let mut sets = vec![];
                for key in &c[1..] {
                    match self.live(key) {
                        Some((M::Set(s), _)) => sets.push(s.clone()),
                        None => sets.push(BTreeSet::new()),
                        Some(_) => return Frame::err(WRONGTYPE),
                    }
                }
                let mut acc = sets[0].clone();
                for s in &sets[1..] {
                    acc = match name.as_str() {
                        "SINTER" => acc.into_iter().filter(|x| s.contains(x)).collect(),
                        "SUNION" => acc.into_iter().chain(s.iter().cloned()).collect(),
                        _ => acc.into_iter().filter(|x| !s.contains(x)).collect(),
                    };
                }
                Frame::Array(acc.iter().map(|m| b(m)).collect())
            }
            // ---- sorted sets
            "ZADD" => {
                let z = make!(Z, BTreeMap::new());
                n(c[2..]
                    .chunks(2)
                    .filter(|p| z.insert(p[1].clone(), p[0].parse().unwrap()).is_none())
                    .count() as i64)
            }
            "ZREM" => {
                let cnt = peek!(Z).map_or(0, |z| {
                    c[2..]
                        .iter()
                        .filter(|m| z.remove(m.as_str()).is_some())
                        .count()
                });
                prune(self, k);
                n(cnt as i64)
            }
            "ZRANGE" | "ZREVRANGE" => {
                let mut v = peek!(Z).map_or(vec![], |z| zsorted(z));
                if name == "ZREVRANGE" {
                    v.reverse();
                }
                flat(
                    rng_idx(v.len(), num(2), num(3)).map_or(vec![], |(a, e)| v[a..=e].to_vec()),
                    c.len() == 5,
                )
            }
            "ZRANGEBYSCORE" => {
                let (lo, hi) = (mb(&c[2]), mb(&c[3]));
                let (mut ws, mut off, mut cnt) = (false, 0usize, usize::MAX);
                if let Some(p) = c.iter().position(|x| x == "LIMIT") {
                    (off, cnt) = (
                        num(p + 1) as usize,
                        if num(p + 2) < 0 {
                            usize::MAX
                        } else {
                            num(p + 2) as usize
                        },
                    );
                }
                ws |= c.iter().any(|x| x == "WITHSCORES");
                let v = peek!(Z).map_or(vec![], |z| zsorted(z));
                let sel = v.into_iter().filter(|(s, _)| {
                    let s = *s as f64;
                    (if lo.1 { s > lo.0 } else { s >= lo.0 })
                        && (if hi.1 { s < hi.0 } else { s <= hi.0 })
                });
                flat(sel.skip(off).take(cnt).collect(), ws)
            }
            "ZCOUNT" => {
                let (lo, hi) = (mb(&c[2]), mb(&c[3]));
                let v = peek!(Z).map_or(vec![], |z| zsorted(z));
                n(v.iter()
                    .filter(|(s, _)| {
                        let s = *s as f64;
                        (if lo.1 { s > lo.0 } else { s >= lo.0 })
                            && (if hi.1 { s < hi.0 } else { s <= hi.0 })
                    })
                    .count() as i64)
            }
            "ZRANK" | "ZREVRANK" => {
                let mut v = peek!(Z).map_or(vec![], |z| zsorted(z));
                if name == "ZREVRANK" {
                    v.reverse();
                }
                v.iter()
                    .position(|(_, m)| *m == c[2])
                    .map_or(Frame::Null, |p| n(p as i64))
            }
            "ZSCORE" => peek!(Z)
                .and_then(|z| z.get(&c[2]).copied())
                .map_or(Frame::Null, |s| b(&s.to_string())),
            "ZINCRBY" => {
                let z = make!(Z, BTreeMap::new());
                let v = z.entry(c[3].clone()).or_insert(0);
                *v += num(2);
                b(&v.to_string())
            }
            "ZCARD" => n(peek!(Z).map_or(0, |z| z.len() as i64)),
            other => panic!("model has no command {other}"),
        }
    }
}

fn gen_cmd(rng: &mut Rng, kind: &str, pfx: &str) -> Vec<String> {
    let key = |rng: &mut Rng| format!("{pfx}{}", rng.below(5));
    let val = |rng: &mut Rng| format!("v{}", rng.below(8));
    let small = |rng: &mut Rng| (rng.below(12) as i64 - 6).to_string();
    let mem = |rng: &mut Rng| format!("m{}", rng.below(10));
    let score = |rng: &mut Rng| (rng.below(21) as i64 - 10).to_string();
    let bound = |rng: &mut Rng| match rng.below(6) {
        0 => "-inf".to_string(),
        1 => "+inf".into(),
        2 => format!("({}", rng.below(21) as i64 - 10),
        _ => score(rng),
    };
    let k = key(rng);
    let mut c: Vec<String> = match kind {
        "str" => match rng.below(16) {
            0 | 1 => {
                let v = if rng.chance(50) { small(rng) } else { val(rng) };
                let mut c = vec!["SET".into(), k, v];
                match rng.below(8) {
                    0 => c.push("NX".into()),
                    1 => c.push("XX".into()),
                    2 => c.extend(["PX".into(), (50 + rng.below(1500)).to_string()]),
                    3 => c.extend(["EX".into(), (1 + rng.below(3)).to_string()]),
                    4 => c.push("KEEPTTL".into()),
                    _ => {}
                }
                c
            }
            2 => vec!["GET".into(), k],
            3 => vec!["GETSET".into(), k, small(rng)],
            4 => vec!["SETNX".into(), k, small(rng)],
            5 => vec!["APPEND".into(), k, small(rng).replace('-', "")],
            6 => vec!["STRLEN".into(), k],
            7 => vec!["INCR".into(), k],
            8 => vec!["DECR".into(), k],
            9 => vec!["INCRBY".into(), k, small(rng)],
            10 => vec!["DECRBY".into(), k, small(rng)],
            11 => vec!["MSET".into(), k, small(rng), key(rng), val(rng)],
            12 => vec!["MGET".into(), k, key(rng), key(rng)],
            13 => vec!["GETRANGE".into(), k, small(rng), small(rng)],
            14 => vec!["SET".into(), k, i64::MAX.to_string()],
            _ => vec!["INCR".into(), k],
        },
        "list" => match rng.below(10) {
            0 | 1 => vec!["LPUSH".into(), k, val(rng), val(rng)],
            2 | 3 => vec!["RPUSH".into(), k, val(rng)],
            4 => vec![if rng.chance(50) { "LPOP" } else { "RPOP" }.into(), k],
            5 => vec![
                if rng.chance(50) { "LPOP" } else { "RPOP" }.into(),
                k,
                rng.below(4).to_string(),
            ],
            6 => vec!["LLEN".into(), k],
            7 => vec!["LRANGE".into(), k, small(rng), small(rng)],
            8 => vec!["LINDEX".into(), k, small(rng)],
            _ => vec!["LTRIM".into(), k, small(rng), small(rng)],
        },
        "hash" => match rng.below(8) {
            0 | 1 => vec!["HSET".into(), k, mem(rng), val(rng), mem(rng), small(rng)],
            2 => vec!["HGET".into(), k, mem(rng)],
            3 => vec!["HDEL".into(), k, mem(rng), mem(rng)],
            4 => vec!["HGETALL".into(), k],
            5 => vec!["HINCRBY".into(), k, mem(rng), small(rng)],
            6 => vec!["HLEN".into(), k],
            _ => vec!["HEXISTS".into(), k, mem(rng)],
        },
        "set" => match rng.below(9) {
            0 | 1 => vec!["SADD".into(), k, mem(rng), mem(rng)],
            2 => vec!["SREM".into(), k, mem(rng), mem(rng)],
            3 => vec!["SMEMBERS".into(), k],
            4 => vec!["SISMEMBER".into(), k, mem(rng)],
            5 => vec!["SCARD".into(), k],
            6 => vec!["SINTER".into(), k, key(rng)],
            7 => vec!["SUNION".into(), k, key(rng), key(rng)],
            _ => vec!["SDIFF".into(), k, key(rng)],
        },
        "zset" => match rng.below(14) {
            0..=2 => vec!["ZADD".into(), k, score(rng), mem(rng), score(rng), mem(rng)],
            3 => vec!["ZREM".into(), k, mem(rng)],
            4 | 5 => {
                let mut c = vec![
                    if rng.chance(50) {
                        "ZRANGE"
                    } else {
                        "ZREVRANGE"
                    }
                    .into(),
                    k,
                    small(rng),
                    small(rng),
                ];
                if rng.chance(50) {
                    c.push("WITHSCORES".into())
                }
                c
            }
            6 | 7 => {
                let mut c = vec!["ZRANGEBYSCORE".into(), k, bound(rng), bound(rng)];
                if rng.chance(40) {
                    c.push("WITHSCORES".into())
                }
                if rng.chance(40) {
                    c.extend([
                        "LIMIT".into(),
                        rng.below(3).to_string(),
                        (rng.below(4) as i64 - 1).to_string(),
                    ])
                }
                c
            }
            8 => vec![
                if rng.chance(50) { "ZRANK" } else { "ZREVRANK" }.into(),
                k,
                mem(rng),
            ],
            9 => vec!["ZSCORE".into(), k, mem(rng)],
            10 | 11 => vec!["ZINCRBY".into(), k, small(rng), mem(rng)],
            12 => vec!["ZCARD".into(), k],
            _ => vec!["ZCOUNT".into(), k, bound(rng), bound(rng)],
        },
        // keys: create values of every type, then use the type-agnostic commands on them
        _ => match rng.below(14) {
            0 => vec!["SET".into(), k, val(rng)],
            1 => vec!["RPUSH".into(), k, val(rng)],
            2 => vec!["HSET".into(), k, mem(rng), val(rng)],
            3 => vec!["SADD".into(), k, mem(rng)],
            4 => vec!["ZADD".into(), k, score(rng), mem(rng)],
            5 => vec!["DEL".into(), k, key(rng)],
            6 => vec!["EXISTS".into(), k, key(rng), k_dup(rng, pfx)],
            7 => vec![
                if rng.chance(50) { "EXPIRE" } else { "PEXPIRE" }.into(),
                k,
                (rng.below(3) as i64 * if rng.chance(50) { 1 } else { 400 } - 1).to_string(),
            ],
            8 => vec![if rng.chance(50) { "TTL" } else { "PTTL" }.into(), k],
            9 => vec!["PERSIST".into(), k],
            10 => vec!["TYPE".into(), k],
            11 => vec![
                "KEYS".into(),
                if rng.chance(50) {
                    format!("{pfx}[0-2]")
                } else {
                    format!("{pfx}*")
                },
            ],
            12 => vec!["RENAME".into(), k, key(rng)],
            _ => vec!["EXPIRE".into(), k, "1".into()],
        },
    };
    // 4% of the time use a command of ANOTHER type on this key: expect WRONGTYPE (or the right answer on a fresh key)
    if kind != "keys" && rng.chance(10) {
        c = vec!["DEL".into(), key(rng)]; // otherwise a foreign-typed key would stay wrong-typed for the rest of the run
    } else if kind != "keys" && rng.chance(5) {
        let other = ["str", "list", "hash", "set", "zset"][rng.below(5) as usize];
        if other != kind {
            c = gen_cmd(rng, other, pfx);
        }
    }
    c
}

fn k_dup(rng: &mut Rng, pfx: &str) -> String {
    format!("{pfx}{}", rng.below(5))
}

async fn model_run(kind: &str, seed: u64, total: usize) -> (usize, usize) {
    let (srv, clock) = mem_server().await;
    let mut c = Client::connect(srv.addr).await.unwrap();
    let (mut m, mut rng) = (
        Model {
            keys: BTreeMap::new(),
            now: clock.now_ms(),
        },
        Rng(seed),
    );
    let pfx = format!("{kind}:");
    let (mut done, mut nonnull, mut errors) = (0, 0, 0);
    while done < total {
        let size = 250.min(total - done);
        let cmds: Vec<Vec<String>> = (0..size).map(|_| gen_cmd(&mut rng, kind, &pfx)).collect();
        let want: Vec<Frame> = cmds.iter().map(|x| m.apply(x)).collect();
        let got = c.pipeline(&cmds).await.unwrap();
        for (i, (w, g)) in want.iter().zip(&got).enumerate() {
            assert_eq!(w, g, "[{kind}] command #{} {:?}", done + i, cmds[i]);
            nonnull += !matches!(g, Frame::Null | Frame::NullArray | Frame::Error(_)) as usize;
            errors += matches!(g, Frame::Error(_)) as usize;
        }
        done += size;
        if rng.chance(70) {
            let dt = rng.below(500); // time passes only BETWEEN batches, so server and model agree on "now"
            clock.advance(dt);
            m.now += dt;
        }
        if rng.chance(15) {
            c.cmd(&["DEBUG", "SWEEP"]).await.unwrap();
            m.keys.retain(|_, (_, e)| !e.is_some_and(|t| t <= m.now));
            assert_eq!(
                c.cmd(&["DBSIZE"]).await.unwrap(),
                n(m.keys.len() as i64),
                "[{kind}] DBSIZE after sweep"
            );
        }
    }
    srv.shutdown().await.unwrap();
    (nonnull, errors)
}

// ================================================================ 12. tests: protocol, golden transcripts, concurrency

/// Raw-socket transcript check: send the RESP encoding of each command, read EXACTLY the expected bytes.
async fn golden(addr: SocketAddr, family: &str, steps: &[(&[&str], &str)]) {
    let mut s = TcpStream::connect(addr).await.unwrap();
    for (cmd, want) in steps {
        s.write_all(&encode_cmd(cmd)).await.unwrap();
        let mut got = vec![0u8; want.len()];
        tokio::time::timeout(IO_TIMEOUT, s.read_exact(&mut got))
            .await
            .unwrap_or_else(|_| panic!("[{family}] {cmd:?}: timed out"))
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&got), *want, "[{family}] {cmd:?}");
    }
}

async fn t_golden() {
    let (srv, _) = mem_server().await;
    let a = srv.addr;
    golden(
        a,
        "strings",
        &[
            (&["SET", "a", "1"], "+OK\r\n"),
            (&["GET", "a"], "$1\r\n1\r\n"),
            (&["SET", "a", "2", "NX"], "$-1\r\n"),
            (&["SET", "b", "x", "XX"], "$-1\r\n"),
            (&["INCR", "a"], ":2\r\n"),
            (&["APPEND", "a", "5"], ":2\r\n"),
            (&["GETRANGE", "a", "0", "0"], "$1\r\n2\r\n"),
            (&["STRLEN", "a"], ":2\r\n"),
            (&["GETSET", "a", "z"], "$2\r\n25\r\n"),
            (&["MSET", "x", "1", "y", "2"], "+OK\r\n"),
            (
                &["MGET", "x", "nokey", "y"],
                "*3\r\n$1\r\n1\r\n$-1\r\n$1\r\n2\r\n",
            ),
            (
                &["INCR", "a"],
                "-ERR value is not an integer or out of range\r\n",
            ),
            (&["SETNX", "x", "9"], ":0\r\n"),
            (&["DECRBY", "x", "5"], ":-4\r\n"),
            (&["SETEX", "e", "10", "v"], "+OK\r\n"),
            (&["TTL", "e"], ":10\r\n"),
        ],
    )
    .await;
    golden(
        a,
        "keys",
        &[
            (&["EXISTS", "x", "y", "q"], ":2\r\n"),
            (&["EXPIRE", "x", "100"], ":1\r\n"),
            (&["TTL", "x"], ":100\r\n"),
            (&["PTTL", "nokey"], ":-2\r\n"),
            (&["PERSIST", "x"], ":1\r\n"),
            (&["TTL", "x"], ":-1\r\n"),
            (&["TYPE", "x"], "+string\r\n"),
            (&["TYPE", "nokey"], "+none\r\n"),
            (&["RENAME", "x", "w"], "+OK\r\n"),
            (&["KEYS", "w*"], "*1\r\n$1\r\nw\r\n"),
            (&["RENAME", "nokey", "q"], "-ERR no such key\r\n"),
            (&["DEL", "w", "y", "w"], ":2\r\n"),
            (
                &["SCAN", "0"],
                "*2\r\n$1\r\n0\r\n*2\r\n$1\r\na\r\n$1\r\ne\r\n",
            ),
            (&["DBSIZE"], ":2\r\n"),
            (&["FLUSHALL"], "+OK\r\n"),
            (&["DBSIZE"], ":0\r\n"),
        ],
    )
    .await;
    golden(
        a,
        "lists",
        &[
            (&["RPUSH", "l", "a", "b", "c"], ":3\r\n"),
            (&["LPUSH", "l", "z"], ":4\r\n"),
            (
                &["LRANGE", "l", "0", "-1"],
                "*4\r\n$1\r\nz\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nc\r\n",
            ),
            (&["LINDEX", "l", "-1"], "$1\r\nc\r\n"),
            (&["LPOP", "l"], "$1\r\nz\r\n"),
            (&["RPOP", "l", "2"], "*2\r\n$1\r\nc\r\n$1\r\nb\r\n"),
            (&["LLEN", "l"], ":1\r\n"),
            (&["LTRIM", "l", "1", "0"], "+OK\r\n"),
            (&["EXISTS", "l"], ":0\r\n"),
            (&["LPOP", "l"], "$-1\r\n"),
            (&["LPOP", "l", "2"], "*-1\r\n"),
            (&["RPUSH", "q", "1"], ":1\r\n"),
            (&["BLPOP", "q", "0"], "*2\r\n$1\r\nq\r\n$1\r\n1\r\n"),
        ],
    )
    .await;
    golden(
        a,
        "hashes",
        &[
            (&["HSET", "h", "f", "v", "g", "w"], ":2\r\n"),
            (&["HGET", "h", "f"], "$1\r\nv\r\n"),
            (
                &["HGETALL", "h"],
                "*4\r\n$1\r\nf\r\n$1\r\nv\r\n$1\r\ng\r\n$1\r\nw\r\n",
            ),
            (&["HINCRBY", "h", "n", "5"], ":5\r\n"),
            (&["HLEN", "h"], ":3\r\n"),
            (&["HDEL", "h", "f", "nope"], ":1\r\n"),
            (&["HEXISTS", "h", "f"], ":0\r\n"),
            (&["HGET", "h", "nope"], "$-1\r\n"),
            (
                &["HINCRBY", "h", "g", "1"],
                "-ERR hash value is not an integer\r\n",
            ),
        ],
    )
    .await;
    golden(
        a,
        "sets",
        &[
            (&["SADD", "s", "b", "a", "a"], ":2\r\n"),
            (&["SMEMBERS", "s"], "*2\r\n$1\r\na\r\n$1\r\nb\r\n"),
            (&["SISMEMBER", "s", "a"], ":1\r\n"),
            (&["SCARD", "s"], ":2\r\n"),
            (&["SADD", "t", "b", "c"], ":2\r\n"),
            (&["SINTER", "s", "t"], "*1\r\n$1\r\nb\r\n"),
            (
                &["SUNION", "s", "t"],
                "*3\r\n$1\r\na\r\n$1\r\nb\r\n$1\r\nc\r\n",
            ),
            (&["SDIFF", "s", "t"], "*1\r\n$1\r\na\r\n"),
            (&["SREM", "s", "a"], ":1\r\n"),
        ],
    )
    .await;
    golden(
        a,
        "zsets",
        &[
            (&["ZADD", "z", "1", "a", "2", "b", "3", "c"], ":3\r\n"),
            (
                &["ZRANGE", "z", "0", "-1", "WITHSCORES"],
                "*6\r\n$1\r\na\r\n$1\r\n1\r\n$1\r\nb\r\n$1\r\n2\r\n$1\r\nc\r\n$1\r\n3\r\n",
            ),
            (&["ZREVRANGE", "z", "0", "0"], "*1\r\n$1\r\nc\r\n"),
            (
                &["ZRANGEBYSCORE", "z", "(1", "+inf"],
                "*2\r\n$1\r\nb\r\n$1\r\nc\r\n",
            ),
            (&["ZRANK", "z", "c"], ":2\r\n"),
            (&["ZSCORE", "z", "b"], "$1\r\n2\r\n"),
            (&["ZINCRBY", "z", "1.5", "a"], "$3\r\n2.5\r\n"),
            (&["ZRANK", "z", "a"], ":1\r\n"),
            (&["ZCARD", "z"], ":3\r\n"),
            (&["ZREM", "z", "b"], ":1\r\n"),
            (&["ZSCORE", "z", "nope"], "$-1\r\n"),
            (&["ZCOUNT", "z", "-inf", "+inf"], ":2\r\n"),
            (
                &["ZADD", "z", "abc", "m"],
                "-ERR value is not a valid float\r\n",
            ),
        ],
    )
    .await;
    golden(
        a,
        "errors",
        &[
            (&["SET", "str", "x"], "+OK\r\n"),
            (
                &["LPUSH", "str", "a"],
                "-WRONGTYPE Operation against a key holding the wrong kind of value\r\n",
            ),
            (
                &["HGET", "z", "f"],
                "-WRONGTYPE Operation against a key holding the wrong kind of value\r\n",
            ),
            (&["FOO", "1"], "-ERR unknown command 'FOO'\r\n"),
            (
                &["GET"],
                "-ERR wrong number of arguments for 'get' command\r\n",
            ),
            (
                &["SET", "k", "v", "EX", "0"],
                "-ERR invalid expire time in 'set' command\r\n",
            ),
            (&["SET", "k", "v", "NX", "XX"], "-ERR syntax error\r\n"),
            (&["PING"], "+PONG\r\n"),
            (&["PING", "hi"], "$2\r\nhi\r\n"),
            (&["ECHO", "hey"], "$3\r\nhey\r\n"),
        ],
    )
    .await;
    golden(a, "transactions", &[
        (&["MULTI"], "+OK\r\n"), (&["SET", "tk", "1"], "+QUEUED\r\n"), (&["INCR", "tk"], "+QUEUED\r\n"), (&["LPUSH", "tk", "x"], "+QUEUED\r\n"),
        (&["EXEC"], "*3\r\n+OK\r\n:2\r\n-WRONGTYPE Operation against a key holding the wrong kind of value\r\n"),
        (&["DISCARD"], "-ERR DISCARD without MULTI\r\n"), (&["MULTI"], "+OK\r\n"), (&["MULTI"], "-ERR MULTI calls can not be nested\r\n"),
        (&["FOO"], "-ERR unknown command 'FOO'\r\n"), (&["EXEC"], "-EXECABORT Transaction discarded because of previous errors.\r\n"),
    ]).await;
    // pub/sub transcript: two sockets
    let mut sub = TcpStream::connect(a).await.unwrap();
    let mut pubr = Client::connect(a).await.unwrap();
    let read = async |s: &mut TcpStream, want: &str| {
        let mut got = vec![0u8; want.len()];
        tokio::time::timeout(IO_TIMEOUT, s.read_exact(&mut got))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&got), want);
    };
    sub.write_all(&encode_cmd(&["SUBSCRIBE", "c"]))
        .await
        .unwrap();
    read(&mut sub, "*3\r\n$9\r\nsubscribe\r\n$1\r\nc\r\n:1\r\n").await;
    sub.write_all(&encode_cmd(&["PSUBSCRIBE", "n*"]))
        .await
        .unwrap();
    read(&mut sub, "*3\r\n$10\r\npsubscribe\r\n$2\r\nn*\r\n:2\r\n").await;
    assert_eq!(pubr.cmd(&["PUBLISH", "c", "hi"]).await.unwrap(), n(1));
    assert_eq!(pubr.cmd(&["PUBLISH", "news", "x"]).await.unwrap(), n(1));
    assert_eq!(pubr.cmd(&["PUBLISH", "other", "x"]).await.unwrap(), n(0));
    read(&mut sub, "*3\r\n$7\r\nmessage\r\n$1\r\nc\r\n$2\r\nhi\r\n").await;
    read(
        &mut sub,
        "*4\r\n$8\r\npmessage\r\n$2\r\nn*\r\n$4\r\nnews\r\n$1\r\nx\r\n",
    )
    .await;
    sub.write_all(&encode_cmd(&["PING"])).await.unwrap();
    read(&mut sub, "*2\r\n$4\r\npong\r\n$0\r\n\r\n").await;
    sub.write_all(&encode_cmd(&["GET", "k"])).await.unwrap();
    read(&mut sub, "-ERR Can't execute 'get': only (P)SUBSCRIBE / (P)UNSUBSCRIBE / PING / QUIT are allowed in this context\r\n").await;
    sub.write_all(&encode_cmd(&["UNSUBSCRIBE"])).await.unwrap();
    read(&mut sub, "*3\r\n$11\r\nunsubscribe\r\n$1\r\nc\r\n:1\r\n").await;
    sub.write_all(&encode_cmd(&["PUNSUBSCRIBE"])).await.unwrap();
    read(&mut sub, "*3\r\n$12\r\npunsubscribe\r\n$2\r\nn*\r\n:0\r\n").await;
    sub.write_all(&encode_cmd(&["GET", "k"])).await.unwrap(); // back to normal mode
    read(&mut sub, "$-1\r\n").await;
    srv.shutdown().await.unwrap();
    println!(
        "  golden transcripts: 9 families, byte-exact replies incl. errors, WRONGTYPE, EXECABORT, subscriber mode"
    );
}

async fn t_protocol() {
    let (srv, _) = mem_server().await;
    let a = srv.addr;
    let mut c = Client::connect(a).await.unwrap();
    // inline commands (telnet style), several in one packet
    c.send_raw(b"SET inl 5\r\nGET inl\nPING\r\n\r\n   \r\nINCR inl\r\n")
        .await
        .unwrap();
    for want in [Frame::ok(), b("5"), Frame::Simple("PONG".into()), n(6)] {
        assert_eq!(c.read_frame().await.unwrap(), want);
    }
    // a frame split at EVERY byte boundary (each byte its own TCP write)
    let wire = encode_cmd(&["SET", "split", "he\r\nllo"]);
    for &byte in &wire {
        c.send_raw(&[byte]).await.unwrap();
        tokio::task::yield_now().await;
    }
    assert_eq!(c.read_frame().await.unwrap(), Frame::ok());
    assert_eq!(c.cmd(&["GET", "split"]).await.unwrap(), b("he\r\nllo"));
    // binary-safe values and a 900 KB value spanning many reads
    let big = vec![0xABu8; 900_000];
    assert_eq!(
        c.cmd(&[&b"SET"[..], b"big", &big]).await.unwrap(),
        Frame::ok()
    );
    assert_eq!(c.cmd(&["STRLEN", "big"]).await.unwrap(), n(900_000));
    assert_eq!(
        c.cmd(&[&b"GET"[..], b"big"]).await.unwrap(),
        Frame::Bulk(big)
    );
    // protocol errors: reply -ERR, then the server closes the connection
    let cases: &[(&[u8], &str)] = &[
        (
            b"*2\r\n$3\r\nGET\r\n:5\r\n",
            "-ERR Protocol error: expected '$', got ':'\r\n",
        ),
        (
            b"*1\r\n$2000000\r\n",
            "-ERR Protocol error: bulk string exceeds the configured limit\r\n",
        ),
        (b"*abc\r\n", "-ERR Protocol error: invalid integer\r\n"),
        (
            b"*1\r\n$3\r\nabcde\r\n",
            "-ERR Protocol error: expected CRLF after bulk payload\r\n",
        ),
        (
            b"*999999999\r\n",
            "-ERR Protocol error: array exceeds the configured limit\r\n",
        ),
    ];
    for (bytes, want) in cases {
        let mut bad = Client::connect(a).await.unwrap();
        bad.send_raw(bytes).await.unwrap();
        assert_eq!(
            bad.read_frame().await.unwrap(),
            Frame::Error(want[1..want.len() - 2].into())
        );
        assert!(
            bad.closed_by_peer().await,
            "connection must be closed after a protocol error"
        );
    }
    // good commands BEFORE the bad bytes are still answered, in order
    let mut bad = Client::connect(a).await.unwrap();
    bad.send_raw(b"PING\r\nECHO ok\r\n*x\r\n").await.unwrap();
    assert_eq!(
        bad.read_frame().await.unwrap(),
        Frame::Simple("PONG".into())
    );
    assert_eq!(bad.read_frame().await.unwrap(), b("ok"));
    assert!(
        matches!(bad.read_frame().await.unwrap(), Frame::Error(e) if e.starts_with("ERR Protocol error"))
    );
    assert!(bad.closed_by_peer().await);
    // an endless line with no newline is cut off at the limit instead of buffering forever
    let mut bad = Client::connect(a).await.unwrap();
    bad.send_raw(&vec![b'x'; 70_000]).await.unwrap();
    assert!(
        matches!(bad.read_frame().await.unwrap(), Frame::Error(e) if e.contains("too big inline request"))
    );
    assert!(bad.closed_by_peer().await);
    // QUIT closes politely
    let mut q = Client::connect(a).await.unwrap();
    assert_eq!(q.cmd(&["QUIT"]).await.unwrap(), Frame::ok());
    assert!(q.closed_by_peer().await);
    assert_eq!(
        c.cmd(&["PING"]).await.unwrap(),
        Frame::Simple("PONG".into())
    ); // the first connection is unaffected
    srv.shutdown().await.unwrap();
    println!(
        "  protocol: inline, byte-at-a-time frames, 900KB binary value, 5 malformed-frame kinds close the connection, 70KB newline-less line rejected"
    );
}

async fn t_pipeline() {
    let (srv, _) = mem_server().await;
    let mut c = Client::connect(srv.addr).await.unwrap();
    // 1000 commands in ONE write: replies must come back complete and in order
    let cmds: Vec<Vec<String>> = (0..1000)
        .map(|i| {
            if i % 2 == 0 {
                vec!["RPUSH".into(), "pl".into(), i.to_string()]
            } else {
                vec!["LLEN".into(), "pl".into()]
            }
        })
        .collect();
    let got = c.pipeline(&cmds).await.unwrap();
    for (i, g) in got.iter().enumerate() {
        assert_eq!(*g, n(i as i64 / 2 + 1), "reply #{i}");
    }
    // the same pipeline written in random-sized pieces, with replies read concurrently by another task
    let wire: Vec<u8> = (0..1000)
        .flat_map(|i| encode_cmd(&["INCR", "pc", &i.to_string()][..2]))
        .collect();
    let (mut rng, mut pos) = (Rng(3), 0);
    let mut c2 = Client::connect(srv.addr).await.unwrap();
    while pos < wire.len() {
        let end = (pos + 1 + rng.below(300) as usize).min(wire.len());
        c2.send_raw(&wire[pos..end]).await.unwrap();
        pos = end;
    }
    for i in 1..=1000 {
        assert_eq!(c2.read_frame().await.unwrap(), n(i));
    }
    assert_eq!(c.cmd(&["GET", "pc"]).await.unwrap(), b("1000"));
    srv.shutdown().await.unwrap();
    println!(
        "  pipelining: 1000 mixed commands in one write answered in order; 1000 INCR in random-sized fragments"
    );
}

async fn t_concurrent_incr() {
    let (srv, _) = mem_server().await;
    let t0 = Instant::now();
    let mut tasks = JoinSet::new();
    for _ in 0..50 {
        let addr = srv.addr;
        tasks.spawn(async move {
            let mut c = Client::connect(addr).await.unwrap();
            let mut seen = Vec::with_capacity(1000);
            for _ in 0..1000 {
                let Frame::Int(v) = c.cmd(&["INCR", "ctr"]).await.unwrap() else {
                    panic!()
                };
                seen.push(v);
            }
            assert!(
                seen.windows(2).all(|w| w[0] < w[1]),
                "one client observes a strictly increasing counter"
            );
            seen
        });
    }
    let mut all = vec![false; 50_001];
    while let Some(r) = tasks.join_next().await {
        for v in r.unwrap() {
            assert!(
                !std::mem::replace(&mut all[v as usize], true),
                "INCR returned {v} twice"
            );
        }
    }
    assert!(all[1..].iter().all(|x| *x));
    let mut c = Client::connect(srv.addr).await.unwrap();
    assert_eq!(c.cmd(&["GET", "ctr"]).await.unwrap(), b("50000"));
    srv.shutdown().await.unwrap();
    println!(
        "  50 clients x 1000 INCR: counter == 50000, every value 1..=50000 returned exactly once ({:.2}s)",
        t0.elapsed().as_secs_f64()
    );
}

async fn t_blocking() {
    let srv = start(Config::default(), Arc::new(SystemClock))
        .await
        .unwrap();
    let a = srv.addr;
    let mut ctl = Client::connect(a).await.unwrap();
    // five clients block on the same list, one at a time so their arrival order is known
    let mut waiters = Vec::new();
    for i in 0..5u64 {
        waiters.push(tokio::spawn(async move {
            let mut c = Client::connect(a).await.unwrap();
            (i, c.cmd(&["BLPOP", "q", "5"]).await.unwrap())
        }));
        ctl.wait_info("blocked_clients", |v| v == i + 1).await;
    }
    assert_eq!(
        ctl.cmd(&["RPUSH", "q", "a", "b", "c", "d", "e"])
            .await
            .unwrap(),
        n(5)
    );
    for w in waiters {
        let (i, reply) = w.await.unwrap();
        assert_eq!(
            reply,
            arr(&["q", ["a", "b", "c", "d", "e"][i as usize]]),
            "waiter {i} must be served in arrival order"
        );
    }
    assert_eq!(ctl.cmd(&["LLEN", "q"]).await.unwrap(), n(0));
    // BRPOP takes from the tail; multi-key BLPOP returns the key that fired
    let w = tokio::spawn(async move {
        Client::connect(a)
            .await
            .unwrap()
            .cmd(&["BRPOP", "x", "y", "5"])
            .await
            .unwrap()
    });
    ctl.wait_info("blocked_clients", |v| v == 1).await;
    ctl.cmd(&["RPUSH", "y", "1", "2"]).await.unwrap();
    assert_eq!(w.await.unwrap(), arr(&["y", "2"]));
    // timeout: nil after about the requested time
    let t0 = Instant::now();
    assert_eq!(
        ctl.cmd(&["BLPOP", "nokey", "0.3"]).await.unwrap(),
        Frame::NullArray
    );
    let el = t0.elapsed();
    assert!(
        el >= Duration::from_millis(280) && el < Duration::from_secs(3),
        "timeout took {el:?}"
    );
    // a pipelined command AFTER a blocking one waits for it: replies stay in order
    let w = tokio::spawn(async move {
        let mut c = Client::connect(a).await.unwrap();
        c.pipeline(&[vec!["BLPOP", "pq", "5"], vec!["PING"], vec!["GET", "after"]])
            .await
            .unwrap()
    });
    ctl.wait_info("blocked_clients", |v| v == 1).await;
    ctl.cmd(&["SET", "after", "yes"]).await.unwrap(); // must NOT be visible to the paused batch before it resumes... it is: later reads see it
    ctl.cmd(&["RPUSH", "pq", "item"]).await.unwrap();
    assert_eq!(
        w.await.unwrap(),
        vec![arr(&["pq", "item"]), Frame::Simple("PONG".into()), b("yes")]
    );
    // a client that disconnects while blocked must not swallow the next element
    let ghost = Client::connect(a).await.unwrap();
    let mut ghost = ghost;
    ghost
        .send_raw(&encode_cmd(&["BLPOP", "dq", "0"]))
        .await
        .unwrap();
    ctl.wait_info("blocked_clients", |v| v == 1).await;
    drop(ghost);
    ctl.wait_info("blocked_clients", |v| v == 0).await;
    ctl.cmd(&["RPUSH", "dq", "x"]).await.unwrap();
    assert_eq!(ctl.cmd(&["LLEN", "dq"]).await.unwrap(), n(1));
    // inside MULTI a blocking pop never blocks
    let r = ctl
        .pipeline(&[vec!["MULTI"], vec!["BLPOP", "empty", "0"], vec!["EXEC"]])
        .await
        .unwrap();
    assert_eq!(r[2], Frame::Array(vec![Frame::NullArray]));
    // graceful shutdown with a client parked in BLPOP 0 must not hang
    let mut parked = Client::connect(a).await.unwrap();
    parked
        .send_raw(&encode_cmd(&["BLPOP", "forever", "0"]))
        .await
        .unwrap();
    ctl.wait_info("blocked_clients", |v| v == 1).await;
    let t0 = Instant::now();
    srv.shutdown().await.unwrap();
    assert!(parked.closed_by_peer().await);
    println!(
        "  blocking: 5 BLPOP waiters served FIFO, BRPOP/multi-key, 0.3s timeout took {el:?}, pipeline resumes in order, ghost waiter skipped, shutdown with a parked client {:?}",
        t0.elapsed()
    );
}

async fn t_transactions() {
    let (srv, _) = mem_server().await;
    let a = srv.addr;
    let (mut c1, mut c2) = (
        Client::connect(a).await.unwrap(),
        Client::connect(a).await.unwrap(),
    );
    // conflict: another client writes the watched key between WATCH and EXEC
    c1.cmd(&["SET", "balance", "100"]).await.unwrap();
    assert_eq!(c1.cmd(&["WATCH", "balance"]).await.unwrap(), Frame::ok());
    assert_eq!(c2.cmd(&["INCR", "balance"]).await.unwrap(), n(101));
    c1.cmd(&["MULTI"]).await.unwrap();
    assert_eq!(
        c1.cmd(&["SET", "balance", "0"]).await.unwrap(),
        Frame::Simple("QUEUED".into())
    );
    assert_eq!(
        c1.cmd(&["EXEC"]).await.unwrap(),
        Frame::NullArray,
        "EXEC must abort with a nil array"
    );
    assert_eq!(
        c1.cmd(&["GET", "balance"]).await.unwrap(),
        b("101"),
        "nothing from the aborted transaction ran"
    );
    // no conflict: EXEC runs; and the watch is cleared afterwards
    c1.cmd(&["WATCH", "balance"]).await.unwrap();
    c1.cmd(&["MULTI"]).await.unwrap();
    c1.cmd(&["DECRBY", "balance", "1"]).await.unwrap();
    assert_eq!(c1.cmd(&["EXEC"]).await.unwrap(), Frame::Array(vec![n(100)]));
    c2.cmd(&["SET", "balance", "7"]).await.unwrap();
    c1.cmd(&["MULTI"]).await.unwrap();
    c1.cmd(&["INCR", "balance"]).await.unwrap();
    assert_eq!(
        c1.cmd(&["EXEC"]).await.unwrap(),
        Frame::Array(vec![n(8)]),
        "watch was released by the previous EXEC"
    );
    // UNWATCH and DISCARD release the watch; deleting / expiring / FLUSHALL a watched key counts as a change
    for breaker in [
        vec!["DEL", "w"],
        vec!["FLUSHALL"],
        vec!["PEXPIRE", "w", "5000"],
        vec!["APPEND", "w", "x"],
    ] {
        c1.cmd(&["SET", "w", "1"]).await.unwrap();
        c1.cmd(&["WATCH", "w"]).await.unwrap();
        c2.cmd(&breaker).await.unwrap();
        c1.cmd(&["MULTI"]).await.unwrap();
        assert_eq!(
            c1.cmd(&["EXEC"]).await.unwrap(),
            Frame::NullArray,
            "{breaker:?} must dirty the watch"
        );
    }
    c1.cmd(&["WATCH", "w"]).await.unwrap();
    c1.cmd(&["UNWATCH"]).await.unwrap();
    c2.cmd(&["SET", "w", "2"]).await.unwrap();
    c1.cmd(&["MULTI"]).await.unwrap();
    assert_eq!(c1.cmd(&["EXEC"]).await.unwrap(), Frame::Array(vec![]));
    assert!(matches!(
        c1.cmd(&["WATCH", "w"]).await.unwrap(),
        Frame::Simple(_)
    ));
    c1.cmd(&["MULTI"]).await.unwrap();
    assert!(matches!(
        c1.cmd(&["WATCH", "w"]).await.unwrap(),
        Frame::Error(_)
    ));
    c1.cmd(&["DISCARD"]).await.unwrap();
    // optimistic-locking retry loop: 10 clients x 20 transfers each, no lost update whatever the interleaving
    c1.cmd(&["MSET", "from", "1000", "to", "0"]).await.unwrap();
    let mut tasks = JoinSet::new();
    for _ in 0..10 {
        tasks.spawn(async move {
            let (mut c, mut retries) = (Client::connect(a).await.unwrap(), 0u32);
            for _ in 0..20 {
                loop {
                    c.cmd(&["WATCH", "from", "to"]).await.unwrap();
                    let Frame::Bulk(f) = c.cmd(&["GET", "from"]).await.unwrap() else {
                        panic!()
                    };
                    let from: i64 = String::from_utf8(f).unwrap().parse().unwrap();
                    let r = c
                        .pipeline(&[
                            vec!["MULTI".to_string()],
                            vec!["SET".into(), "from".into(), (from - 1).to_string()],
                            vec!["INCR".into(), "to".into()],
                            vec!["EXEC".into()],
                        ])
                        .await
                        .unwrap();
                    if r[3] != Frame::NullArray {
                        break;
                    }
                    retries += 1;
                }
            }
            retries
        });
    }
    let mut retries = 0;
    while let Some(r) = tasks.join_next().await {
        retries += r.unwrap();
    }
    assert_eq!(
        c1.cmd(&["MGET", "from", "to"]).await.unwrap(),
        arr(&["800", "200"])
    );
    srv.shutdown().await.unwrap();
    println!(
        "  transactions: WATCH conflict aborts (nil), 4 breaker commands dirty the watch, UNWATCH/DISCARD release it; 200 CAS transfers had {retries} aborted attempts, 0 lost updates"
    );
}

async fn t_pubsub() {
    let srv = start(Config::default(), Arc::new(SystemClock))
        .await
        .unwrap();
    let a = srv.addr;
    // 3 subscribers, one also holds a matching pattern; one publisher, 1000 messages
    let mut subs = Vec::new();
    for i in 0..3 {
        let mut s = Client::connect(a).await.unwrap();
        assert_eq!(
            s.cmd(&["SUBSCRIBE", "news"]).await.unwrap(),
            Frame::Array(vec![b("subscribe"), b("news"), n(1)])
        );
        if i == 2 {
            assert_eq!(
                s.cmd(&["PSUBSCRIBE", "n*"]).await.unwrap(),
                Frame::Array(vec![b("psubscribe"), b("n*"), n(2)])
            );
        }
        subs.push(s);
    }
    let mut p = Client::connect(a).await.unwrap();
    let msgs: Vec<Vec<String>> = (0..1000)
        .map(|i| vec!["PUBLISH".into(), "news".into(), format!("msg{i}")])
        .collect();
    let counts = p.pipeline(&msgs).await.unwrap();
    assert!(
        counts[..1000].iter().all(|c| *c == n(4)),
        "3 channel subscriptions + 1 pattern subscription"
    );
    for (si, s) in subs.iter_mut().enumerate() {
        let mut next = 0;
        let expected = if si == 2 { 2000 } else { 1000 };
        for _ in 0..expected {
            let Frame::Array(f) = s.read_frame().await.unwrap() else {
                panic!()
            };
            match f[0].text().as_str() {
                "message" => assert_eq!(
                    f[2],
                    b(&format!("msg{next}")),
                    "sub {si}: out of order or duplicated"
                ),
                "pmessage" => {
                    assert_eq!(si, 2);
                    assert_eq!(f[3], b(&format!("msg{}", next - 1)));
                }
                other => panic!("unexpected push {other}"),
            }
            if f[0].text() == "message" {
                next += 1;
            }
        }
        // nothing more arrives: exactly once
        s.send_raw(&encode_cmd(&["PING"])).await.unwrap();
        assert_eq!(
            s.read_frame().await.unwrap(),
            Frame::Array(vec![b("pong"), b("")])
        );
    }
    srv.shutdown().await.unwrap();
    // slow subscriber: never reads, queue holds 256 frames, messages are 64 KB: it must be disconnected, others unaffected
    let srv = start(
        Config {
            push_buffer: 256,
            ..Config::default()
        },
        Arc::new(SystemClock),
    )
    .await
    .unwrap();
    let a = srv.addr;
    let mut p = Client::connect(a).await.unwrap();
    let mut lazy = Client::connect(a).await.unwrap();
    lazy.cmd(&["SUBSCRIBE", "firehose"]).await.unwrap();
    let mut healthy = Client::connect(a).await.unwrap();
    healthy.cmd(&["SUBSCRIBE", "firehose"]).await.unwrap();
    let reader = tokio::spawn(async move {
        let mut got = 0;
        while got < 1000 {
            healthy.read_frame().await.unwrap();
            got += 1;
        }
        (got, healthy)
    });
    let payload = "x".repeat(64 * 1024);
    for _ in 0..1000 {
        p.cmd(&["PUBLISH", "firehose", &payload]).await.unwrap();
    }
    let (got, _healthy) = reader.await.unwrap();
    assert_eq!(got, 1000, "the healthy subscriber got every message");
    let kills = p.wait_info("slow_subscriber_disconnects", |v| v >= 1).await;
    assert_eq!(
        p.cmd(&["PUBLISH", "firehose", "again"]).await.unwrap(),
        n(1),
        "only the healthy subscriber is left"
    );
    // the slow client eventually sees the connection die (after draining whatever the kernel buffered)
    let mut dead = false;
    for _ in 0..100_000 {
        match lazy.read_frame().await {
            Ok(_) => {}
            Err(_) => {
                dead = true;
                break;
            }
        }
    }
    assert!(dead, "slow subscriber was not disconnected");
    srv.shutdown().await.unwrap();
    println!(
        "  pub/sub: 3 subscribers x 1000 msgs in order exactly once (+pattern dup), subscriber mode, slow subscriber cut off (disconnects={kills}) while healthy one got all 1000"
    );
}

async fn t_expiry() {
    // deterministic: manual clock, one explicit sampling cycle at a time
    let (srv, clock) = mem_server().await;
    let mut c = Client::connect(srv.addr).await.unwrap();
    let cmds: Vec<Vec<String>> = (0..10_000)
        .map(|i| {
            if i % 5 < 3 {
                vec![
                    "SET".into(),
                    format!("e{i}"),
                    "v".into(),
                    "PX".into(),
                    "100".into(),
                ]
            } else if i % 5 == 3 {
                vec![
                    "SET".into(),
                    format!("l{i}"),
                    "v".into(),
                    "PX".into(),
                    "100000".into(),
                ]
            } else {
                vec!["SET".into(), format!("p{i}"), "v".into()]
            }
        })
        .collect();
    c.pipeline(&cmds).await.unwrap();
    assert_eq!(c.info_u64("expired_keys").await, 0);
    clock.advance(200); // 6000 keys are now dead but nobody has touched them
    assert_eq!(
        c.cmd(&["DBSIZE"]).await.unwrap(),
        n(10_000),
        "expired keys linger until touched or sampled (like Redis)"
    );
    assert_eq!(
        c.cmd(&["GET", "e0"]).await.unwrap(),
        Frame::Null,
        "LAZY expiry: the access itself deletes the key"
    );
    assert_eq!(c.cmd(&["DBSIZE"]).await.unwrap(), n(9_999));
    let mut rounds = 0;
    let mut left_expired = 5_999;
    while left_expired > 0 && rounds < 30 {
        let removed = if let Frame::Int(v) = c.cmd(&["DEBUG", "EXPIRE-CYCLE"]).await.unwrap() {
            v
        } else {
            panic!()
        };
        left_expired -= removed;
        rounds += 1;
        if rounds == 1 {
            let frac = left_expired as f64 / (left_expired + 2_000) as f64;
            assert!(
                frac < 0.5,
                "one cycle must cut the garbage from 75% to well below half, got {frac:.2}"
            );
            println!(
                "    one sampling cycle removed {removed} keys; garbage fraction now {:.1}% (it stops once a 20-key sample is <=25% expired, so this is noisy)",
                frac * 100.0
            );
        }
    }
    // each cycle stops once a sample looks clean, so the last stragglers survive many cycles; a sweep (SCAN of all TTLs) finishes
    let rest = if let Frame::Int(v) = c.cmd(&["DEBUG", "SWEEP"]).await.unwrap() {
        v
    } else {
        panic!()
    };
    assert_eq!(c.cmd(&["DBSIZE"]).await.unwrap(), n(4_000));
    assert_eq!(c.info_u64("expired_keys").await, 6_000);
    println!(
        "    {rounds} extra cycles + sweep of {rest}; 6000 expired keys reclaimed, 4000 live keys untouched"
    );
    srv.shutdown().await.unwrap();

    // real time, real timer: no client ever touches the keys, yet the background cycle reclaims them
    let srv = start(Config::default(), Arc::new(SystemClock))
        .await
        .unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    let cmds: Vec<Vec<String>> = (0..500)
        .map(|i| {
            vec![
                "SET".into(),
                format!("t{i}"),
                "v".into(),
                "PX".into(),
                "60".into(),
            ]
        })
        .collect();
    c.pipeline(&cmds).await.unwrap();
    let t0 = Instant::now();
    c.wait_info("expired_keys", |v| v == 500).await;
    assert_eq!(c.cmd(&["DBSIZE"]).await.unwrap(), n(0));
    println!(
        "    background cycle reclaimed 500 never-touched keys {:?} after they expired",
        t0.elapsed().saturating_sub(Duration::from_millis(60))
    );
    // TTL semantics
    c.cmd(&["SET", "s", "v", "EX", "100"]).await.unwrap();
    assert_eq!(
        c.cmd(&["SET", "s", "v2", "KEEPTTL"]).await.unwrap(),
        Frame::ok()
    );
    assert!(matches!(
        c.cmd(&["TTL", "s"]).await.unwrap(),
        Frame::Int(98..=100)
    ));
    c.cmd(&["SET", "s", "v3"]).await.unwrap();
    assert_eq!(
        c.cmd(&["TTL", "s"]).await.unwrap(),
        n(-1),
        "plain SET clears the TTL"
    );
    srv.shutdown().await.unwrap();
}

async fn t_scan() {
    let (srv, _) = mem_server().await;
    let mut c = Client::connect(srv.addr).await.unwrap();
    let cmds: Vec<Vec<String>> = (0..1000)
        .map(|i| vec!["SET".into(), format!("k{i:04}"), "v".into()])
        .collect();
    c.pipeline(&cmds).await.unwrap();
    let (mut cursor, mut seen, mut iters) = ("0".to_string(), BTreeMap::<String, u32>::new(), 0);
    loop {
        let Frame::Array(r) = c
            .cmd(&["SCAN", &cursor, "COUNT", "7", "MATCH", "k*"])
            .await
            .unwrap()
        else {
            panic!()
        };
        cursor = r[0].text();
        let Frame::Array(keys) = &r[1] else { panic!() };
        keys.iter()
            .for_each(|k| *seen.entry(k.text()).or_default() += 1);
        iters += 1;
        if iters == 20 {
            // mutate while the iteration is in flight: delete some keys, add others
            for i in (0..1000).step_by(3) {
                c.cmd(&["DEL", &format!("k{i:04}")]).await.unwrap();
            }
            c.cmd(&["SET", "zzz-new", "v"]).await.unwrap();
        }
        if cursor == "0" {
            break;
        }
    }
    let must: Vec<String> = (0..1000)
        .filter(|i| i % 3 != 0)
        .map(|i| format!("k{i:04}"))
        .collect();
    assert!(
        must.iter().all(|k| seen.get(k) == Some(&1)),
        "every key present for the whole scan appears exactly once"
    );
    assert!(seen.values().all(|v| *v == 1));
    srv.shutdown().await.unwrap();
    println!(
        "  scan: {iters} cursor steps with deletes+inserts mid-iteration; all {} stable keys returned exactly once",
        must.len()
    );
}

// ================================================================ 13. tests: persistence

type Unit = Vec<Vec<&'static str>>;

/// A script of atomic units (a transaction is ONE unit): the committed prefix after unit j is well defined.
fn units() -> Vec<Unit> {
    let one = |c: &[&'static str]| -> Unit { vec![c.to_vec()] };
    let mut u: Vec<Unit> = vec![
        one(&["SET", "a", "1"]),
        one(&["INCR", "a"]),
        one(&["RPUSH", "l", "x", "y", "z"]),
        one(&["HSET", "h", "f", "1", "g", "2"]),
        one(&["SADD", "s", "a", "b", "c"]),
        one(&["ZADD", "z", "1", "a", "2", "b"]),
        one(&["EXPIRE", "a", "100"]),
        one(&["LPOP", "l"]),
        one(&["HINCRBY", "h", "f", "5"]),
        one(&["SREM", "s", "b"]),
        one(&["ZINCRBY", "z", "2.5", "a"]),
        one(&["SET", "t", "v", "PX", "50000"]),
        one(&["MSET", "m1", "1", "m2", "2"]),
        vec![
            vec!["MULTI"],
            vec!["SET", "tx1", "1"],
            vec!["RPUSH", "tx2", "q"],
            vec!["INCR", "a"],
            vec!["EXEC"],
        ],
        one(&["RENAME", "m1", "m3"]),
        one(&["DEL", "m2"]),
        one(&["LPUSH", "l2", "1", "2", "3"]),
        one(&["LTRIM", "l2", "0", "0"]),
        one(&["SETEX", "se", "30", "v"]),
        one(&["PERSIST", "t"]),
        one(&["ZREM", "z", "b"]),
        one(&["HDEL", "h", "g"]),
        one(&["GETSET", "a", "new"]),
        one(&["BLPOP", "l", "0"]),
    ];
    u.extend((0..40).map(|_| one(&["INCR", "ctr"])));
    u
}

async fn run_unit(c: &mut Client, u: &Unit) {
    for cmd in u {
        let r = c.cmd(cmd).await.unwrap();
        assert!(!matches!(r, Frame::Error(_)), "{cmd:?} -> {r:?}");
    }
}

fn aof_cfg(path: &Path, p: Fsync) -> Config {
    Config {
        aof: Some((path.to_path_buf(), p)),
        active_expire_every: None,
        ..Config::default()
    }
}

async fn t_aof() {
    let dir = TempDir::new("aof");
    let path = dir.join("appendonly.aof");
    let us = units();
    // reference: the state after every prefix of the script, computed on a server with no persistence at all
    let (refsrv, _) = mem_server().await;
    let mut rc = Client::connect(refsrv.addr).await.unwrap();
    let mut expected = vec![dump_all(&mut rc).await];
    for u in &us {
        run_unit(&mut rc, u).await;
        expected.push(dump_all(&mut rc).await);
    }
    refsrv.shutdown().await.unwrap();

    // 1. fsync=always: record the file size after each acknowledged unit
    let clock = ManualClock::new(1_000_000);
    let srv = start(aof_cfg(&path, Fsync::Always), clock.clone())
        .await
        .unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    let (mut sizes, t0) = (vec![0u64], Instant::now());
    for u in &us {
        run_unit(&mut c, u).await;
        sizes.push(fs::metadata(&path).unwrap().len());
    }
    let always_ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(
        sizes.windows(2).all(|w| w[0] < w[1]),
        "every acknowledged write is already in the file"
    );
    let fsyncs = c.info_u64("aof_fsyncs").await;
    assert!(
        fsyncs >= us.len() as u64,
        "always: at least one fsync per acknowledged unit ({fsyncs})"
    );
    let state_a = dump_all(&mut c).await;
    assert_eq!(state_a, *expected.last().unwrap());
    srv.shutdown().await.unwrap();
    let full = fs::read(&path).unwrap();

    // 2. replay on startup reproduces the state exactly
    let srv = start(aof_cfg(&path, Fsync::Always), clock.clone())
        .await
        .unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    assert_eq!(dump_all(&mut c).await, state_a);
    assert_eq!(c.info_u64("aof_truncated_bytes").await, 0);
    // 3. rewrite: compaction alone, then a rewrite while writes keep arriving
    let before = c.info_u64("aof_current_size").await;
    assert!(matches!(
        c.cmd(&["BGREWRITEAOF"]).await.unwrap(),
        Frame::Simple(_)
    ));
    c.wait_info("aof_rewrites", |v| v == 1).await;
    let compact = c.info_u64("aof_current_size").await;
    assert!(
        compact < before,
        "rewrite must shrink a log full of overwrites ({before} -> {compact})"
    );
    assert_eq!(dump_all(&mut c).await, state_a);
    let mut cmds: Vec<Vec<&str>> = vec![vec!["BGREWRITEAOF"]];
    cmds.extend((0..200).map(|_| vec!["INCR", "ctr"]));
    let r = c.pipeline(&cmds).await.unwrap();
    assert!(matches!(&r[0], Frame::Simple(s) if s.starts_with("Background")));
    c.wait_info("aof_rewrites", |v| v == 2).await;
    assert_eq!(c.info_u64("aof_rewrite_in_progress").await, 0);
    assert_eq!(c.cmd(&["GET", "ctr"]).await.unwrap(), b("240"));
    let state_b = dump_all(&mut c).await;
    srv.shutdown().await.unwrap();
    let srv = start(aof_cfg(&path, Fsync::Always), clock.clone())
        .await
        .unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    assert_eq!(
        dump_all(&mut c).await,
        state_b,
        "rewritten AOF replays to the same state"
    );
    srv.shutdown().await.unwrap();
    println!(
        "  aof: {} units, replay == original; rewrite {before} -> {compact} bytes; a second rewrite with 200 concurrent INCRs loses none; state identical after restart",
        us.len()
    );

    // 4. crash recovery: cut the log at many points (mid-record, last byte, record boundary) and restart
    let mut cuts = 0;
    for j in [0usize, 2, 5, 12, 13, 14, 20, 23, us.len() - 1] {
        for cut in [
            sizes[j],
            sizes[j] + (sizes[j + 1] - sizes[j]) / 2,
            sizes[j + 1] - 1,
        ] {
            let p = dir.join(&format!("crash{cuts}.aof"));
            fs::write(&p, &full[..cut as usize]).unwrap();
            let srv = start(aof_cfg(&p, Fsync::No), clock.clone()).await.unwrap();
            let mut c = Client::connect(srv.addr).await.unwrap();
            assert_eq!(
                dump_all(&mut c).await,
                expected[j],
                "cut at {cut} inside unit {j}: must equal the committed prefix"
            );
            assert_eq!(c.info_u64("aof_truncated_bytes").await, cut - sizes[j]);
            c.cmd(&["SET", "after", "1"]).await.unwrap(); // appending after recovery must start on a clean boundary
            srv.shutdown().await.unwrap();
            let srv = start(aof_cfg(&p, Fsync::No), clock.clone()).await.unwrap();
            let mut c = Client::connect(srv.addr).await.unwrap();
            let mut d = dump_all(&mut c).await;
            assert!(d.iter().any(|l| l.starts_with("after string")));
            d.retain(|l| !l.starts_with("after "));
            assert_eq!(d, expected[j]);
            srv.shutdown().await.unwrap();
            cuts += 1;
        }
    }
    println!(
        "  aof crash recovery: {cuts} truncation points (incl. inside a MULTI/EXEC block) all recover exactly the committed prefix and keep appending cleanly"
    );

    // 5. the three fsync policies
    let mut row = vec![];
    for (pol, name) in [
        (Fsync::No, "no"),
        (Fsync::EverySec, "everysec"),
        (Fsync::Always, "always"),
    ] {
        let p = dir.join(&format!("{name}.aof"));
        let srv = start(aof_cfg(&p, pol), Arc::new(SystemClock))
            .await
            .unwrap();
        let mut c = Client::connect(srv.addr).await.unwrap();
        let t0 = Instant::now();
        for i in 0..30 {
            c.cmd(&["SET", &format!("k{i}"), "v"]).await.unwrap();
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        match pol {
            Fsync::No => assert_eq!(c.info_u64("aof_fsyncs").await, 0),
            Fsync::EverySec => {
                c.wait_info("aof_fsyncs", |v| v >= 1).await;
            }
            Fsync::Always => assert!(c.info_u64("aof_fsyncs").await >= 30),
        }
        assert!(fs::metadata(&p).unwrap().len() > 0);
        srv.shutdown().await.unwrap();
        let srv = start(aof_cfg(&p, pol), Arc::new(SystemClock))
            .await
            .unwrap();
        let mut c = Client::connect(srv.addr).await.unwrap();
        assert_eq!(
            c.cmd(&["DBSIZE"]).await.unwrap(),
            n(30),
            "graceful shutdown flushed everything"
        );
        srv.shutdown().await.unwrap();
        row.push(format!("{name}: {ms:.1} ms/30 writes"));
    }
    println!(
        "  fsync policies (30 sequential SETs): {} (always: {always_ms:.0} ms for {} units)",
        row.join(", "),
        us.len()
    );
}

async fn t_rdb() {
    let dir = TempDir::new("rdb");
    let path = dir.join("dump.mrdb");
    let clock = ManualClock::new(1_000_000);
    let cfg = Config {
        rdb: Some(path.clone()),
        active_expire_every: None,
        ..Config::default()
    };
    let srv = start(cfg.clone(), clock.clone()).await.unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    for u in &units() {
        run_unit(&mut c, u).await;
    }
    c.cmd(&["SET", "gone", "v", "PX", "100"]).await.unwrap();
    c.cmd(&["ZADD", "fz", "-inf", "lo", "0.1", "frac", "+inf", "hi"])
        .await
        .unwrap();
    c.cmd(&["SET", "bin", "\u{0}\u{1}\r\n\u{ff}"])
        .await
        .unwrap();
    clock.advance(200);
    let state = dump_all(&mut c).await;
    assert!(!state.iter().any(|l| l.starts_with("gone ")));
    assert_eq!(c.cmd(&["SAVE"]).await.unwrap(), Frame::ok());
    let size = fs::metadata(&path).unwrap().len();
    c.cmd(&["INCR", "ctr"]).await.unwrap();
    assert_eq!(c.info_u64("rdb_changes_since_last_save").await, 1);
    assert!(matches!(
        c.cmd(&["BGSAVE"]).await.unwrap(),
        Frame::Simple(_)
    ));
    c.wait_info("rdb_saves", |v| v == 2).await;
    c.wait_info("rdb_bgsave_in_progress", |v| v == 0).await;
    srv.shutdown().await.unwrap();
    let srv = start(cfg.clone(), clock.clone()).await.unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    let mut restored = dump_all(&mut c).await;
    let ctr = restored.iter().position(|l| l.starts_with("ctr ")).unwrap();
    assert!(
        restored.remove(ctr).contains("\"41\""),
        "BGSAVE captured the INCR made after SAVE"
    );
    let mut want = state.clone();
    want.remove(want.iter().position(|l| l.starts_with("ctr ")).unwrap());
    assert_eq!(restored, want);
    srv.shutdown().await.unwrap();
    // corruption: any flipped bit or truncation is detected by the checksum and refuses to start
    let good = fs::read(&path).unwrap();
    let mut rng = Rng(11);
    for _ in 0..2000 {
        let mut bad = good.clone();
        if rng.chance(30) {
            bad.truncate(rng.below(bad.len() as u64) as usize);
        } else {
            let i = rng.below(bad.len() as u64) as usize;
            bad[i] ^= 1 << rng.below(8);
        }
        assert!(rdb_decode(&bad).is_err());
    }
    let mut bad = good.clone();
    bad[good.len() / 2] ^= 0x40;
    fs::write(&path, &bad).unwrap();
    let err = start(cfg, clock.clone())
        .await
        .err()
        .expect("corrupt snapshot must not load");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    println!(
        "  rdb: SAVE+BGSAVE round trip ({size} bytes, expired key skipped, -inf/+inf/fractions/binary exact); 2000 corruptions all rejected: {err}"
    );
}

// ================================================================ 14. measurements and the demo session

/// The concurrency-model question in miniature (no sockets, so only the coordination cost is measured).
fn bench_models() {
    const OPS: usize = 100_000;
    let executor = |threads: usize, batch: usize| -> f64 {
        let (tx, rx) = xc::unbounded::<(Vec<(u64, bool)>, xc::Sender<u64>)>();
        let exec = std::thread::spawn(move || {
            let (mut m, mut acc) = (HashMap::<u64, u64>::new(), 0);
            while let Ok((reqs, reply)) = rx.recv() {
                for (k, w) in reqs {
                    if w {
                        m.insert(k, k);
                    } else {
                        acc += m.get(&k).copied().unwrap_or(0);
                    }
                }
                let _ = reply.send(acc);
            }
        });
        let t0 = Instant::now();
        std::thread::scope(|s| {
            for t in 0..threads {
                let tx = tx.clone();
                s.spawn(move || {
                    let (rtx, rrx) = xc::bounded(1);
                    let mut rng = Rng(t as u64 + 1);
                    let mut i = 0;
                    while i < OPS {
                        let k = batch.min(OPS - i);
                        let reqs = (0..k)
                            .map(|_| (rng.below(10_000), rng.chance(50)))
                            .collect();
                        tx.send((reqs, rtx.clone())).unwrap();
                        rrx.recv().unwrap();
                        i += k;
                    }
                });
            }
        });
        drop(tx);
        exec.join().unwrap();
        (threads * OPS) as f64 / t0.elapsed().as_secs_f64()
    };
    let sharded = |threads: usize| -> f64 {
        let shards: Vec<parking_lot::Mutex<HashMap<u64, u64>>> =
            (0..16).map(|_| Default::default()).collect();
        let t0 = Instant::now();
        std::thread::scope(|s| {
            for t in 0..threads {
                let shards = &shards;
                s.spawn(move || {
                    let mut rng = Rng(t as u64 + 1);
                    for _ in 0..OPS {
                        let (k, w) = (rng.below(10_000), rng.chance(50));
                        let mut g = shards[(k % 16) as usize].lock();
                        if w {
                            g.insert(k, k);
                        } else {
                            g.get(&k);
                        }
                    }
                });
            }
        });
        (threads * OPS) as f64 / t0.elapsed().as_secs_f64()
    };
    println!(
        "  {:<44}{:>14}{:>14}",
        "model (ops/s, mixed 50% SET / 50% GET)", "1 thread", "8 threads"
    );
    println!(
        "  {:<44}{:>14.0}{:>14.0}",
        "single executor, 1 op per round trip",
        executor(1, 1),
        executor(8, 1)
    );
    println!(
        "  {:<44}{:>14.0}{:>14.0}",
        "single executor, 64 ops per round trip",
        executor(1, 64),
        executor(8, 64)
    );
    println!(
        "  {:<44}{:>14.0}{:>14.0}",
        "16 shards, one Mutex each",
        sharded(1),
        sharded(8)
    );
}

async fn bench_ops(addr: SocketAddr, conns: usize, each: usize, set: bool, depth: usize) -> f64 {
    let t0 = Instant::now();
    let mut tasks = JoinSet::new();
    for t in 0..conns {
        tasks.spawn(async move {
            let mut c = Client::connect(addr).await.unwrap();
            let mut i = 0;
            while i < each {
                let k = depth.min(each - i);
                let cmds: Vec<Vec<String>> = (0..k)
                    .map(|j| {
                        let key = format!("bench:{}", (t * 7919 + i + j) % 1000);
                        if set {
                            vec!["SET".into(), key, "value-0123456789".into()]
                        } else {
                            vec!["GET".into(), key]
                        }
                    })
                    .collect();
                c.pipeline(&cmds).await.unwrap();
                i += k;
            }
        });
    }
    while let Some(r) = tasks.join_next().await {
        r.unwrap();
    }
    (conns * each) as f64 / t0.elapsed().as_secs_f64()
}

async fn t_session_and_bench() {
    let srv = start(Config::default(), Arc::new(SystemClock))
        .await
        .unwrap();
    let mut c = Client::connect(srv.addr).await.unwrap();
    let mut sub = Client::connect(srv.addr).await.unwrap();
    println!("  --- sample session (redis-cli style) ---");
    let script: &[&[&str]] = &[
        &["PING"],
        &["SET", "greeting", "hello world"],
        &["GET", "greeting"],
        &["INCR", "visits"],
        &["INCRBY", "visits", "41"],
        &["SET", "session:42", "alice", "EX", "30"],
        &["TTL", "session:42"],
        &["RPUSH", "queue", "job1", "job2", "job3"],
        &["LPOP", "queue"],
        &["LRANGE", "queue", "0", "-1"],
        &["HSET", "user:1", "name", "ada", "age", "36"],
        &["HGETALL", "user:1"],
        &["SADD", "tags", "rust", "redis", "rust"],
        &["SMEMBERS", "tags"],
        &[
            "ZADD",
            "leaderboard",
            "120",
            "ann",
            "95",
            "bob",
            "310",
            "cy",
        ],
        &["ZREVRANGE", "leaderboard", "0", "1", "WITHSCORES"],
        &["ZRANK", "leaderboard", "ann"],
        &["ZINCRBY", "leaderboard", "250", "bob"],
        &["ZRANGEBYSCORE", "leaderboard", "200", "+inf"],
        &["LPUSH", "greeting", "x"],
        &["SUBSCRIBE", "alerts"],
        &["PUBLISH", "alerts", "disk full"],
        &["BLPOP", "empty", "0.1"],
        &["MULTI"],
        &["INCR", "visits"],
        &["GET", "visits"],
        &["EXEC"],
        &["SCAN", "0", "MATCH", "user:*"],
        &["FOO"],
    ];
    for cmd in script {
        let line = cmd
            .iter()
            .map(|w| {
                if w.contains(' ') {
                    format!("{w:?}")
                } else {
                    w.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let r = if cmd[0] == "SUBSCRIBE" {
            sub.cmd(cmd).await.unwrap()
        } else {
            c.cmd(cmd).await.unwrap()
        };
        println!(
            "  {}> {line}\n  {}",
            srv.addr,
            render(&r, 0).replace('\n', "\n  ")
        );
        if cmd[0] == "PUBLISH" {
            let m = sub.read_frame().await.unwrap();
            println!(
                "  (subscriber connection receives)\n  {}",
                render(&m, 0).replace('\n', "\n  ")
            );
        }
    }
    println!("  --- throughput (loopback, release build; machine dependent) ---");
    let _ = bench_ops(srv.addr, 1, 2000, true, 1).await; // warm up
    for (conns, each, depth, label) in [
        (1, 20_000, 1, "1 conn, no pipelining"),
        (50, 2_000, 1, "50 conns, no pipelining"),
        (1, 100_000, 100, "1 conn, pipeline depth 100"),
    ] {
        let set = bench_ops(srv.addr, conns, each, true, depth).await;
        let get = bench_ops(srv.addr, conns, each, false, depth).await;
        println!("  {label:<28} SET {set:>10.0} ops/s   GET {get:>10.0} ops/s");
    }
    srv.shutdown().await.unwrap();
}

async fn sub_first(sub: &mut Client, cmd: &[&str]) -> Frame {
    sub.cmd(cmd).await.unwrap()
}

// ================================================================ 15. main

async fn step<F: std::future::Future<Output = ()>>(name: &str, f: F) {
    println!("== {name}");
    let t0 = Instant::now();
    if tokio::time::timeout(Duration::from_secs(25), f)
        .await
        .is_err()
    {
        panic!("step '{name}' timed out");
    }
    println!("   ({:.2}s)", t0.elapsed().as_secs_f64());
}

#[tokio::main]
async fn main() {
    let t0 = Instant::now();
    println!(
        "mini redis: {} commands, concurrency model = ONE executor thread fed by a channel\n",
        TABLE.len()
    );
    step("glob matcher", async { t_glob() }).await;
    step("skip list sorted set", async { t_skiplist() }).await;
    step("RESP parser", async { t_resp_parser() }).await;
    step("golden transcripts", t_golden()).await;
    step("protocol errors, inline, partial frames", t_protocol()).await;
    step("pipelining", t_pipeline()).await;
    step("model test: server vs reference model (20k random commands per type)", async {
        for (i, kind) in ["str", "list", "hash", "set", "zset", "keys"].into_iter().enumerate() {
            let (nonnull, errors) = model_run(kind, 1000 + i as u64, 20_000).await;
            println!("  {kind:<5} 20000 commands match the model ({nonnull} non-nil replies, {errors} error replies)");
        }
    })
    .await;
    step("expiry: lazy + active sampling", t_expiry()).await;
    step("SCAN", t_scan()).await;
    step("50 clients x 1000 INCR", t_concurrent_incr()).await;
    step("blocking pops", t_blocking()).await;
    step("transactions: MULTI/EXEC/WATCH", t_transactions()).await;
    step("pub/sub", t_pubsub()).await;
    step(
        "AOF: replay, rewrite, crash recovery, fsync policies",
        t_aof(),
    )
    .await;
    step("RDB snapshot", t_rdb()).await;
    step("concurrency models", async {
        tokio::task::spawn_blocking(bench_models).await.unwrap()
    })
    .await;
    step("session + throughput", t_session_and_bench()).await;
    println!("\nall checks passed in {:.1}s", t0.elapsed().as_secs_f64());
}
