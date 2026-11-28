use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{self, Write};
use std::path::PathBuf;

// ===================================================================== the protocol (step 1)

pub type Args = Vec<Vec<u8>>;

/// One value of the protocol.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
    Array(Vec<Frame>),
    NullArray,
}

/// What the reader accepts. Every length in the input is untrusted.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_line: usize,
    pub max_bulk: usize,
    pub max_array: usize,
    pub max_depth: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoError {
    /// The first byte of a frame is not one of `+ - : $ *`.
    BadType(u8),
    BadInteger,
    BadLength,
    BadTerminator,
    BadUtf8,
    LineTooLong,
    TooLarge,
    TooDeep,
    /// An element of a command is not a bulk string; the byte is its type byte.
    ExpectedBulk(u8),
}

/// The line at the front of `buf` and the bytes it takes with its CRLF.
fn take_line<'a>(buf: &'a [u8], limits: &Limits) -> Result<Option<(&'a [u8], usize)>, ProtoError> {
    match buf.windows(2).position(|w| w == b"\r\n") {
        Some(end) if end > limits.max_line => Err(ProtoError::LineTooLong),
        Some(end) => Ok(Some((&buf[..end], end + 2))),
        None => {
            // a final CR may be the first half of the CRLF
            let held = buf.len() - usize::from(buf.ends_with(b"\r"));
            if held > limits.max_line {
                Err(ProtoError::LineTooLong)
            } else {
                Ok(None)
            }
        }
    }
}

