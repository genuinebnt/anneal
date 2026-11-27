use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

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

// ===================================================================== the keyspace

const WRONGTYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";
const NOT_INT: &str = "ERR value is not an integer or out of range";
const OVERFLOW: &str = "ERR increment or decrement would overflow";
const SYNTAX: &str = "ERR syntax error";

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
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Hash(_) => "hash",
            Value::Set(_) => "set",
        }
    }

    /// A key never holds an empty collection.
    fn is_empty_collection(&self) -> bool {
        match self {
            Value::Str(_) => false,
            Value::List(l) => l.is_empty(),
            Value::Hash(h) => h.is_empty(),
            Value::Set(s) => s.is_empty(),
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
        let removed = self.deadlines.sweep(now, dice, usize::MAX);
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
            _ => return Err(error("ERR unknown command")),
        })
    }
}