/// A decimal `i64`: an optional `-`, then digits.
fn parse_int(bytes: &[u8]) -> Option<i64> {
    if bytes.first() == Some(&b'+') {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// A declared length: `None` for -1, which stands for "no value".
fn parse_len(bytes: &[u8], max: usize) -> Result<Option<usize>, ProtoError> {
    let n = parse_int(bytes).ok_or(ProtoError::BadInteger)?;
    if n == -1 {
        return Ok(None);
    }
    let n = usize::try_from(n).map_err(|_| ProtoError::BadLength)?;
    if n > max {
        return Err(ProtoError::TooLarge);
    }
    Ok(Some(n))
}

fn parse_at(
    buf: &[u8],
    depth: usize,
    limits: &Limits,
) -> Result<Option<(Frame, usize)>, ProtoError> {
    let Some(&kind) = buf.first() else {
        return Ok(None);
    };
    if !matches!(kind, b'+' | b'-' | b':' | b'$' | b'*') {
        return Err(ProtoError::BadType(kind));
    }
    let Some((line, used)) = take_line(buf, limits)? else {
        return Ok(None);
    };
    let body = &line[1..];
    let text = || String::from_utf8(body.to_vec()).map_err(|_| ProtoError::BadUtf8);
    let frame = match kind {
        b'+' => Frame::Simple(text()?),
        b'-' => Frame::Error(text()?),
        b':' => Frame::Int(parse_int(body).ok_or(ProtoError::BadInteger)?),
        b'$' => {
            let Some(n) = parse_len(body, limits.max_bulk)? else {
                return Ok(Some((Frame::Null, used)));
            };
            let rest = &buf[used..];
            if rest.len() < n {
                return Ok(None);
            }
            let tail = &rest[n..rest.len().min(n.saturating_add(2))];
            if !b"\r\n".starts_with(tail) {
                return Err(ProtoError::BadTerminator);
            }
            if tail.len() < 2 {
                return Ok(None);
            }
            return Ok(Some((Frame::Bulk(rest[..n].to_vec()), used + n + 2)));
        }
        _ => {
            let Some(n) = parse_len(body, limits.max_array)? else {
                return Ok(Some((Frame::NullArray, used)));
            };
            if depth >= limits.max_depth {
                return Err(ProtoError::TooDeep);
            }
            // never allocate from a declared count
            let mut items = Vec::with_capacity(n.min(64));
            let mut at = used;
            for _ in 0..n {
                let Some((item, len)) = parse_at(&buf[at..], depth + 1, limits)? else {
                    return Ok(None);
                };
                items.push(item);
                at += len;
            }
            return Ok(Some((Frame::Array(items), at)));
        }
    };
    Ok(Some((frame, used)))
}

/// One frame from the front of `buf`: the frame and the bytes it takes, or `Ok(None)` for "need more".
pub fn parse_frame(buf: &[u8], limits: &Limits) -> Result<Option<(Frame, usize)>, ProtoError> {
    parse_at(buf, 0, limits)
}

/// One client command from the front of `buf`: its arguments and the bytes it takes.
pub fn parse_command(buf: &[u8], limits: &Limits) -> Result<Option<(Args, usize)>, ProtoError> {
    let Some(&first) = buf.first() else {
        return Ok(None);
    };
    if first != b'*' {
        // an inline command, as typed into a terminal
        let Some(newline) = buf.iter().position(|&b| b == b'\n') else {
            let held = buf.len() - usize::from(buf.ends_with(b"\r"));
            return if held > limits.max_line {
                Err(ProtoError::LineTooLong)
            } else {
                Ok(None)
            };
        };
        let line = buf[..newline]
            .strip_suffix(b"\r")
            .unwrap_or(&buf[..newline]);
        if line.len() > limits.max_line {
            return Err(ProtoError::LineTooLong);
        }
        let words = line
            .split(|b| b.is_ascii_whitespace())
            .filter(|w| !w.is_empty())
            .map(<[u8]>::to_vec)
            .collect();
        return Ok(Some((words, newline + 1)));
    }
    let Some((line, mut at)) = take_line(buf, limits)? else {
        return Ok(None);
    };
    let count = parse_len(&line[1..], limits.max_array)?.unwrap_or(0);
    let mut args = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        match buf.get(at) {
            None => return Ok(None),
            Some(b'$') => {}
            Some(&other) => return Err(ProtoError::ExpectedBulk(other)),
        }
        match parse_at(&buf[at..], 0, limits)? {
            None => return Ok(None),
            Some((Frame::Bulk(bytes), len)) => {
                args.push(bytes);
                at += len;
            }
            Some(_) => return Err(ProtoError::ExpectedBulk(b'$')),
        }
    }
    Ok(Some((args, at)))
}

/// Appends the bytes of `frame` to `out`.
pub fn encode(frame: &Frame, out: &mut Vec<u8>) {
    fn line(out: &mut Vec<u8>, kind: u8, body: &[u8]) {
        out.push(kind);
        // a line cannot hold a line end: it would start a second, forged frame
        out.extend(body.iter().map(|&b| match b {
            b'\r' | b'\n' => b' ',
            other => other,
        }));
        out.extend_from_slice(b"\r\n");
    }
    match frame {
        Frame::Simple(s) => line(out, b'+', s.as_bytes()),
        Frame::Error(s) => line(out, b'-', s.as_bytes()),
        Frame::Int(n) => line(out, b':', n.to_string().as_bytes()),
        Frame::Null => line(out, b'$', b"-1"),
        Frame::NullArray => line(out, b'*', b"-1"),
        Frame::Bulk(bytes) => {
            line(out, b'$', bytes.len().to_string().as_bytes());
            out.extend_from_slice(bytes);
            out.extend_from_slice(b"\r\n");
        }
        Frame::Array(items) => {
            line(out, b'*', items.len().to_string().as_bytes());
            items.iter().for_each(|item| encode(item, out));
        }
    }
}

/// A command as a client sends it: an array of bulk strings.
pub fn encode_command(args: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    let items = args.iter().cloned().map(Frame::Bulk).collect();
    encode(&Frame::Array(items), &mut out);
    out
}

// ===================================================================== keys with a deadline

/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

/// The keys that have a deadline, in a form that can pick one at random in constant time.
#[derive(Debug, Clone, Default)]
pub struct Deadlines {
    items: Vec<(Vec<u8>, u64)>,
    places: HashMap<Vec<u8>, usize>,
}

impl Deadlines {
    pub fn set(&mut self, key: &[u8], at: u64) {
        match self.places.get(key) {
            Some(&i) => self.items[i].1 = at,
            None => {
                self.places.insert(key.to_vec(), self.items.len());
                self.items.push((key.to_vec(), at));
            }
        }
    }

    pub fn remove(&mut self, key: &[u8]) -> bool {
        let Some(i) = self.places.remove(key) else {
            return false;
        };
        self.items.swap_remove(i);
        if let Some((moved, _)) = self.items.get(i) {
            self.places.insert(moved.clone(), i);
        }
        true
    }

    pub fn get(&self, key: &[u8]) -> Option<u64> {
        self.places.get(key).map(|&i| self.items[i].1)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.places.clear();
    }

    /// Looks at up to 20 keys chosen at random, removes those whose deadline has passed, and goes
    /// again while more than a quarter had. Returns the keys it removed.
    pub fn sweep(&mut self, now: u64, dice: &mut dyn Dice, max_rounds: usize) -> Vec<Vec<u8>> {
        let mut removed = Vec::new();
        for _ in 0..max_rounds {
            let looks = self.len().min(20);
            if looks == 0 {
                break;
            }
            let before = removed.len();
            for _ in 0..looks {
                if self.is_empty() {
                    break;
                }
                let (key, at) = &self.items[dice.below(self.len())];
                if *at <= now {
                    let key = key.clone();
                    self.remove(&key);
                    removed.push(key);
                }
            }
            if (removed.len() - before) * 4 <= looks {
                break;
            }
        }
        removed
    }
}

// ===================================================================== a ranked set (for sorted sets)

const NIL: usize = usize::MAX;
const LEVELS: usize = 32;

#[derive(Debug, Clone)]
struct Node {
    score: f64,
    member: Vec<u8>,
    /// `links[l]` is the next node on level `l` and how many places further on it is.
    links: Vec<(usize, usize)>,
}

/// Members ordered by score, then by their bytes. Insert, remove and rank take O(log n).
/// Scores must not be NaN; pass 0.0 rather than -0.0.
#[derive(Debug, Clone)]
pub struct Ranked {
    /// Node 0 is the head and holds no member.
    nodes: Vec<Node>,
    free: Vec<usize>,
    scores: HashMap<Vec<u8>, f64>,
    seed: u64,
}

impl Default for Ranked {
    fn default() -> Self {
        let head = Node {
            score: 0.0,
            member: Vec::new(),
            links: vec![(NIL, 0); LEVELS],
        };
        Ranked {
            nodes: vec![head],
            free: Vec::new(),
            scores: HashMap::new(),
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.scores == other.scores
    }
}

impl Ranked {
    pub fn len(&self) -> usize {
        self.scores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scores.is_empty()
    }

    pub fn score(&self, member: &[u8]) -> Option<f64> {
        self.scores.get(member).copied()
    }

    /// The last node on each level for which `before` holds, and that node's position.
    fn path(&self, before: impl Fn(&Node) -> bool) -> [(usize, usize); LEVELS] {
        let mut path = [(0, 0); LEVELS];
        let (mut at, mut position) = (0, 0);
        for level in (0..LEVELS).rev() {
            loop {
                let (next, span) = self.nodes[at].links[level];
                if next == NIL || !before(&self.nodes[next]) {
                    break;
                }
                at = next;
                position += span;
            }
            path[level] = (at, position);
        }
        path
    }

    /// How many members come before the first one for which `before` does not hold.
    pub fn count_while(&self, before: impl Fn(f64) -> bool) -> usize {
        self.path(|n| before(n.score))[0].1
    }

    /// Adds the member or moves it to a new score. Returns its earlier score.
    pub fn insert(&mut self, member: &[u8], score: f64) -> Option<f64> {
        let old = self.remove(member);
        let path = self.path(|n| (n.score, n.member.as_slice()) < (score, member));
        let mut height = 1;
        while height < LEVELS {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            if (self.seed >> 33) & 3 != 0 {
                break;
            }
            height += 1;
        }
        let node = Node {
            score,
            member: member.to_vec(),
            links: vec![(NIL, 0); height],
        };
        let id = match self.free.pop() {
            Some(id) => {
                self.nodes[id] = node;
                id
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        let position = path[0].1 + 1;
        for (level, &(prev, prev_position)) in path.iter().enumerate() {
            let (next, span) = self.nodes[prev].links[level];
            if level < height {
                let left = position - prev_position;
                self.nodes[prev].links[level] = (id, left);
                let right = if next == NIL { 0 } else { span + 1 - left };
                self.nodes[id].links[level] = (next, right);
            } else if next != NIL {
                self.nodes[prev].links[level].1 += 1;
            }
        }
        self.scores.insert(member.to_vec(), score);
        old
    }

    pub fn remove(&mut self, member: &[u8]) -> Option<f64> {
        let score = self.scores.remove(member)?;
        let path = self.path(|n| (n.score, n.member.as_slice()) < (score, member));
        let id = self.nodes[path[0].0].links[0].0;
        for (level, &(prev, _)) in path.iter().enumerate() {
            let (next, span) = self.nodes[prev].links[level];
            if next == id {
                let (after, further) = self.nodes[id].links[level];
                let span = if after == NIL { 0 } else { span + further - 1 };
                self.nodes[prev].links[level] = (after, span);
            } else if next != NIL {
                self.nodes[prev].links[level].1 -= 1;
            }
        }
        self.nodes[id].member = Vec::new();
        self.free.push(id);
        Some(score)
    }

    /// The member's place in the order, from 0.
    pub fn rank(&self, member: &[u8]) -> Option<usize> {
        let score = self.score(member)?;
        Some(self.path(|n| (n.score, n.member.as_slice()) < (score, member))[0].1)
    }

    /// Up to `count` members from place `start` on.
    pub fn range(&self, start: usize, count: usize) -> Vec<(Vec<u8>, f64)> {
        let (mut at, mut position) = (0, 0);
        for level in (0..LEVELS).rev() {
            loop {
                let (next, span) = self.nodes[at].links[level];
                if next == NIL || position + span > start {
                    break;
                }
                at = next;
                position += span;
            }
        }
        let mut out = Vec::new();
        at = self.nodes[at].links[0].0;
        while at != NIL && out.len() < count {
            out.push((self.nodes[at].member.clone(), self.nodes[at].score));
            at = self.nodes[at].links[0].0;
        }
        out
    }
}

// ===================================================================== key patterns

/// One token of the pattern at `p[i..]`: whether it accepts `c`, and where the next token starts.
fn token(p: &[u8], mut i: usize, c: u8) -> (bool, usize) {
    match p[i] {
        b'?' => (true, i + 1),
        b'\\' if i + 1 < p.len() => (p[i + 1] == c, i + 2),
        b'[' => {
            i += 1;
            let negated = p.get(i) == Some(&b'^');
            i += usize::from(negated);
            let mut found = false;
            while i < p.len() && p[i] != b']' {
                if p[i] == b'\\' && i + 1 < p.len() {
                    i += 1;
                }
                let low = p[i];
                if p.get(i + 1) == Some(&b'-') && i + 2 < p.len() && p[i + 2] != b']' {
                    let high = p[i + 2];
                    found |= low.min(high) <= c && c <= low.max(high);
                    i += 3;
                } else {
                    found |= low == c;
                    i += 1;
                }
            }
            (found != negated, (i + 1).min(p.len()))
        }
        literal => (literal == c, i + 1),
    }
}

/// Whether the whole of `text` fits `pattern`: `*`, `?`, `[abc]`, `[a-z]`, `[^a]` and `\` escapes.
pub fn glob_matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    // where to resume if the latest `*` has to take one byte more
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if pattern.get(p) == Some(&b'*') {
            p += 1;
            star = Some((p, t));
            continue;
        }
        if p < pattern.len() {
            let (accepted, next) = token(pattern, p, text[t]);
            if accepted {
                p = next;
                t += 1;
                continue;
            }
        }
        let Some((after, taken)) = star else {
            return false;
        };
        p = after;
        t = taken + 1;
        star = Some((after, taken + 1));
    }
    pattern[p..].iter().all(|&b| b == b'*')
}

// ===================================================================== clients waiting for a key

#[derive(Debug)]
struct Waiter<T> {
    order: u64,
    keys: Vec<Vec<u8>>,
    deadline: Option<u64>,
    ticket: T,
}

/// Clients that each wait for the first of several keys, or until a deadline.
#[derive(Debug)]
pub struct Waiting<T> {
    parked: u64,
    waiters: HashMap<u64, Waiter<T>>,
    queues: HashMap<Vec<u8>, BTreeMap<u64, u64>>,
    deadlines: BTreeSet<(u64, u64, u64)>,
}

impl<T> Default for Waiting<T> {
    fn default() -> Self {
        Waiting {
            parked: 0,
            waiters: HashMap::new(),
            queues: HashMap::new(),
            deadlines: BTreeSet::new(),
        }
    }
}

impl<T> Waiting<T> {
    /// `false`, and nothing changes, when `id` is already waiting.
    pub fn park(&mut self, id: u64, keys: &[Vec<u8>], deadline: Option<u64>, ticket: T) -> bool {
        if self.waiters.contains_key(&id) {
            return false;
        }
        let order = self.parked;
        self.parked += 1;
        for key in keys {
            self.queues
                .entry(key.clone())
                .or_default()
                .insert(order, id);
        }
        if let Some(at) = deadline {
            self.deadlines.insert((at, order, id));
        }
        let keys = keys.to_vec();
        let waiter = Waiter {
            order,
            keys,
            deadline,
            ticket,
        };
        self.waiters.insert(id, waiter);
        true
    }

    /// Takes `id` out of every queue.
    pub fn cancel(&mut self, id: u64) -> Option<T> {
        let waiter = self.waiters.remove(&id)?;
        for key in &waiter.keys {
            if let Some(queue) = self.queues.get_mut(key) {
                queue.remove(&waiter.order);
                if queue.is_empty() {
                    self.queues.remove(key);
                }
            }
        }
        if let Some(at) = waiter.deadline {
            self.deadlines.remove(&(at, waiter.order, id));
        }
        Some(waiter.ticket)
    }

    /// Removes and returns whoever has waited longest on `key`.
    pub fn wake(&mut self, key: &[u8]) -> Option<(u64, T)> {
        let id = *self.queues.get(key)?.values().next()?;
        self.cancel(id).map(|ticket| (id, ticket))
    }

    /// Removes those whose deadline is `now` or earlier: earliest deadline first, then longest waiting.
    pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
        let mut out = Vec::new();
        while let Some(&(at, _, id)) = self.deadlines.first() {
            if at > now {
                break;
            }
            if let Some(ticket) = self.cancel(id) {
                out.push((id, ticket));
            }
        }
        out
    }

    pub fn contains(&self, id: u64) -> bool {
        self.waiters.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.waiters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }
}

// ===================================================================== the keyspace

const WRONGTYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";
const NOT_INT: &str = "ERR value is not an integer or out of range";
const OVERFLOW: &str = "ERR increment or decrement would overflow";
const SYNTAX: &str = "ERR syntax error";
const NOT_FLOAT: &str = "ERR value is not a valid float";

fn error(text: &str) -> Frame {
    Frame::Error(text.to_string())
}

fn ok() -> Frame {
    Frame::Simple("OK".to_string())
}

fn bulks(items: impl IntoIterator<Item = Vec<u8>>) -> Frame {
    Frame::Array(items.into_iter().map(Frame::Bulk).collect())
}

fn int_arg(arg: &[u8]) -> Result<i64, Frame> {
    parse_int(arg).ok_or_else(|| error(NOT_INT))
}

fn upper(arg: &[u8]) -> String {
    String::from_utf8_lossy(arg).to_ascii_uppercase()
}

/// An inclusive range with negative indexes counted from the end and both ends clamped.
fn clamp_range(start: i64, stop: i64, len: usize) -> Option<(usize, usize)> {
    let len = len as i64;
    let start = if start < 0 {
        len.saturating_add(start).max(0)
    } else {
        start
    };
    let stop = if stop < 0 {
        len.saturating_add(stop)
    } else {
        stop.min(len - 1)
    };
    (start <= stop && start < len).then_some((start as usize, stop as usize))
}

fn wrong_arguments(name: &str) -> Frame {
    error(&format!(
        "ERR wrong number of arguments for '{}' command",
        name.to_ascii_lowercase()
    ))
}

fn arity_error(name: &str, count: usize, (min, max): (usize, usize)) -> Option<Frame> {
    (count < min || count > max).then(|| wrong_arguments(name))
}

const ANY: usize = usize::MAX;

/// The fewest and most arguments a command takes, its name included.
fn arity(name: &str) -> Option<(usize, usize)> {
    Some(match name {
        "PING" => (1, 2),
        "ECHO" | "GET" | "STRLEN" | "INCR" | "DECR" | "TYPE" | "LLEN" | "HLEN" | "HGETALL"
        | "SCARD" | "SMEMBERS" => (2, 2),
        "DBSIZE" | "FLUSHALL" => (1, 1),
        "APPEND" | "INCRBY" | "RENAME" | "HGET" | "SISMEMBER" => (3, 3),
        "LRANGE" | "LTRIM" | "HINCRBY" => (4, 4),
        "MGET" | "DEL" | "EXISTS" | "SINTER" | "SUNION" | "SDIFF" => (2, ANY),
        "MSET" | "LPUSH" | "RPUSH" | "HDEL" | "SADD" | "SREM" => (3, ANY),
        "HSET" => (4, ANY),
        "LPOP" | "RPOP" => (2, 3),
        "SET" => (3, ANY),
        "EXPIRE" | "PEXPIRE" | "PEXPIREAT" => (3, 3),
        "TTL" | "PTTL" | "PERSIST" => (2, 2),
        "ZADD" => (4, ANY),
        "ZREM" => (3, ANY),
        "ZCARD" => (2, 2),
        "ZSCORE" | "ZRANK" => (3, 3),
        "ZCOUNT" | "ZINCRBY" => (4, 4),
        "ZRANGE" | "ZRANGEBYSCORE" => (4, 5),
        "KEYS" => (2, 2),
        "SCAN" => (2, ANY),
        _ => return None,
    })
}

/// The error for a command nobody knows, or one with the wrong number of arguments.
fn check_command(args: &[Vec<u8>]) -> Option<Frame> {
    let name = upper(&args[0]);
    match arity(&name) {
        Some(limits) => arity_error(&name, args.len(), limits),
        None => Some(error("ERR unknown command")),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Str(Vec<u8>),
    List(VecDeque<Vec<u8>>),
    Hash(BTreeMap<Vec<u8>, Vec<u8>>),
    Set(BTreeSet<Vec<u8>>),
    Sorted(Ranked),
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Hash(_) => "hash",
            Value::Set(_) => "set",
            Value::Sorted(_) => "zset",
        }
    }

    /// A key never holds an empty collection.
    fn is_empty_collection(&self) -> bool {
        match self {
            Value::Str(_) => false,
            Value::List(l) => l.is_empty(),
            Value::Hash(h) => h.is_empty(),
            Value::Set(s) => s.is_empty(),
            Value::Sorted(z) => z.is_empty(),
        }
    }
}

/// An accessor for one kind of value: an error for another kind, and an empty one on request.
macro_rules! typed {
    ($name:ident, $kind:ident, $of:ty) => {
        fn $name(&mut self, key: &[u8], create: bool) -> Result<Option<&mut $of>, Frame> {
            self.purge(key);
            if create && !self.map.contains_key(key) {
                self.map
                    .insert(key.to_vec(), Value::$kind(Default::default()));
            }
            match self.map.get_mut(key) {
                None => Ok(None),
                Some(Value::$kind(value)) => Ok(Some(value)),
                Some(_) => Err(error(WRONGTYPE)),
            }
        }
    };
}

/// All the keys and what they hold.
#[derive(Debug, Default)]
pub struct Engine {
    map: BTreeMap<Vec<u8>, Value>,
    /// The keys changed since someone last looked, in the order they changed.
    changed: Vec<Vec<u8>>,
    deadlines: Deadlines,
    now: u64,
}

type Reply = Result<Frame, Frame>;

impl Engine {
    pub fn new() -> Engine {
        Engine::default()
    }

    typed!(string, Str, Vec<u8>);
    typed!(list, List, VecDeque<Vec<u8>>);
    typed!(hash, Hash, BTreeMap<Vec<u8>, Vec<u8>>);
    typed!(set_of, Set, BTreeSet<Vec<u8>>);
    typed!(sorted, Sorted, Ranked);

    /// Runs one command and returns its reply.
    pub fn execute(&mut self, args: &[Vec<u8>], now: u64) -> Frame {
        if args.is_empty() {
            return error("ERR unknown command");
        }
        if let Some(refusal) = check_command(args) {
            return refusal;
        }
        self.now = now;
        let reply = self
            .run(&upper(&args[0]), args)
            .unwrap_or_else(|refusal| refusal);
        reply
    }

    /// Records that a command changed `key`.
    fn touch(&mut self, key: &[u8]) {
        self.changed.push(key.to_vec());
    }

    /// After a change to a collection: records it and drops the key if nothing is left.
    fn after_write(&mut self, key: &[u8]) {
        self.touch(key);
        if self.map.get(key).is_some_and(Value::is_empty_collection) {
            self.remove_key(key);
        }
    }

    fn remove_key(&mut self, key: &[u8]) -> bool {
        self.deadlines.remove(key);
        self.map.remove(key).is_some()
    }

    fn expired(&self, key: &[u8]) -> bool {
        self.deadlines.get(key).is_some_and(|at| at <= self.now)
    }

    /// Removes the key if its deadline has passed. Every access to a key starts here.
    fn purge(&mut self, key: &[u8]) {
        if self.expired(key) {
            self.remove_key(key);
            self.forget_expired(key.to_vec());
        }
    }

    fn forget_expired(&mut self, key: Vec<u8>) {
        self.changed.push(key);
    }

    /// One round of active expiry: removes expired keys that nobody has touched. Returns how many.
    pub fn cycle(&mut self, now: u64, dice: &mut dyn Dice) -> usize {
        self.now = now;
        let removed = self.deadlines.sweep(now, dice, 16);
        let count = removed.len();
        for key in removed {
            self.map.remove(&key);
            self.forget_expired(key);
        }
        count
    }

    fn exists(&mut self, key: &[u8]) -> bool {
        self.purge(key);
        self.map.contains_key(key)
    }

    /// Replaces the key with a string. The deadline is the one given, the old one, or none.
    fn put(&mut self, key: &[u8], value: Vec<u8>, keep_ttl: bool, deadline: Option<u64>) {
        self.map.insert(key.to_vec(), Value::Str(value));
        if !keep_ttl {
            self.deadlines.remove(key);
        }
        if let Some(at) = deadline {
            self.deadlines.set(key, at);
        }
        self.touch(key);
    }

    fn set(&mut self, args: &[Vec<u8>]) -> Reply {
        let (mut nx, mut xx, mut keep, mut deadline) = (false, false, false, None);
        let mut rest = args[3..].iter();
        while let Some(option) = rest.next() {
            let option = upper(option);
            match option.as_str() {
                "NX" => nx = true,
                "XX" => xx = true,
                "KEEPTTL" => keep = true,
                "EX" | "PX" | "PXAT" => {
                    let n = int_arg(rest.next().ok_or_else(|| error(SYNTAX))?)?;
                    let bad = || error("ERR invalid expire time in 'set' command");
                    if n <= 0 {
                        return Err(bad());
                    }
                    let n = n as u64;
                    let at = match option.as_str() {
                        "EX" => n.checked_mul(1000).and_then(|ms| ms.checked_add(self.now)),
                        "PX" => n.checked_add(self.now),
                        _ => Some(n),
                    };
                    if deadline.replace(at.ok_or_else(bad)?).is_some() {
                        return Err(error(SYNTAX));
                    }
                }
                _ => return Err(error(SYNTAX)),
            }
        }
        if (nx && xx) || (keep && deadline.is_some()) {
            return Err(error(SYNTAX));
        }
        let exists = self.exists(&args[1]);
        if (nx && exists) || (xx && !exists) {
            return Ok(Frame::Null);
        }
        self.put(&args[1], args[2].clone(), keep, deadline);
        Ok(ok())
    }

    /// EXPIRE and its relatives, with the deadline already worked out in milliseconds.
    fn expire_at(&mut self, key: &[u8], at: i64) -> Frame {
        if !self.exists(key) {
            return Frame::Int(0);
        }
        let past = u64::try_from(at).map_or(true, |at| at <= self.now);
        if past {
            self.remove_key(key);
        } else {
            self.deadlines.set(key, at as u64);
        }
        self.touch(key);
        Frame::Int(1)
    }

    /// -2 for no key, -1 for no deadline, otherwise the milliseconds left.
    fn remaining(&mut self, key: &[u8]) -> i64 {
        if !self.exists(key) {
            return -2;
        }
        match self.deadlines.get(key) {
            None => -1,
            Some(at) => i64::try_from(at - self.now).unwrap_or(i64::MAX),
        }
    }

    fn push(&mut self, args: &[Vec<u8>], left: bool) -> Reply {
        let list = self.list(&args[1], true)?.expect("created");
        for value in &args[2..] {
            if left {
                list.push_front(value.clone());
            } else {
                list.push_back(value.clone());
            }
        }
        let len = list.len();
        self.after_write(&args[1]);
        Ok(Frame::Int(len as i64))
    }

    fn pop(&mut self, args: &[Vec<u8>], left: bool) -> Reply {
        let count = match args.get(2) {
            Some(arg) => Some(
                usize::try_from(int_arg(arg)?)
                    .map_err(|_| error("ERR value is out of range, must be positive"))?,
            ),
            None => None,
        };
        let Some(list) = self.list(&args[1], false)? else {
            return Ok(match count {
                Some(_) => Frame::NullArray,
                None => Frame::Null,
            });
        };
        let take = count.unwrap_or(1).min(list.len());
        let mut out: Vec<Vec<u8>> = (0..take)
            .filter_map(|_| {
                if left {
                    list.pop_front()
                } else {
                    list.pop_back()
                }
            })
            .collect();
        if take > 0 {
            self.after_write(&args[1]);
        }
        Ok(match count {
            Some(_) => bulks(out),
            None => out.pop().map_or(Frame::Null, Frame::Bulk),
        })
    }

    fn increment(&mut self, key: &[u8], by: i64) -> Reply {
        let old = match self.string(key, false)? {
            Some(value) => int_arg(value)?,
            None => 0,
        };
        let new = old.checked_add(by).ok_or_else(|| error(OVERFLOW))?;
        // only the value changes, so a deadline stays
        self.map
            .insert(key.to_vec(), Value::Str(new.to_string().into_bytes()));
        self.touch(key);
        Ok(Frame::Int(new))
    }

    fn set_operation(&mut self, name: &str, keys: &[Vec<u8>]) -> Reply {
        let mut sets = Vec::new();
        for key in keys {
            sets.push(self.set_of(key, false)?.cloned().unwrap_or_default());
        }
        let result = sets.into_iter().reduce(|a, b| match name {
            "SINTER" => a.intersection(&b).cloned().collect(),
            "SUNION" => a.union(&b).cloned().collect(),
            _ => a.difference(&b).cloned().collect(),
        });
        Ok(bulks(result.unwrap_or_default()))
    }

    fn run(&mut self, name: &str, args: &[Vec<u8>]) -> Reply {
        Ok(match name {
            "PING" => match args.get(1) {
                Some(message) => Frame::Bulk(message.clone()),
                None => Frame::Simple("PONG".to_string()),
            },
            "ECHO" => Frame::Bulk(args[1].clone()),
            "GET" => self
                .string(&args[1], false)?
                .map_or(Frame::Null, |v| Frame::Bulk(v.clone())),
            "SET" => self.set(args)?,
            "APPEND" => {
                let value = self.string(&args[1], true)?.expect("created");
                value.extend_from_slice(&args[2]);
                let len = value.len();
                self.touch(&args[1]);
                Frame::Int(len as i64)
            }
            "STRLEN" => Frame::Int(self.string(&args[1], false)?.map_or(0, |v| v.len()) as i64),
            "INCR" => self.increment(&args[1], 1)?,
            "DECR" => self.increment(&args[1], -1)?,
            "INCRBY" => self.increment(&args[1], int_arg(&args[2])?)?,
            "MGET" => Frame::Array(
                args[1..]
                    .iter()
                    .map(|key| match self.string(key, false) {
                        Ok(Some(value)) => Frame::Bulk(value.clone()),
                        _ => Frame::Null,
                    })
                    .collect(),
            ),
            "MSET" => {
                if args.len() % 2 != 1 {
                    return Err(wrong_arguments(name));
                }
                for pair in args[1..].chunks(2) {
                    self.purge(&pair[0]);
                    self.put(&pair[0], pair[1].clone(), false, None);
                }
                ok()
            }
            "DEL" => {
                let mut count = 0;
                for key in &args[1..] {
                    self.purge(key);
                    if self.remove_key(key) {
                        self.touch(key);
                        count += 1;
                    }
                }
                Frame::Int(count)
            }
            "EXISTS" => {
                let mut count = 0;
                for key in &args[1..] {
                    count += i64::from(self.exists(key));
                }
                Frame::Int(count)
            }
            "TYPE" => {
                self.purge(&args[1]);
                let kind = self.map.get(&args[1]).map_or("none", Value::type_name);
                Frame::Simple(kind.to_string())
            }
            "RENAME" => {
                self.purge(&args[1]);
                self.purge(&args[2]);
                let deadline = self.deadlines.get(&args[1]);
                let Some(value) = self.map.get(&args[1]).cloned() else {
                    return Err(error("ERR no such key"));
                };
                self.remove_key(&args[1]);
                self.touch(&args[1]);
                self.remove_key(&args[2]);
                self.map.insert(args[2].clone(), value);
                if let Some(at) = deadline {
                    self.deadlines.set(&args[2], at);
                }
                self.touch(&args[2]);
                ok()
            }
            "DBSIZE" => {
                let live = self.map.keys().filter(|k| !self.expired(k)).count();
                Frame::Int(live as i64)
            }
            "FLUSHALL" => {
                let keys: Vec<Vec<u8>> = self.map.keys().cloned().collect();
                for key in keys {
                    self.remove_key(&key);
                    self.touch(&key);
                }
                ok()
            }
            "LPUSH" => self.push(args, true)?,
            "RPUSH" => self.push(args, false)?,
            "LPOP" => self.pop(args, true)?,
            "RPOP" => self.pop(args, false)?,
            "LLEN" => Frame::Int(self.list(&args[1], false)?.map_or(0, |l| l.len()) as i64),
            "LRANGE" => {
                let (start, stop) = (int_arg(&args[2])?, int_arg(&args[3])?);
                let Some(list) = self.list(&args[1], false)? else {
                    return Ok(bulks([]));
                };
                match clamp_range(start, stop, list.len()) {
                    Some((a, b)) => bulks(list.range(a..=b).cloned()),
                    None => bulks([]),
                }
            }
            "LTRIM" => {
                let (start, stop) = (int_arg(&args[2])?, int_arg(&args[3])?);
                if let Some(list) = self.list(&args[1], false)? {
                    match clamp_range(start, stop, list.len()) {
                        Some((a, b)) => {
                            list.truncate(b + 1);
                            list.drain(..a);
                        }
                        None => list.clear(),
                    }
                    self.after_write(&args[1]);
                }
                ok()
            }
            "HSET" => {
                if args.len() % 2 != 0 {
                    return Err(wrong_arguments(name));
                }
                let hash = self.hash(&args[1], true)?.expect("created");
                let added = args[2..]
                    .chunks(2)
                    .filter(|pair| hash.insert(pair[0].clone(), pair[1].clone()).is_none())
                    .count();
                self.after_write(&args[1]);
                Frame::Int(added as i64)
            }
            "HGET" => self
                .hash(&args[1], false)?
                .and_then(|h| h.get(&args[2]))
                .map_or(Frame::Null, |v| Frame::Bulk(v.clone())),
            "HDEL" => {
                let mut removed = 0;
                if let Some(hash) = self.hash(&args[1], false)? {
                    removed = args[2..]
                        .iter()
                        .filter(|f| hash.remove(*f).is_some())
                        .count();
                }
                if removed > 0 {
                    self.after_write(&args[1]);
                }
                Frame::Int(removed as i64)
            }
            "HLEN" => Frame::Int(self.hash(&args[1], false)?.map_or(0, |h| h.len()) as i64),
            "HGETALL" => {
                let hash = self.hash(&args[1], false)?;
                bulks(
                    hash.into_iter()
                        .flat_map(|h| h.iter())
                        .flat_map(|(f, v)| [f.clone(), v.clone()]),
                )
            }
            "HINCRBY" => {
                let by = int_arg(&args[3])?;
                let old = match self.hash(&args[1], false)?.and_then(|h| h.get(&args[2])) {
                    Some(value) => {
                        parse_int(value).ok_or_else(|| error("ERR hash value is not an integer"))?
                    }
                    None => 0,
                };
                let new = old.checked_add(by).ok_or_else(|| error(OVERFLOW))?;
                let hash = self.hash(&args[1], true)?.expect("created");
                hash.insert(args[2].clone(), new.to_string().into_bytes());
                self.after_write(&args[1]);
                Frame::Int(new)
            }
            "SADD" => {
                let set = self.set_of(&args[1], true)?.expect("created");
                let added = args[2..]
                    .iter()
                    .filter(|m| set.insert((*m).clone()))
                    .count();
                if added > 0 {
                    self.after_write(&args[1]);
                }
                Frame::Int(added as i64)
            }
            "SREM" => {
                let mut removed = 0;
                if let Some(set) = self.set_of(&args[1], false)? {
                    removed = args[2..].iter().filter(|m| set.remove(*m)).count();
                }
                if removed > 0 {
                    self.after_write(&args[1]);
                }
                Frame::Int(removed as i64)
            }
            "SISMEMBER" => {
                let set = self.set_of(&args[1], false)?;
                Frame::Int(i64::from(set.is_some_and(|s| s.contains(&args[2]))))
            }
            "SCARD" => Frame::Int(self.set_of(&args[1], false)?.map_or(0, |s| s.len()) as i64),
            "SMEMBERS" => bulks(self.set_of(&args[1], false)?.cloned().unwrap_or_default()),
            "SINTER" | "SUNION" | "SDIFF" => self.set_operation(name, &args[1..])?,
            "EXPIRE" | "PEXPIRE" | "PEXPIREAT" => {
                let n = int_arg(&args[2])?;
                let now = i64::try_from(self.now).unwrap_or(i64::MAX);
                let at = match name {
                    "EXPIRE" => n.checked_mul(1000).and_then(|ms| ms.checked_add(now)),
                    "PEXPIRE" => n.checked_add(now),
                    _ => Some(n),
                };
                let at = at.ok_or_else(|| {
                    error(&format!(
                        "ERR invalid expire time in '{}' command",
                        name.to_ascii_lowercase()
                    ))
                })?;
                self.expire_at(&args[1], at)
            }
            "PTTL" => Frame::Int(self.remaining(&args[1])),
            "TTL" => {
                let ms = self.remaining(&args[1]);
                Frame::Int(if ms < 0 {
                    ms
                } else {
                    ms.saturating_add(500) / 1000
                })
            }
            "PERSIST" => {
                let had = self.exists(&args[1]) && self.deadlines.remove(&args[1]);
                if had {
                    self.touch(&args[1]);
                }
                Frame::Int(i64::from(had))
            }
            "ZADD" => {
                if args.len() % 2 != 0 {
                    return Err(error(SYNTAX));
                }
                // every score is checked before anything changes
                let pairs = args[2..]
                    .chunks(2)
                    .map(|pair| Ok((score_arg(&pair[0])?, &pair[1])))
                    .collect::<Result<Vec<_>, Frame>>()?;
                let set = self.sorted(&args[1], true)?.expect("created");
                let (mut added, mut changed) = (0, false);
                for (score, member) in pairs {
                    if set.score(member) != Some(score) {
                        added += i64::from(set.insert(member, score).is_none());
                        changed = true;
                    }
                }
                if changed {
                    self.after_write(&args[1]);
                }
                Frame::Int(added)
            }
            "ZREM" => {
                let mut removed = 0;
                if let Some(set) = self.sorted(&args[1], false)? {
                    removed = args[2..].iter().filter(|m| set.remove(m).is_some()).count();
                }
                if removed > 0 {
                    self.after_write(&args[1]);
                }
                Frame::Int(removed as i64)
            }
            "ZCARD" => Frame::Int(self.sorted(&args[1], false)?.map_or(0, |z| z.len()) as i64),
            "ZSCORE" => self
                .sorted(&args[1], false)?
                .and_then(|z| z.score(&args[2]))
                .map_or(Frame::Null, score_frame),
            "ZRANK" => self
                .sorted(&args[1], false)?
                .and_then(|z| z.rank(&args[2]))
                .map_or(Frame::Null, |r| Frame::Int(r as i64)),
            "ZRANGE" => {
                let (start, stop) = (int_arg(&args[2])?, int_arg(&args[3])?);
                let scores = with_scores(args.get(4))?;
                let Some(set) = self.sorted(&args[1], false)? else {
                    return Ok(bulks([]));
                };
                match clamp_range(start, stop, set.len()) {
                    Some((a, b)) => members(set.range(a, b - a + 1), scores),
                    None => bulks([]),
                }
            }
            "ZRANGEBYSCORE" | "ZCOUNT" => {
                let (low, high) = (bound(&args[2])?, bound(&args[3])?);
                let scores = with_scores(args.get(4))?;
                let (start, end) = match self.sorted(&args[1], false)? {
                    Some(set) => {
                        let start = set.count_while(|s| s < low.0 || (low.1 && s == low.0));
                        let end = set.count_while(|s| s < high.0 || (!high.1 && s == high.0));
                        (start, end.max(start))
                    }
                    None => (0, 0),
                };
                if name == "ZCOUNT" {
                    return Ok(Frame::Int((end - start) as i64));
                }
                match self.sorted(&args[1], false)? {
                    Some(set) => members(set.range(start, end - start), scores),
                    None => bulks([]),
                }
            }
            "ZINCRBY" => {
                let by = score_arg(&args[2])?;
                let old = self
                    .sorted(&args[1], false)?
                    .and_then(|z| z.score(&args[3]));
                let new = old.unwrap_or(0.0) + by;
                if new.is_nan() {
                    return Err(error("ERR resulting score is not a number (NaN)"));
                }
                if old != Some(new) {
                    let set = self.sorted(&args[1], true)?.expect("created");
                    set.insert(&args[3], new + 0.0);
                    self.after_write(&args[1]);
                }
                score_frame(new + 0.0)
            }
            "KEYS" => {
                let live = self.map.keys().filter(|k| !self.expired(k));
                bulks(live.filter(|k| glob_matches(&args[1], k)).cloned())
            }
            "SCAN" => self.scan(args)?,
            _ => return Err(error("ERR unknown command")),
        })
    }

    /// The cursor is the last key looked at, so keys that come and go cannot shift it.
    fn scan(&mut self, args: &[Vec<u8>]) -> Reply {
        let bad_cursor = || error("ERR invalid cursor");
        let after = match args[1].as_slice() {
            b"0" => None,
            [b'1', hex @ ..] => Some(from_hex(hex).ok_or_else(bad_cursor)?),
            _ => return Err(bad_cursor()),
        };
        let (mut count, mut pattern) = (10, None);
        for option in args[2..].chunks(2) {
            let value = option.get(1).ok_or_else(|| error(SYNTAX))?;
            match upper(&option[0]).as_str() {
                "COUNT" => {
                    count = usize::try_from(int_arg(value)?).unwrap_or(0);
                    if count == 0 {
                        return Err(error(SYNTAX));
                    }
                }
                "MATCH" => pattern = Some(value),
                _ => return Err(error(SYNTAX)),
            }
        }
        use std::ops::Bound;
        let from = after.map_or(Bound::Unbounded, Bound::Excluded);
        let mut keys = self.map.range((from, Bound::Unbounded)).map(|(k, _)| k);
        let (mut found, mut last) = (Vec::new(), None);
        for key in keys.by_ref().take(count) {
            last = Some(key);
            if !self.expired(key) && pattern.is_none_or(|p| glob_matches(p, key)) {
                found.push(key.clone());
            }
        }
        let cursor = match (keys.next(), last) {
            (Some(_), Some(last)) => {
                let hex: String = last.iter().map(|b| format!("{b:02x}")).collect();
                format!("1{hex}")
            }
            _ => "0".to_string(),
        };
        Ok(Frame::Array(vec![
            Frame::Bulk(cursor.into_bytes()),
            bulks(found),
        ]))
    }
}

fn from_hex(hex: &[u8]) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    hex.chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

/// A score: any `f64` but NaN, with `inf`, `+inf` and `-inf`.
fn parse_score(arg: &[u8]) -> Option<f64> {
    let score: f64 = std::str::from_utf8(arg).ok()?.parse().ok()?;
    // adding 0.0 turns -0.0 into 0.0
    (!score.is_nan()).then_some(score + 0.0)
}

fn score_arg(arg: &[u8]) -> Result<f64, Frame> {
    parse_score(arg).ok_or_else(|| error(NOT_FLOAT))
}

/// An end of a score range, and whether it is excluded.
fn bound(arg: &[u8]) -> Result<(f64, bool), Frame> {
    let (excluded, number) = match arg.strip_prefix(b"(") {
        Some(rest) => (true, rest),
        None => (false, arg),
    };
    let score = parse_score(number).ok_or_else(|| error("ERR min or max is not a float"))?;
    Ok((score, excluded))
}

fn with_scores(option: Option<&Vec<u8>>) -> Result<bool, Frame> {
    match option {
        None => Ok(false),
        Some(word) if upper(word) == "WITHSCORES" => Ok(true),
        Some(_) => Err(error(SYNTAX)),
    }
}

fn score_text(score: f64) -> Vec<u8> {
    score.to_string().into_bytes()
}

fn score_frame(score: f64) -> Frame {
    Frame::Bulk(score_text(score))
}

fn members(items: Vec<(Vec<u8>, f64)>, scores: bool) -> Frame {
    bulks(items.into_iter().flat_map(|(member, score)| {
        let score = scores.then(|| score_text(score));
        std::iter::once(member).chain(score)
    }))
}

// ===================================================================== clients (step 5 on)

pub type ClientId = u64;

/// A frame the core wants sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub to: ClientId,
    pub frame: Frame,
    /// This is a published message, not a reply to the client's own command.
    pub published: bool,
    /// Close the connection after sending this.
    pub close: bool,
}
#[derive(Debug, Default)]
struct Client {
    /// The commands of an open transaction.
    queue: Option<Vec<Args>>,
    /// A command was refused while queuing.
    aborted: bool,
    watched: BTreeSet<Vec<u8>>,
    /// A watched key has changed.
    stale: bool,
    /// Commands sent while this client was blocked.
    held: VecDeque<Args>,
    channels: BTreeSet<Vec<u8>>,
    patterns: BTreeSet<Vec<u8>>,
}

impl Client {
    fn subscriptions(&self) -> usize {
        self.channels.len() + self.patterns.len()
    }
}

/// For each key, channel or pattern, the clients that asked about it.
type Listeners = BTreeMap<Vec<u8>, BTreeSet<ClientId>>;

/// The commands that need to know who is asking.
fn core_arity(name: &str) -> Option<(usize, usize)> {
    Some(match name {
        "MULTI" | "EXEC" | "DISCARD" | "UNWATCH" => (1, 1),
        "WATCH" => (2, ANY),
        "QUIT" => (1, ANY),
        "BLPOP" | "BRPOP" => (3, ANY),
        "SUBSCRIBE" | "PSUBSCRIBE" => (2, ANY),
        "UNSUBSCRIBE" | "PUNSUBSCRIBE" => (1, ANY),
        "PUBLISH" => (3, 3),
        _ => return None,
    })
}

/// The commands of many clients over one keyspace.
#[derive(Debug, Default)]
pub struct Core {
    engine: Engine,
    clients: HashMap<ClientId, Client>,
    last_id: ClientId,
    watchers: Listeners,
    /// The ticket says whether to pop from the left.
    waiting: Waiting<bool>,
    /// Changed keys that someone blocked may be able to use.
    ready: VecDeque<Vec<u8>>,
    /// Clients whose held commands can run again.
    woken: VecDeque<ClientId>,
    channels: Listeners,
    patterns: Listeners,
}

impl Core {
    pub fn new() -> Core {
        Core::default()
    }

    /// A new client, with an id never used before.
    pub fn connect(&mut self) -> ClientId {
        self.last_id += 1;
        self.clients.insert(self.last_id, Client::default());
        self.last_id
    }

    pub fn disconnect(&mut self, id: ClientId) {
        self.unwatch(id);
        self.waiting.cancel(id);
        if let Some(client) = self.clients.get(&id) {
            let (channels, patterns) = (client.channels.clone(), client.patterns.clone());
            for (names, listeners) in [
                (channels, &mut self.channels),
                (patterns, &mut self.patterns),
            ] {
                for name in names {
                    leave(listeners, &name, id);
                }
            }
        }
        self.clients.remove(&id);
    }

    /// Active expiry, as `Engine::cycle`.
    pub fn cycle(&mut self, now: u64, dice: &mut dyn Dice) -> usize {
        let removed = self.engine.cycle(now, dice);
        self.note_changes();
        removed
    }

    /// Runs one command of client `id` and returns what must be sent.
    pub fn request(&mut self, id: ClientId, args: Args, now: u64) -> Vec<Delivery> {
        let mut out = Vec::new();
        if args.is_empty() || !self.clients.contains_key(&id) {
            return out;
        }
        if self.waiting.contains(id) {
            if let Some(client) = self.clients.get_mut(&id) {
                client.held.push_back(args);
            }
            return out;
        }
        self.step(id, args, now, &mut out);
        self.settle(now, &mut out);
        out
    }

    /// Answers the blocked clients whose time is up.
    pub fn tick(&mut self, now: u64) -> Vec<Delivery> {
        let mut out = Vec::new();
        for (id, _) in self.waiting.expire(now) {
            out.push(answer(id, Frame::NullArray));
            self.woken.push_back(id);
        }
        self.settle(now, &mut out);
        out
    }

    /// How many clients are blocked.
    pub fn blocked(&self) -> usize {
        self.waiting.len()
    }

    /// Serves blocked clients while changed keys allow it, then runs what woken clients had sent.
    fn settle(&mut self, now: u64, out: &mut Vec<Delivery>) {
        loop {
            if let Some(key) = self.ready.pop_front() {
                while self.engine.list_len(&key, now).unwrap_or(0) > 0 {
                    let Some((id, left)) = self.waiting.wake(&key) else {
                        break;
                    };
                    let frame = self.take_one(&key, left, now);
                    out.push(answer(id, frame));
                    self.woken.push_back(id);
                }
                self.note_changes();
                continue;
            }
            let Some(&id) = self.woken.front() else {
                break;
            };
            let held = match self.clients.get_mut(&id) {
                Some(client) if !self.waiting.contains(id) => client.held.pop_front(),
                _ => None,
            };
            match held {
                Some(args) => self.step(id, args, now, out),
                None => {
                    self.woken.pop_front();
                }
            }
        }
    }

    /// Pops for a blocked pop, as a plain pop would: the reply names the key.
    fn take_one(&mut self, key: &[u8], left: bool, now: u64) -> Frame {
        let name: &[u8] = if left { b"LPOP" } else { b"RPOP" };
        let value = self.engine.execute(&[name.to_vec(), key.to_vec()], now);
        Frame::Array(vec![Frame::Bulk(key.to_vec()), value])
    }

    /// BLPOP and BRPOP. `None` means the client now waits.
    fn blocking_pop(
        &mut self,
        id: ClientId,
        args: &[Vec<u8>],
        left: bool,
        now: u64,
    ) -> Option<Frame> {
        // `f64` only checks that the text is a finite number; its value comes from the digits,
        // because in `f64` a tiny timeout such as 1e-400 is 0, which would mean forever
        let finite = std::str::from_utf8(&args[args.len() - 1])
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .is_some_and(|t| t.is_finite());
        if !finite {
            return Some(error("ERR timeout is not a float or out of range"));
        }
        let (negative, millis) = Self::timeout_millis(&args[args.len() - 1]);
        if negative && millis > 0 {
            return Some(error("ERR timeout is negative"));
        }
        let keys = &args[1..args.len() - 1];
        for key in keys {
            match self.engine.list_len(key, now) {
                Err(refusal) => return Some(refusal),
                Ok(0) => {}
                Ok(_) => return Some(self.take_one(key, left, now)),
            }
        }
        if self.clients.get(&id).is_some_and(|c| c.queue.is_some()) {
            return Some(Frame::NullArray);
        }
        // the milliseconds saturate, and so must the addition
        let deadline = (millis > 0).then(|| now.saturating_add(millis));
        let mut unique = Vec::new();
        let mut seen = BTreeSet::new();
        for key in keys {
            if seen.insert(key) {
                unique.push(key.clone());
            }
        }
        self.waiting.park(id, &unique, deadline, left);
        None
    }

    /// Whether a timeout has a minus sign, and its milliseconds rounded up, worked out from its digits.
    /// `403e-2` is 4030, while `4.03 * 1000.0` in `f64` is a little above 4030, and `1e-400` is 1.
    /// The milliseconds are 0 only when every digit is 0. The text has already parsed as a finite number.
    fn timeout_millis(text: &[u8]) -> (bool, u64) {
        let (negative, text) = match text.split_first() {
            Some((b'-', rest)) => (true, rest),
            Some((b'+', rest)) => (false, rest),
            _ => (false, text),
        };
        let (number, exponent) = match text.iter().position(|&b| b == b'e' || b == b'E') {
            Some(at) => (&text[..at], &text[at + 1..]),
            None => (text, &text[text.len()..]),
        };
        let (down, exponent) = match exponent.split_first() {
            Some((b'-', rest)) => (true, rest),
            Some((b'+', rest)) => (false, rest),
            _ => (false, exponent),
        };
        // a huge exponent only makes the result saturate or round up to 1, so it is capped
        let exponent = exponent
            .iter()
            .fold(0i64, |e, &d| (e * 10 + i64::from(d - b'0')).min(1 << 20));
        let exponent = if down { -exponent } else { exponent };
        let point = number
            .iter()
            .position(|&b| b == b'.')
            .unwrap_or(number.len());
        let digits: Vec<u64> = number
            .iter()
            .filter(|&&b| b != b'.')
            .map(|&b| u64::from(b - b'0'))
            .collect();
        // the first `whole` digits, padded with zeros, are the whole milliseconds
        let whole = point as i64 + exponent + 3;
        let mut millis: u64 = 0;
        let mut part = false;
        for (i, &d) in digits.iter().enumerate() {
            if (i as i64) < whole {
                millis = millis.saturating_mul(10).saturating_add(d);
            } else if d != 0 {
                part = true;
            }
        }
        for _ in digits.len() as i64..whole {
            if millis == u64::MAX {
                break;
            }
            millis = millis.saturating_mul(10);
        }
        (negative, millis.saturating_add(u64::from(part)))
    }

    /// Marks the watchers of every key that changed.
    fn note_changes(&mut self) {
        for key in std::mem::take(&mut self.engine.changed) {
            for id in self.watchers.get(&key).into_iter().flatten() {
                if let Some(client) = self.clients.get_mut(id) {
                    client.stale = true;
                }
            }
            self.ready.push_back(key);
        }
    }

    fn unwatch(&mut self, id: ClientId) {
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        client.stale = false;
        for key in std::mem::take(&mut client.watched) {
            leave(&mut self.watchers, &key, id);
        }
    }

    /// One command of a client that is free to run it.
    fn step(&mut self, id: ClientId, args: Args, now: u64, out: &mut Vec<Delivery>) {
        let name = upper(&args[0]);
        let refusal = match core_arity(&name) {
            Some(limits) => arity_error(&name, args.len(), limits),
            None => check_command(&args),
        };
        let subscriber = self.clients.get(&id).is_some_and(|c| c.subscriptions() > 0);
        let allowed = matches!(
            name.as_str(),
            "SUBSCRIBE" | "UNSUBSCRIBE" | "PSUBSCRIBE" | "PUNSUBSCRIBE" | "PING" | "QUIT"
        );
        if subscriber && !allowed {
            let text = format!(
                "ERR Can't execute '{}': only (P)SUBSCRIBE / (P)UNSUBSCRIBE / PING / QUIT are allowed in this context",
                name.to_ascii_lowercase()
            );
            out.push(answer(id, Frame::Error(text)));
            return;
        }
        if name == "QUIT" {
            let mut last = answer(id, ok());
            last.close = true;
            out.push(last);
            self.disconnect(id);
            return;
        }
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        let control = matches!(name.as_str(), "MULTI" | "EXEC" | "DISCARD" | "WATCH");
        if client.queue.is_some() && !control {
            let refusal = refusal.or_else(|| {
                let not_here = format!("ERR {name} is not allowed in transactions");
                name.ends_with("SUBSCRIBE").then(|| error(&not_here))
            });
            let frame = match refusal {
                Some(refusal) => {
                    client.aborted = true;
                    refusal
                }
                None => {
                    client.queue.get_or_insert_default().push(args);
                    Frame::Simple("QUEUED".to_string())
                }
            };
            out.push(answer(id, frame));
            return;
        }
        if let Some(refusal) = refusal {
            // a refused MULTI, EXEC, DISCARD or WATCH leaves an open transaction as it was
            out.push(answer(id, refusal));
            return;
        }
        match name.as_str() {
            "SUBSCRIBE" | "PSUBSCRIBE" => {
                self.subscribe(id, &args[1..], name == "PSUBSCRIBE", out);
                return;
            }
            "UNSUBSCRIBE" | "PUNSUBSCRIBE" => {
                self.unsubscribe(id, &args[1..], name == "PUNSUBSCRIBE", out);
                return;
            }
            "PING" if subscriber => {
                let message = args.get(1).cloned().unwrap_or_default();
                out.push(answer(id, bulks([b"pong".to_vec(), message])));
                return;
            }
            _ => {}
        }
        let frame = match name.as_str() {
            "MULTI" => Some(self.multi(id)),
            "EXEC" => Some(self.exec(id, now, out)),
            "PUBLISH" => Some(Frame::Int(self.publish(&args[1], &args[2], out))),
            "DISCARD" => Some(self.discard(id)),
            "WATCH" => Some(self.watch(id, &args[1..], now)),
            "UNWATCH" => {
                self.unwatch(id);
                Some(ok())
            }
            "BLPOP" | "BRPOP" => {
                let frame = self.blocking_pop(id, &args, name == "BLPOP", now);
                self.note_changes();
                frame
            }
            _ => Some(self.apply(&args, now)),
        };
        if let Some(frame) = frame {
            out.push(answer(id, frame));
        }
    }

    /// A command for the keyspace alone.
    fn apply(&mut self, args: &[Vec<u8>], now: u64) -> Frame {
        let frame = self.engine.execute(args, now);
        self.note_changes();
        frame
    }

    fn multi(&mut self, id: ClientId) -> Frame {
        let client = self.clients.entry(id).or_default();
        if client.queue.is_some() {
            return error("ERR MULTI calls can not be nested");
        }
        client.queue = Some(Vec::new());
        ok()
    }

    fn discard(&mut self, id: ClientId) -> Frame {
        let client = self.clients.entry(id).or_default();
        if client.queue.take().is_none() {
            return error("ERR DISCARD without MULTI");
        }
        client.aborted = false;
        self.unwatch(id);
        ok()
    }

    fn watch(&mut self, id: ClientId, keys: &[Vec<u8>], now: u64) -> Frame {
        if self.clients.get(&id).is_some_and(|c| c.queue.is_some()) {
            return error("ERR WATCH inside MULTI is not allowed");
        }
        for key in keys {
            // a key that is already past its deadline goes now, so that it does not count later
            self.engine.expire_now(key, now);
            self.note_changes();
            self.watchers.entry(key.clone()).or_default().insert(id);
            if let Some(client) = self.clients.get_mut(&id) {
                client.watched.insert(key.clone());
            }
        }
        ok()
    }

    fn exec(&mut self, id: ClientId, now: u64, out: &mut Vec<Delivery>) -> Frame {
        let client = self.clients.entry(id).or_default();
        if client.queue.is_none() {
            return error("ERR EXEC without MULTI");
        }
        let watched: Vec<_> = client.watched.iter().cloned().collect();
        // a watched key may have expired without anyone touching it
        for key in &watched {
            self.engine.expire_now(key, now);
        }
        self.note_changes();
        let Some(client) = self.clients.get_mut(&id) else {
            return Frame::NullArray;
        };
        // the queue stays in place while its commands run, so that a blocking pop knows not to wait
        let commands = client.queue.replace(Vec::new()).unwrap_or_default();
        let (aborted, stale) = (std::mem::take(&mut client.aborted), client.stale);
        self.unwatch(id);
        let frame = if aborted {
            error("EXECABORT Transaction discarded because of previous errors.")
        } else if stale {
            Frame::NullArray
        } else {
            let mut replies = Vec::with_capacity(commands.len());
            for command in &commands {
                replies.push(self.run_queued(id, command, now, out));
            }
            Frame::Array(replies)
        };
        if let Some(client) = self.clients.get_mut(&id) {
            client.queue = None;
        }
        frame
    }

    fn run_queued(
        &mut self,
        id: ClientId,
        command: &[Vec<u8>],
        now: u64,
        out: &mut Vec<Delivery>,
    ) -> Frame {
        let name = upper(&command[0]);
        match name.as_str() {
            "UNWATCH" => ok(),
            "BLPOP" | "BRPOP" => {
                let frame = self.blocking_pop(id, command, name == "BLPOP", now);
                self.note_changes();
                frame.unwrap_or(Frame::NullArray)
            }
            "PUBLISH" => Frame::Int(self.publish(&command[1], &command[2], out)),
            _ => self.apply(command, now),
        }
    }

    fn subscribe(
        &mut self,
        id: ClientId,
        names: &[Vec<u8>],
        pattern: bool,
        out: &mut Vec<Delivery>,
    ) {
        let kind: &[u8] = if pattern { b"psubscribe" } else { b"subscribe" };
        for name in names {
            let Some(client) = self.clients.get_mut(&id) else {
                return;
            };
            let (mine, listeners) = if pattern {
                (&mut client.patterns, &mut self.patterns)
            } else {
                (&mut client.channels, &mut self.channels)
            };
            mine.insert(name.clone());
            listeners.entry(name.clone()).or_default().insert(id);
            let count = client.subscriptions();
            out.push(answer(id, notice(kind, Some(name), count)));
        }
    }

    fn unsubscribe(
        &mut self,
        id: ClientId,
        names: &[Vec<u8>],
        pattern: bool,
        out: &mut Vec<Delivery>,
    ) {
        let kind: &[u8] = if pattern {
            b"punsubscribe"
        } else {
            b"unsubscribe"
        };
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        let (mine, listeners) = if pattern {
            (&mut client.patterns, &mut self.patterns)
        } else {
            (&mut client.channels, &mut self.channels)
        };
        let names: Vec<_> = if names.is_empty() {
            mine.iter().cloned().collect()
        } else {
            names.to_vec()
        };
        if names.is_empty() {
            let count = client.subscriptions();
            out.push(answer(id, notice(kind, None, count)));
            return;
        }
        for name in names {
            let Some(client) = self.clients.get_mut(&id) else {
                return;
            };
            let mine = if pattern {
                &mut client.patterns
            } else {
                &mut client.channels
            };
            mine.remove(&name);
            leave(listeners, &name, id);
            let count = client.subscriptions();
            out.push(answer(id, notice(kind, Some(&name), count)));
        }
    }

    /// Queues a message for everyone listening and returns how many frames that made.
    fn publish(&mut self, channel: &[u8], payload: &[u8], out: &mut Vec<Delivery>) -> i64 {
        let start = out.len();
        let message = |to, parts: &[&[u8]]| Delivery {
            to,
            frame: bulks(parts.iter().map(|p| p.to_vec())),
            published: true,
            close: false,
        };
        for &to in self.channels.get(channel).into_iter().flatten() {
            out.push(message(to, &[b"message", channel, payload]));
        }
        for (pattern, listeners) in &self.patterns {
            if glob_matches(pattern, channel) {
                for &to in listeners {
                    out.push(message(to, &[b"pmessage", pattern, channel, payload]));
                }
            }
        }
        (out.len() - start) as i64
    }
    /// The records the log needs for everything since the last call.
    pub fn take_log(&mut self) -> Vec<Args> {
        todo!()
    }
}

/// Removes `id` from one name's set, and the name with it when nobody is left.
fn leave(map: &mut Listeners, name: &[u8], id: ClientId) {
    if let Some(set) = map.get_mut(name) {
        set.remove(&id);
        if set.is_empty() {
            map.remove(name);
        }
    }
}

/// The confirmation of a (un)subscribe.
fn notice(kind: &[u8], name: Option<&Vec<u8>>, count: usize) -> Frame {
    Frame::Array(vec![
        Frame::Bulk(kind.to_vec()),
        name.map_or(Frame::Null, |n| Frame::Bulk(n.clone())),
        Frame::Int(count as i64),
    ])
}

fn answer(to: ClientId, frame: Frame) -> Delivery {
    Delivery {
        to,
        frame,
        published: false,
        close: false,
    }
}

impl Engine {
    /// Applies expiry to a key without doing anything else to it.
    fn expire_now(&mut self, key: &[u8], now: u64) {
        self.now = now;
        self.purge(key);
    }
}

impl Engine {
    /// The length of the list at `key`, 0 when there is no key.
    fn list_len(&mut self, key: &[u8], now: u64) -> Result<usize, Frame> {
        self.now = now;
        Ok(self.list(key, false)?.map_or(0, |l| l.len()))
    }
}

// ===================================================================== files

/// The few file operations persistence needs, so that tests can make them fail.
pub trait Disk {
    /// The file's bytes, or `None` when there is no such file.
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>>;
    /// Adds bytes at the end, creating the file if needed. On an error, any prefix may have been added.
    fn append(&mut self, name: &str, bytes: &[u8]) -> io::Result<()>;
    /// Cuts the file down to `len` bytes.
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()>;
    /// Makes the file survive a power cut: its bytes and, if it is new, its name.
    fn sync(&mut self, name: &str) -> io::Result<()>;
    /// Renames `from` to `to` in one step, replacing `to`. The new name survives a power cut.
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
    /// Removes the file. A missing file is not an error.
    fn remove(&mut self, name: &str) -> io::Result<()>;
}

/// A real directory.
#[derive(Debug, Clone)]
pub struct DirDisk {
    dir: PathBuf,
}

impl DirDisk {
    pub fn new(dir: impl Into<PathBuf>) -> DirDisk {
        DirDisk { dir: dir.into() }
    }
}

impl Disk for DirDisk {
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.dir.join(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn append(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        let mut file = options
            .create(true)
            .append(true)
            .open(self.dir.join(name))?;
        file.write_all(bytes)
    }
    fn truncate(&mut self, name: &str, len: u64) -> io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(self.dir.join(name))?;
        file.set_len(len)
    }
    fn sync(&mut self, name: &str) -> io::Result<()> {
        std::fs::File::open(self.dir.join(name))?.sync_all()?;
        // `append` may have created the file, so the directory that holds its name is synced too.
        // No test checks this line: that would take a real power cut.
        std::fs::File::open(&self.dir)?.sync_all()
    }
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(self.dir.join(from), self.dir.join(to))?;
        std::fs::File::open(&self.dir)?.sync_all()
    }
    fn remove(&mut self, name: &str) -> io::Result<()> {
        match std::fs::remove_file(self.dir.join(name)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// The name of the log file.
pub const AOF: &str = "appendonly.aof";

/// When the log is synced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPolicy {
    /// After every write, before it returns.
    Always,
    /// Never: the operating system decides.
    Never,
}

#[derive(Debug)]
pub enum AofError {
    Io(io::Error),
    /// The log holds something that is not a torn end. The number is where that record starts.
    Corrupt(u64),
    /// An earlier write failed, so the log takes no more.
    Broken,
}

impl From<io::Error> for AofError {
    fn from(e: io::Error) -> AofError {
        AofError::Io(e)
    }
}

/// The append-only log of a core.
pub struct Aof<D: Disk> {
    disk: D,
    policy: SyncPolicy,
    // add what you need
}

impl<D: Disk> Aof<D> {
    /// Rebuilds a core from the log and returns both.
    pub fn open(disk: D, policy: SyncPolicy) -> Result<(Aof<D>, Core), AofError> {
        todo!()
    }

    /// Adds records to the log.
    pub fn write(&mut self, records: &[Args]) -> Result<(), AofError> {
        todo!()
    }
}
