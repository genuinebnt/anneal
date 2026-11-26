//! RECIPE 914 (networking): Protocol Buffers wire format by hand (schema parser, reflective codec, compatibility rules) and the gRPC framing layer
//! LEVEL: advanced
//! Use in: debugging "why did this field come back as 0", reviewing .proto changes, writing codecs or proxies, understanding gRPC streaming.   Run: cargo run --release --example protobuf_grpc_wire_format
//! COMMON USES: gRPC services, Kafka payloads with a schema registry, on-disk records; `prost` and `protobuf` generate typed code for what this file does reflectively.
#![allow(dead_code)]
//!
//! WIRE FORMAT: a message is a flat sequence of (tag, payload). tag = varint(field_number << 3 | wire_type). Wire types: 0 VARINT,
//! 1 I64, 2 LEN, 3/4 SGROUP/EGROUP (deprecated, still skippable), 5 I32. NAMES AND DECLARED TYPES ARE NOT ON THE WIRE, only numbers
//! and these five shapes. Every compatibility rule follows from that:
//!   - rename or reorder declarations: bytes identical. Change a NUMBER: a different field.
//!   - int32/int64/uint32/uint64/bool share the VARINT encoding: interchangeable, but values are TRUNCATED to the reader's width.
//!   - sint32/sint64 use ZigZag: NOT compatible with int32 (same shape, different meaning: silently wrong numbers).
//!   - a different wire type for a known number is treated like an unknown field: the reader sees the default.
//!   - reusing a deleted number is silent corruption; declare it `reserved`.
//! PARSING RULES a codec must honour: any field order; unknown fields are KEPT and re-emitted (an old proxy must not strip new
//! data); a repeated singular scalar means LAST ONE WINS; repeated singular message occurrences MERGE; packed and expanded
//! repeated encodings must both be accepted; in a oneof the last member wins; hence parse(a ++ b) == merge(parse(a), parse(b)).
//! PRESENCE: a proto3 scalar without `optional` has IMPLICIT presence: the default (0, "", false) is not sent, so "unset" and "zero"
//! are indistinguishable. `optional`, oneof members and message fields have EXPLICIT presence.
//! DECODER SAFETY: lengths are checked against the remaining input before allocating; nesting depth is capped.
//! gRPC: each message is [compressed flag: 1 byte][length: 4 bytes big endian][bytes]. These are carried in HTTP/2 DATA frames whose
//! boundaries are UNRELATED to message boundaries. The status travels in TRAILERS (`grpc-status`, percent-encoded `grpc-message`),
//! the deadline in `grpc-timeout`. HTTP/2 itself is abstracted here: frames are enum values on in-memory queues, with ONE per-stream
//! flow-control window (no HPACK, no connection window, no TLS).
//! SIMPLIFICATIONS: proto3 subset (no imports, packages, services, field options or well-known types); enums decode as open int32.
//! The hex vectors marked DOCS were copied from the official encoding guide (first link).
//! Read: https://protobuf.dev/programming-guides/encoding/ and https://protobuf.dev/programming-guides/proto3/#updating
//! Read: https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-HTTP2.md and https://httpwg.org/specs/rfc9113.html#FlowControl
//!
//! TRY NEXT:
//!   - [S] In `Schema::decode_into` make a repeated singular message REPLACE instead of merge. Which test fails first?
//!   - [S] Stop re-emitting unknown fields in `encode`. Predict which compatibility test breaks and what the user-visible bug would be.
//!   - [M] Add `json_name`-style printing (proto3 JSON mapping: int64 as string, bytes as base64) and round-trip it.
//!   - [M] Add a connection-level flow-control window shared by two streams and show how one stalled reader can starve the other if the connection window is not replenished.
//!   - [A] PROJECT: generate Rust structs from the parsed schema and check byte-for-byte equality against this reflective codec on 10000 random messages (and against `prost` if you add it).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ===========================================================================
// 1. primitives
// ===========================================================================
#[derive(Debug, PartialEq, Clone, Copy)]
enum WireError {
    Truncated,
    VarintOverflow,
    BadFieldNumber,
    BadWireType,
    UnbalancedGroup,
    TooDeep,
    BadUtf8,
}
const VARINT: u8 = 0;
const I64: u8 = 1;
const LEN: u8 = 2;
const SGROUP: u8 = 3;
const EGROUP: u8 = 4;
const I32: u8 = 5;
const MAX_DEPTH: u32 = 100;
const MAX_FIELD: u64 = (1 << 29) - 1;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn get_varint(buf: &mut &[u8]) -> Result<u64, WireError> {
    let mut v = 0u64;
    for i in 0..10 {
        let (&b, rest) = buf.split_first().ok_or(WireError::Truncated)?;
        *buf = rest;
        // the 10th byte may only contribute the 64th bit
        if i == 9 && b > 1 {
            return Err(WireError::VarintOverflow);
        }
        v |= u64::from(b & 0x7f) << (7 * i);
        if b < 0x80 {
            return Ok(v);
        }
    }
    Err(WireError::VarintOverflow)
}
fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}
fn unzigzag(v: u64) -> i64 {
    (v >> 1) as i64 ^ -((v & 1) as i64)
}
fn take<'a>(buf: &mut &'a [u8], n: u64) -> Result<&'a [u8], WireError> {
    if n > buf.len() as u64 {
        return Err(WireError::Truncated); // checked BEFORE anything is allocated
    }
    let (head, tail) = buf.split_at(n as usize);
    *buf = tail;
    Ok(head)
}
fn get_tag(buf: &mut &[u8]) -> Result<(u32, u8), WireError> {
    let tag = get_varint(buf)?;
    let (number, wire) = (tag >> 3, (tag & 7) as u8);
    if number == 0 || number > MAX_FIELD {
        return Err(WireError::BadFieldNumber);
    }
    if wire > I32 {
        return Err(WireError::BadWireType);
    }
    Ok((number as u32, wire))
}
fn put_tag(out: &mut Vec<u8>, number: u32, wire: u8) {
    put_varint(out, u64::from(number) << 3 | u64::from(wire));
}
/// Consume the payload of a record whose tag has been read.
fn skip(buf: &mut &[u8], number: u32, wire: u8, depth: u32) -> Result<(), WireError> {
    match wire {
        VARINT => drop(get_varint(buf)?),
        I64 => drop(take(buf, 8)?),
        I32 => drop(take(buf, 4)?),
        LEN => {
            let n = get_varint(buf)?;
            take(buf, n)?;
        }
        SGROUP => {
            if depth >= MAX_DEPTH {
                return Err(WireError::TooDeep);
            }
            loop {
                let (n, w) = get_tag(buf)?;
                if w == EGROUP {
                    return if n == number {
                        Ok(())
                    } else {
                        Err(WireError::UnbalancedGroup)
                    };
                }
                skip(buf, n, w, depth + 1)?;
            }
        }
        _ => return Err(WireError::UnbalancedGroup), // a stray EGROUP
    }
    Ok(())
}

// ===========================================================================
// 2. schema and .proto subset parser
// ===========================================================================
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Int32,
    Int64,
    Uint32,
    Uint64,
    Sint32,
    Sint64,
    Bool,
    Enum,
    Fixed32,
    Sfixed32,
    Float,
    Fixed64,
    Sfixed64,
    Double,
    Str,
    Bytes,
    Message(String),
}
impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        use Kind::*;
        Some(match s {
            "int32" => Int32,
            "int64" => Int64,
            "uint32" => Uint32,
            "uint64" => Uint64,
            "sint32" => Sint32,
            "sint64" => Sint64,
            "bool" => Bool,
            "fixed32" => Fixed32,
            "sfixed32" => Sfixed32,
            "float" => Float,
            "fixed64" => Fixed64,
            "sfixed64" => Sfixed64,
            "double" => Double,
            "string" => Str,
            "bytes" => Bytes,
            _ => return None,
        })
    }
    fn wire(&self) -> u8 {
        use Kind::*;
        match self {
            Int32 | Int64 | Uint32 | Uint64 | Sint32 | Sint64 | Bool | Enum => VARINT,
            Fixed32 | Sfixed32 | Float => I32,
            Fixed64 | Sfixed64 | Double => I64,
            Str | Bytes | Message(_) => LEN,
        }
    }
    fn packable(&self) -> bool {
        self.wire() != LEN
    }
}
#[derive(Debug, Clone, PartialEq)]
enum Label {
    Implicit,
    Explicit,
    Repeated,
    Map { key: Kind, value: Kind },
}
#[derive(Debug, Clone)]
struct Field {
    name: String,
    kind: Kind,
    label: Label,
    oneof: Option<String>,
}
#[derive(Debug, Default)]
struct MessageDesc {
    fields: BTreeMap<u32, Field>,
    reserved_numbers: HashSet<u32>,
    reserved_names: HashSet<String>,
}
#[derive(Debug, Default)]
struct Schema {
    messages: HashMap<String, MessageDesc>,
    enums: HashSet<String>,
}
fn tokenize(src: &str) -> Result<Vec<String>, String> {
    let mut out = vec![];
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
        } else if c == '/' && chars.peek() == Some(&'/') {
            chars.by_ref().find(|&c| c == '\n');
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut previous = ' ';
            if !chars
                .by_ref()
                .any(|c| std::mem::replace(&mut previous, c) == '*' && c == '/')
            {
                return Err("unterminated comment".into());
            }
        } else if c == '"' {
            let mut s = String::from("\"");
            loop {
                let c = chars.next().ok_or("unterminated string")?;
                if c == '"' {
                    break;
                }
                s.push(c);
            }
            out.push(s);
        } else if c.is_alphanumeric() || c == '_' {
            let mut s = c.to_string();
            while let Some(&n) = chars
                .peek()
                .filter(|n| n.is_alphanumeric() || **n == '_' || **n == '.')
            {
                s.push(n);
                chars.next();
            }
            out.push(s);
        } else if "{}=;<>,".contains(c) {
            out.push(c.to_string());
        } else {
            return Err(format!("unexpected character {c:?}"));
        }
    }
    Ok(out)
}
struct Parser {
    tokens: VecDeque<String>,
    schema: Schema,
}
impl Parser {
    fn next(&mut self) -> Result<String, String> {
        self.tokens
            .pop_front()
            .ok_or_else(|| "unexpected end of file".to_string())
    }
    fn expect(&mut self, want: &str) -> Result<(), String> {
        let got = self.next()?;
        if got == want {
            Ok(())
        } else {
            Err(format!("expected {want:?}, found {got:?}"))
        }
    }
    fn number(&mut self) -> Result<u32, String> {
        let t = self.next()?;
        t.parse()
            .map_err(|_| format!("expected a number, found {t:?}"))
    }
    fn message(&mut self) -> Result<(), String> {
        let name = self.next()?;
        self.expect("{")?;
        let mut desc = MessageDesc::default();
        let mut oneof: Option<String> = None;
        loop {
            let t = self.next()?;
            match t.as_str() {
                "}" if oneof.is_some() => oneof = None,
                "}" => break,
                "message" => self.message()?,
                "enum" => self.enumeration()?,
                "oneof" => {
                    oneof = Some(self.next()?);
                    self.expect("{")?;
                }
                "reserved" => loop {
                    let item = self.next()?;
                    if let Some(n) = item.strip_prefix('"') {
                        desc.reserved_names.insert(n.to_string());
                    } else {
                        let from: u32 =
                            item.parse().map_err(|_| format!("bad reserved {item:?}"))?;
                        let mut to = from;
                        if self.tokens.front().is_some_and(|t| t == "to") {
                            self.next()?;
                            to = self.number()?;
                        }
                        if to < from || to - from > 10_000 {
                            return Err("unsupported reserved range".into());
                        }
                        desc.reserved_numbers.extend(from..=to);
                    }
                    match self.next()?.as_str() {
                        "," => {}
                        ";" => break,
                        other => return Err(format!("expected , or ; found {other:?}")),
                    }
                },
                _ => self.field(t, &oneof, &mut desc)?,
            }
        }
        for (n, f) in &desc.fields {
            if desc.reserved_numbers.contains(n) || desc.reserved_names.contains(&f.name) {
                return Err(format!(
                    "field {} = {n} uses a reserved number or name",
                    f.name
                ));
            }
        }
        if self.schema.messages.insert(name.clone(), desc).is_some() {
            return Err(format!("duplicate message {name}"));
        }
        Ok(())
    }
    fn field(
        &mut self,
        first: String,
        oneof: &Option<String>,
        desc: &mut MessageDesc,
    ) -> Result<(), String> {
        let mut ty = first;
        let mut label = if oneof.is_some() {
            Label::Explicit
        } else {
            Label::Implicit
        };
        if ty == "optional" || ty == "repeated" {
            if oneof.is_some() {
                return Err("no labels inside oneof".into());
            }
            label = if ty == "optional" {
                Label::Explicit
            } else {
                Label::Repeated
            };
            ty = self.next()?;
        }
        // message type names are resolved after parsing, so forward references work
        let kind_of = |s: &str| Kind::parse(s).unwrap_or_else(|| Kind::Message(s.to_string()));
        let kind = if ty == "map" {
            if label != Label::Implicit || oneof.is_some() {
                return Err("map cannot be repeated, optional or in a oneof".into());
            }
            self.expect("<")?;
            let key = kind_of(&self.next()?);
            self.expect(",")?;
            let value = kind_of(&self.next()?);
            self.expect(">")?;
            if matches!(
                key,
                Kind::Float | Kind::Double | Kind::Bytes | Kind::Message(_)
            ) {
                return Err(format!("illegal map key type {key:?}"));
            }
            label = Label::Map { key, value };
            Kind::Bytes // placeholder: each wire record is a LEN entry
        } else {
            kind_of(&ty)
        };
        let name = self.next()?;
        self.expect("=")?;
        let number = self.number()?;
        self.expect(";")?;
        if number == 0 || u64::from(number) > MAX_FIELD || (19_000..=19_999).contains(&number) {
            return Err(format!("illegal field number {number}"));
        }
        if desc.fields.values().any(|f| f.name == name) {
            return Err(format!("duplicate field name {name}"));
        }
        if desc
            .fields
            .insert(
                number,
                Field {
                    name,
                    kind,
                    label,
                    oneof: oneof.clone(),
                },
            )
            .is_some()
        {
            return Err(format!("duplicate field number {number}"));
        }
        Ok(())
    }
    fn enumeration(&mut self) -> Result<(), String> {
        let name = self.next()?;
        self.expect("{")?;
        let mut first = true;
        loop {
            if self.next()? == "}" {
                break;
            }
            self.expect("=")?;
            let v = self.number()?;
            if first && v != 0 {
                return Err("the first proto3 enum value must be 0".into());
            }
            first = false;
            self.expect(";")?;
        }
        self.schema.enums.insert(name);
        Ok(())
    }
}
fn parse_proto(src: &str) -> Result<Schema, String> {
    let mut p = Parser {
        tokens: tokenize(src)?.into(),
        schema: Schema::default(),
    };
    while let Some(t) = p.tokens.pop_front() {
        match t.as_str() {
            "syntax" => {
                p.expect("=")?;
                p.expect("\"proto3")?;
                p.expect(";")?;
            }
            "message" => p.message()?,
            "enum" => p.enumeration()?,
            other => return Err(format!("unsupported top-level item {other:?}")),
        }
    }
    // resolve named types: enum or message
    let Schema {
        mut messages,
        enums,
    } = p.schema;
    let known: HashSet<String> = messages.keys().cloned().collect();
    let resolve = |k: &mut Kind| match k {
        Kind::Message(n) if enums.contains(n) => {
            *k = Kind::Enum;
            Ok(())
        }
        Kind::Message(n) if !known.contains(n) => Err(format!("unknown type {n}")),
        _ => Ok(()),
    };
    for f in messages.values_mut().flat_map(|m| m.fields.values_mut()) {
        resolve(&mut f.kind)?;
        if let Label::Map { value, .. } = &mut f.label {
            resolve(value)?;
        }
    }
    Ok(Schema { messages, enums })
}

// ===========================================================================
// 3. reflective codec
// ===========================================================================
/// Floats are kept as raw bits so that NaN round-trips and `Eq`/`Ord` are lawful.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Value {
    Int(i64),
    Uint(u64),
    Bool(bool),
    Bits32(u32),
    Bits64(u64),
    Str(String),
    Bytes(Vec<u8>),
    Msg(Message),
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    One(Value),
    Many(Vec<Value>),
    Map(BTreeMap<Value, Value>),
}
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Message {
    fields: BTreeMap<u32, Slot>,
    unknown: Vec<u8>, // complete records, in arrival order
}
impl Message {
    fn with(mut self, n: u32, v: Value) -> Self {
        self.fields.insert(n, Slot::One(v));
        self
    }
    fn many(mut self, n: u32, v: Vec<Value>) -> Self {
        self.fields.insert(n, Slot::Many(v));
        self
    }
    fn get(&self, n: u32) -> Option<&Value> {
        match self.fields.get(&n) {
            Some(Slot::One(v)) => Some(v),
            _ => None,
        }
    }
}
impl Schema {
    fn desc(&self, name: &str) -> &MessageDesc {
        &self.messages[name]
    }
    fn default_of(&self, k: &Kind) -> Value {
        use Kind::*;
        match k {
            Int32 | Int64 | Sint32 | Sint64 | Sfixed32 | Sfixed64 | Enum => Value::Int(0),
            Uint32 | Uint64 => Value::Uint(0),
            Bool => Value::Bool(false),
            Fixed32 | Float => Value::Bits32(0),
            Fixed64 | Double => Value::Bits64(0),
            Str => Value::Str(String::new()),
            Bytes => Value::Bytes(vec![]),
            Message(_) => Value::Msg(Default::default()),
        }
    }
    /// Payload only (no tag).
    fn put_value(&self, out: &mut Vec<u8>, kind: &Kind, v: &Value) {
        let mut delimited = |bytes: &[u8]| {
            put_varint(out, bytes.len() as u64);
            out.extend_from_slice(bytes);
        };
        match (kind, v) {
            (Kind::Str, Value::Str(s)) => delimited(s.as_bytes()),
            (Kind::Bytes, Value::Bytes(b)) => delimited(b),
            (Kind::Message(name), Value::Msg(m)) => delimited(&self.encode(name, m)),
            (Kind::Sint32 | Kind::Sint64, Value::Int(i)) => put_varint(out, zigzag(*i)),
            (Kind::Sfixed32, Value::Int(i)) => out.extend((*i as i32).to_le_bytes()),
            (Kind::Sfixed64, Value::Int(i)) => out.extend(i.to_le_bytes()),
            // negative int32 is SIGN EXTENDED to 64 bits: always ten bytes
            (Kind::Int32 | Kind::Int64 | Kind::Enum, Value::Int(i)) => put_varint(out, *i as u64),
            (Kind::Uint32 | Kind::Uint64, Value::Uint(u)) => put_varint(out, *u),
            (Kind::Bool, Value::Bool(b)) => put_varint(out, u64::from(*b)),
            (Kind::Fixed32 | Kind::Float, Value::Bits32(b)) => out.extend(b.to_le_bytes()),
            (Kind::Fixed64 | Kind::Double, Value::Bits64(b)) => out.extend(b.to_le_bytes()),
            (k, v) => panic!("value {v:?} does not fit kind {k:?}"),
        }
    }
    fn put_record(&self, out: &mut Vec<u8>, number: u32, kind: &Kind, v: &Value) {
        put_tag(out, number, kind.wire());
        self.put_value(out, kind, v);
    }
    fn encode(&self, name: &str, m: &Message) -> Vec<u8> {
        let mut out = vec![];
        for (&number, slot) in &m.fields {
            let field = &self.desc(name).fields[&number];
            match slot {
                Slot::One(v) => {
                    if !(field.label == Label::Implicit
                        && !matches!(field.kind, Kind::Message(_))
                        && *v == self.default_of(&field.kind))
                    {
                        self.put_record(&mut out, number, &field.kind, v);
                    }
                }
                Slot::Many(vs) if field.kind.packable() => {
                    if !vs.is_empty() {
                        let mut packed = vec![];
                        vs.iter()
                            .for_each(|v| self.put_value(&mut packed, &field.kind, v));
                        put_tag(&mut out, number, LEN);
                        put_varint(&mut out, packed.len() as u64);
                        out.extend(packed);
                    }
                }
                Slot::Many(vs) => vs
                    .iter()
                    .for_each(|v| self.put_record(&mut out, number, &field.kind, v)),
                Slot::Map(entries) => {
                    let Label::Map { key, value } = &field.label else {
                        panic!("not a map")
                    };
                    for (k, v) in entries {
                        let mut entry = vec![];
                        self.put_record(&mut entry, 1, key, k);
                        self.put_record(&mut entry, 2, value, v);
                        put_tag(&mut out, number, LEN);
                        put_varint(&mut out, entry.len() as u64);
                        out.extend(entry);
                    }
                }
            }
        }
        out.extend_from_slice(&m.unknown);
        out
    }
    fn get_value(&self, buf: &mut &[u8], kind: &Kind, depth: u32) -> Result<Value, WireError> {
        use Kind::*;
        Ok(match kind {
            // truncate to the declared width, exactly like a C cast
            Int32 | Enum => Value::Int(i64::from(get_varint(buf)? as i32)),
            Int64 => Value::Int(get_varint(buf)? as i64),
            Uint32 => Value::Uint(u64::from(get_varint(buf)? as u32)),
            Uint64 => Value::Uint(get_varint(buf)?),
            Sint32 => Value::Int(i64::from(
                unzigzag(u64::from(get_varint(buf)? as u32)) as i32
            )),
            Sint64 => Value::Int(unzigzag(get_varint(buf)?)),
            Bool => Value::Bool(get_varint(buf)? != 0),
            Fixed32 | Float => Value::Bits32(u32::from_le_bytes(take(buf, 4)?.try_into().unwrap())),
            Sfixed32 => Value::Int(i64::from(i32::from_le_bytes(
                take(buf, 4)?.try_into().unwrap(),
            ))),
            Fixed64 | Double => {
                Value::Bits64(u64::from_le_bytes(take(buf, 8)?.try_into().unwrap()))
            }
            Sfixed64 => Value::Int(i64::from_le_bytes(take(buf, 8)?.try_into().unwrap())),
            Str | Bytes | Message(_) => {
                let n = get_varint(buf)?;
                let bytes = take(buf, n)?;
                match kind {
                    Str => Value::Str(
                        String::from_utf8(bytes.to_vec()).map_err(|_| WireError::BadUtf8)?,
                    ),
                    Message(name) => {
                        let mut m = crate::Message::default();
                        self.decode_into(name, bytes, &mut m, depth + 1)?;
                        Value::Msg(m)
                    }
                    _ => Value::Bytes(bytes.to_vec()),
                }
            }
        })
    }
    /// Parsing INTO an existing message is exactly protobuf's merge.
    fn decode_into(
        &self,
        name: &str,
        mut buf: &[u8],
        m: &mut Message,
        depth: u32,
    ) -> Result<(), WireError> {
        if depth > MAX_DEPTH {
            return Err(WireError::TooDeep);
        }
        let desc = self.desc(name);
        while !buf.is_empty() {
            let record_start = buf;
            let (number, wire) = get_tag(&mut buf)?;
            let field = desc.fields.get(&number).filter(|f| match f.label {
                Label::Map { .. } => wire == LEN,
                Label::Repeated => wire == f.kind.wire() || (wire == LEN && f.kind.packable()),
                _ => wire == f.kind.wire(),
            });
            let Some(field) = field else {
                skip(&mut buf, number, wire, depth)?;
                m.unknown
                    .extend_from_slice(&record_start[..record_start.len() - buf.len()]);
                continue;
            };
            if let Some(group) = &field.oneof {
                // the last member wins: setting one clears the others
                m.fields
                    .retain(|n, _| *n == number || desc.fields[n].oneof.as_ref() != Some(group));
            }
            match &field.label {
                Label::Map { key, value } => {
                    let n = get_varint(&mut buf)?;
                    let mut entry = take(&mut buf, n)?;
                    let (mut k, mut v) = (self.default_of(key), self.default_of(value));
                    while !entry.is_empty() {
                        match get_tag(&mut entry)? {
                            (1, w) if w == key.wire() => {
                                k = self.get_value(&mut entry, key, depth + 1)?
                            }
                            (2, w) if w == value.wire() => {
                                v = self.get_value(&mut entry, value, depth + 1)?
                            }
                            (n, w) => skip(&mut entry, n, w, depth)?,
                        }
                    }
                    let Slot::Map(map) = m
                        .fields
                        .entry(number)
                        .or_insert_with(|| Slot::Map(BTreeMap::new()))
                    else {
                        unreachable!()
                    };
                    map.insert(k, v);
                }
                Label::Repeated => {
                    let packed = wire == LEN && field.kind.packable();
                    let mut values = vec![];
                    if packed {
                        let n = get_varint(&mut buf)?;
                        let mut run = take(&mut buf, n)?;
                        while !run.is_empty() {
                            values.push(self.get_value(&mut run, &field.kind, depth)?);
                        }
                    } else {
                        values.push(self.get_value(&mut buf, &field.kind, depth)?);
                    }
                    if !values.is_empty() {
                        let Slot::Many(list) =
                            m.fields.entry(number).or_insert_with(|| Slot::Many(vec![]))
                        else {
                            unreachable!()
                        };
                        list.extend(values);
                    }
                }
                _ => {
                    if let (Kind::Message(sub), Some(Slot::One(Value::Msg(existing)))) =
                        (&field.kind, m.fields.get_mut(&number))
                    {
                        let n = get_varint(&mut buf)?;
                        let bytes = take(&mut buf, n)?;
                        self.decode_into(sub, bytes, existing, depth + 1)?;
                    } else {
                        let v = self.get_value(&mut buf, &field.kind, depth)?;
                        // implicit presence: an explicit default on the wire still means "default", stored as absent
                        if field.label == Label::Implicit
                            && !matches!(field.kind, Kind::Message(_))
                            && v == self.default_of(&field.kind)
                        {
                            m.fields.remove(&number);
                        } else {
                            m.fields.insert(number, Slot::One(v));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn decode(&self, name: &str, buf: &[u8]) -> Result<Message, WireError> {
        let mut m = Message::default();
        self.decode_into(name, buf, &mut m, 0)?;
        Ok(m)
    }
}

// ===========================================================================
// 4. official vectors
// ===========================================================================
fn hex(s: &str) -> Vec<u8> {
    let s: String = s.split_whitespace().collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
const DOC_PROTO: &str = r#"
syntax = "proto3";
message Test1 { int32 a = 1; }
message Test2 { string b = 2; }
message Test3 { Test1 c = 3; }
message Test4 { string d = 4; repeated int32 e = 5; }
message Test6 { map<string, int32> g = 7; }
"#;
fn official_vectors() {
    let s = parse_proto(DOC_PROTO).unwrap();
    let check = |name: &str, m: Message, bytes: &str| {
        assert_eq!(s.encode(name, &m), hex(bytes), "DOCS {name}");
        assert_eq!(
            s.decode(name, &hex(bytes)).unwrap(),
            m,
            "DOCS {name} decode"
        );
    };
    check(
        "Test1",
        Message::default().with(1, Value::Int(150)),
        "08 96 01",
    );
    check(
        "Test2",
        Message::default().with(2, Value::Str("testing".into())),
        "12 07 74 65 73 74 69 6e 67",
    );
    check(
        "Test3",
        Message::default().with(3, Value::Msg(Message::default().with(1, Value::Int(150)))),
        "1a 03 08 96 01",
    );
    let test4 = Message::default()
        .with(4, Value::Str("hello".into()))
        .many(5, [1, 2, 3].map(Value::Int).to_vec());
    check("Test4", test4.clone(), "220568656c6c6f2a03010203");
    // DOCS: expanded records interleaved with another field, and a packed field split in two, must parse the same
    assert_eq!(
        s.decode("Test4", &hex("2801 2802 220568656c6c6f 2803"))
            .unwrap(),
        test4
    );
    assert_eq!(
        s.decode("Test4", &hex("2a020102 220568656c6c6f 2a0103"))
            .unwrap(),
        test4
    );
    let mut v = vec![];
    put_varint(&mut v, 150);
    assert_eq!(v, hex("9601"));
    v.clear();
    put_varint(&mut v, -2i64 as u64);
    assert_eq!(
        v,
        hex("fe ff ff ff ff ff ff ff ff 01"),
        "DOCS: int -2 takes ten bytes"
    );
    for (signed, encoded) in [
        (0i64, 0u64),
        (-1, 1),
        (1, 2),
        (-2, 3),
        (0x7fff_ffff, 0xffff_fffe),
        (-0x8000_0000, 0xffff_ffff),
    ] {
        assert_eq!(
            (zigzag(signed), unzigzag(encoded)),
            (encoded, signed),
            "DOCS zigzag table"
        );
    }
    // varint edges and malformed input
    let mut r = Rng(1);
    for i in 0..20_000 {
        let x = if i < 64 {
            1u64 << i
        } else {
            r.next() >> r.below(64)
        };
        let mut b = vec![];
        put_varint(&mut b, x);
        assert_eq!(b.len() as u32, (64 - x.max(1).leading_zeros()).div_ceil(7));
        let mut cursor = &b[..];
        assert_eq!(get_varint(&mut cursor), Ok(x));
        assert_eq!(
            get_varint(&mut &b[..b.len() - 1]),
            Err(WireError::Truncated)
        );
        assert_eq!(unzigzag(zigzag(x as i64)), x as i64);
    }
    assert_eq!(
        get_varint(&mut &hex("ffffffffffffffffff02")[..]),
        Err(WireError::VarintOverflow)
    );
    assert_eq!(
        get_varint(&mut &hex("8080808080808080808001")[..]),
        Err(WireError::VarintOverflow)
    );
    assert_eq!(
        s.decode("Test1", &hex("0001")),
        Err(WireError::BadFieldNumber)
    );
    assert_eq!(s.decode("Test1", &hex("0e")), Err(WireError::BadWireType));
    assert_eq!(
        s.decode("Test2", &hex("12ffffffff0f")),
        Err(WireError::Truncated),
        "a 4 GB length is refused before allocating"
    );
    assert_eq!(s.decode("Test2", &hex("1201ff")), Err(WireError::BadUtf8));
    // unknown group (field 8 holding a varint) is skipped and preserved; mismatched end is an error
    let group = hex("43 0802 44");
    assert_eq!(s.decode("Test1", &group).unwrap().unknown, group);
    assert_eq!(
        s.decode("Test1", &hex("43 3c")),
        Err(WireError::UnbalancedGroup)
    );
    assert_eq!(s.decode("Test1", &[0x43; 5000]), Err(WireError::TooDeep));
    println!(
        "  official examples (150, \"testing\", submessage, packed, zigzag table, -2) match byte for byte"
    );
}

// ===========================================================================
// 5. random messages, merge law, fuzzing
// ===========================================================================
const FULL_PROTO: &str = r#"
syntax = "proto3";
/* every kind this codec supports */
enum Color { NONE = 0; RED = 1; }
message Leaf { sint64 id = 1; string tag = 2; }
message All {
  int32 a = 1; int64 b = 2; uint32 c = 3; uint64 d = 4; sint32 e = 5; sint64 f = 6; bool g = 7;
  fixed32 h = 8; sfixed32 i = 9; float j = 10; fixed64 k = 11; sfixed64 l = 12; double m = 13;
  string n = 14; bytes o = 15; Leaf p = 16; Color q = 17;
  optional int32 maybe = 18;
  repeated sint32 rs = 19; repeated string rn = 20; repeated Leaf rm = 21; repeated double rd = 22;
  map<string, int64> by_name = 23; map<uint32, Leaf> by_id = 24;
  oneof choice { int32 as_int = 25; string as_text = 26; Leaf as_leaf = 27; }
  All child = 536870911; // the largest legal field number
}
"#;
fn random_value(s: &Schema, k: &Kind, r: &mut Rng, depth: u32) -> Value {
    use Kind::*;
    let raw = match r.below(5) {
        0 => 0,
        1 => u64::MAX,
        2 => 1u64 << r.below(64),
        _ => r.next() >> r.below(64),
    };
    match k {
        Int32 | Sint32 | Sfixed32 | Enum => Value::Int(i64::from(raw as i32)),
        Int64 | Sint64 | Sfixed64 => Value::Int(raw as i64),
        Uint32 => Value::Uint(u64::from(raw as u32)),
        Uint64 => Value::Uint(raw),
        Bool => Value::Bool(raw & 1 == 1),
        Fixed32 | Float => Value::Bits32(raw as u32),
        Fixed64 | Double => Value::Bits64(raw),
        Str => Value::Str(
            (0..r.below(6))
                .map(|_| ['a', 'é', '€', '"', '\0', '𝄞'][r.below(6) as usize])
                .collect(),
        ),
        Bytes => Value::Bytes((0..r.below(6)).map(|_| r.next() as u8).collect()),
        Message(name) => Value::Msg(random_message(s, name, r, depth + 1)),
    }
}
/// A random message in CANONICAL form (what the decoder produces): no implicit defaults, no empty lists or maps.
fn random_message(s: &Schema, name: &str, r: &mut Rng, depth: u32) -> Message {
    let mut m = Message::default();
    let mut used_oneof = HashSet::new();
    let mut order: Vec<(&u32, &Field)> = s.desc(name).fields.iter().collect();
    let shift = r.below(order.len() as u64) as usize;
    order.rotate_left(shift);
    for (&n, f) in order {
        if r.below(3) != 0 || (depth >= 3 && matches!(f.kind, Kind::Message(_))) {
            continue;
        }
        if f.oneof
            .as_ref()
            .is_some_and(|g| !used_oneof.insert(g.clone()))
        {
            continue;
        }
        let slot = match &f.label {
            Label::Repeated => Slot::Many(
                (0..1 + r.below(4))
                    .map(|_| random_value(s, &f.kind, r, depth))
                    .collect(),
            ),
            Label::Map { key, value } => {
                if depth >= 3 {
                    continue;
                }
                Slot::Map(
                    (0..1 + r.below(3))
                        .map(|_| {
                            (
                                random_value(s, key, r, depth),
                                random_value(s, value, r, depth),
                            )
                        })
                        .collect(),
                )
            }
            label => {
                let v = random_value(s, &f.kind, r, depth);
                if *label == Label::Implicit
                    && !matches!(f.kind, Kind::Message(_))
                    && v == s.default_of(&f.kind)
                {
                    continue;
                }
                Slot::One(v)
            }
        };
        m.fields.insert(n, slot);
    }
    m
}
/// The documented MergeFrom, written independently of the wire decoder, as an oracle.
fn merge(s: &Schema, name: &str, into: &mut Message, from: &Message) {
    let desc = s.desc(name);
    for (&n, slot) in &from.fields {
        let f = &desc.fields[&n];
        if let Some(group) = &f.oneof {
            into.fields
                .retain(|k, _| *k == n || desc.fields[k].oneof.as_ref() != Some(group));
        }
        match (into.fields.get_mut(&n), slot) {
            (Some(Slot::Many(a)), Slot::Many(b)) => a.extend(b.iter().cloned()),
            (Some(Slot::Map(a)), Slot::Map(b)) => a.extend(b.clone()), // a repeated key replaces the whole value
            (Some(Slot::One(Value::Msg(a))), Slot::One(Value::Msg(b))) => {
                let Kind::Message(sub) = &f.kind else {
                    unreachable!()
                };
                merge(s, sub, a, b);
            }
            _ => drop(into.fields.insert(n, slot.clone())),
        }
    }
    into.unknown.extend_from_slice(&from.unknown);
}
fn random_and_fuzz() {
    let s = parse_proto(FULL_PROTO).unwrap();
    let mut r = Rng(2);
    let mut corpus = vec![];
    let mut total = 0;
    for _ in 0..3000 {
        let m = random_message(&s, "All", &mut r, 0);
        let bytes = s.encode("All", &m);
        assert_eq!(s.decode("All", &bytes).unwrap(), m);
        total += bytes.len();
        corpus.push((m, bytes));
    }
    for pair in corpus.windows(2) {
        let concatenated = [pair[0].1.clone(), pair[1].1.clone()].concat();
        let mut want = pair[0].0.clone();
        merge(&s, "All", &mut want, &pair[1].0);
        assert_eq!(
            s.decode("All", &concatenated).unwrap(),
            want,
            "parse(a ++ b) == merge(parse(a), parse(b))"
        );
    }
    // fuzz: any bytes may be rejected, none may panic; whatever is accepted must be stable under re-encoding
    let (mut accepted, mut rejected) = (0, 0);
    for i in 0..60_000 {
        let input: Vec<u8> = if i % 3 == 0 {
            (0..r.below(80)).map(|_| r.next() as u8).collect()
        } else {
            let mut b = corpus[r.below(3000) as usize].1.clone();
            for _ in 0..1 + r.below(4) {
                if b.is_empty() {
                    break;
                }
                let at = r.below(b.len() as u64) as usize;
                match r.below(4) {
                    0 => b[at] ^= 1 << r.below(8),
                    1 => b.truncate(at),
                    2 => b.insert(at, r.next() as u8),
                    _ => drop(b.remove(at)),
                }
            }
            b
        };
        match s.decode("All", &input) {
            Ok(m) => {
                accepted += 1;
                // not necessarily the same bytes (order and defaults are normalised), but a fixed point after one pass
                let once = s.encode("All", &m);
                let again = s.decode("All", &once).expect("our own output must parse");
                assert_eq!(s.encode("All", &again), once);
            }
            Err(_) => rejected += 1,
        }
    }
    assert!(accepted > 1000 && rejected > 1000);
    // a hand-made deeply nested message is refused instead of overflowing the stack
    let mut deep = vec![];
    for _ in 0..300 {
        let mut outer = vec![];
        put_tag(&mut outer, MAX_FIELD as u32, LEN);
        put_varint(&mut outer, deep.len() as u64);
        outer.extend(&deep);
        deep = outer;
    }
    assert_eq!(s.decode("All", &deep), Err(WireError::TooDeep));
    println!(
        "  3000 random messages ({total} bytes) round-tripped; merge law held on 2999 pairs; fuzz: {accepted} accepted, {rejected} rejected, 0 panics"
    );
}

// ===========================================================================
// 6. compatibility
// ===========================================================================
fn compatibility() {
    let v1 =
        parse_proto("message User { int32 id = 1; string name = 2; int64 score = 3; }").unwrap();
    let v2 = parse_proto("message User { string display_name = 2; int64 score = 3; int32 id = 1; repeated string mail = 4; optional bool admin = 5; }").unwrap();
    let new = Message::default()
        .with(1, Value::Int(7))
        .with(2, Value::Str("ada".into()))
        .with(3, Value::Int(5_000_000_000))
        .many(4, vec![Value::Str("a@x".into()), Value::Str("b@x".into())])
        .with(5, Value::Bool(false));
    let wire = v2.encode("User", &new);
    // old reader, new data: renamed and reordered fields are invisible; new fields become unknown and SURVIVE a rewrite
    let mut old_view = v1.decode("User", &wire).unwrap();
    assert_eq!(old_view.get(2), Some(&Value::Str("ada".into())));
    assert!(!old_view.unknown.is_empty());
    old_view
        .fields
        .insert(2, Slot::One(Value::Str("ada l.".into()))); // an old service edits the record
    let back = v2.decode("User", &v1.encode("User", &old_view)).unwrap();
    assert_eq!(
        back,
        new.clone().with(2, Value::Str("ada l.".into())),
        "an old intermediary loses nothing"
    );
    assert_eq!(
        back.get(5),
        Some(&Value::Bool(false)),
        "explicit presence: false was SENT and is distinguishable from unset"
    );
    // new reader, old data: missing fields are simply absent
    let from_old = v2
        .decode(
            "User",
            &v1.encode("User", &Message::default().with(1, Value::Int(1))),
        )
        .unwrap();
    assert_eq!((from_old.fields.len(), from_old.get(5)), (1, None));
    assert!(
        v1.encode("User", &Message::default().with(1, Value::Int(0)))
            .is_empty(),
        "implicit presence: zero is not sent"
    );

    // type changes: write with one declaration, read with another
    let one = |ty: &str| parse_proto(&format!("message M {{ {ty} x = 1; }}")).unwrap();
    let reread = |from: &str, v: Value, to: &str| {
        let bytes = one(from).encode("M", &Message::default().with(1, v));
        one(to).decode("M", &bytes).unwrap()
    };
    let read = |from: &str, v: Value, to: &str| reread(from, v, to).get(1).cloned();
    assert_eq!(read("int32", Value::Int(-5), "int64"), Some(Value::Int(-5)));
    assert_eq!(
        read("int64", Value::Int((1 << 32) + 7), "int32"),
        Some(Value::Int(7)),
        "compatible but TRUNCATED"
    );
    assert_eq!(
        read("int32", Value::Int(-1), "uint32"),
        Some(Value::Uint(u64::from(u32::MAX)))
    );
    assert_eq!(
        read("uint32", Value::Uint(2), "bool"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        read("sint32", Value::Int(-1), "int32"),
        Some(Value::Int(1)),
        "INCOMPATIBLE: parses fine, wrong number"
    );
    assert_eq!(
        read("int32", Value::Int(150), "sint32"),
        Some(Value::Int(75))
    );
    assert_eq!(
        read("string", Value::Str("é".into()), "bytes"),
        Some(Value::Bytes("é".as_bytes().to_vec()))
    );
    assert_eq!(
        read("fixed32", Value::Bits32(7), "sfixed32"),
        Some(Value::Int(7))
    );
    let mismatch = reread("fixed32", Value::Bits32(7), "int32");
    assert!(
        mismatch.get(1).is_none() && !mismatch.unknown.is_empty(),
        "a wire type change reads as the default"
    );
    // singular -> repeated is readable both ways for strings; for a repeated scalar read as singular, expanded: last wins
    let rep = one("repeated string");
    let two = rep.encode(
        "M",
        &Message::default().many(1, vec![Value::Str("a".into()), Value::Str("b".into())]),
    );
    assert_eq!(
        one("string").decode("M", &two).unwrap().get(1),
        Some(&Value::Str("b".into()))
    );
    let packed = one("repeated int32").encode(
        "M",
        &Message::default().many(1, vec![Value::Int(1), Value::Int(2)]),
    );
    assert!(
        one("int32").decode("M", &packed).unwrap().get(1).is_none(),
        "PACKED data does not fit a singular int field"
    );

    for (bad, why) in [
        (
            "message M { reserved 2, 9 to 11; int32 x = 10; }",
            "reserved",
        ),
        ("message M { reserved \"old\"; int32 old = 1; }", "reserved"),
        (
            "message M { int32 a = 1; int32 b = 1; }",
            "duplicate field number",
        ),
        ("message M { int32 a = 0; }", "illegal field number"),
        ("message M { int32 a = 19500; }", "illegal field number"),
        ("message M { map<double, int32> m = 1; }", "illegal map key"),
        ("message M { Missing a = 1; }", "unknown type"),
        ("enum E { A = 1; }", "first proto3 enum"),
        ("message M { int32 a = 1 }", "expected"),
        ("message M { /* ", "unterminated"),
    ] {
        let err = parse_proto(bad).unwrap_err();
        assert!(err.contains(why), "{bad}: {err}");
    }
    println!(
        "  compatibility: rename/reorder/add safe, unknown fields survive; int32->sint32 silently wrong; reserved numbers enforced"
    );
}

// ===========================================================================
// 7. gRPC framing
// ===========================================================================
#[derive(Debug, PartialEq, Clone, Copy)]
enum Code {
    Ok = 0,
    Cancelled = 1,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    ResourceExhausted = 8,
    Internal = 13,
}
fn frame_message(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0]; // not compressed
    out.extend((payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}
/// Reassembles messages from arbitrary chunks.
#[derive(Default)]
struct Deframer {
    buffer: Vec<u8>,
}
impl Deframer {
    const MAX_MESSAGE: usize = 1 << 16;
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, Code> {
        self.buffer.extend_from_slice(chunk);
        let mut out = vec![];
        while self.buffer.len() >= 5 {
            if self.buffer[0] != 0 {
                return Err(Code::Internal); // compressed with no negotiated grpc-encoding, or a corrupt flag
            }
            let n = u32::from_be_bytes(self.buffer[1..5].try_into().unwrap()) as usize;
            if n > Self::MAX_MESSAGE {
                return Err(Code::ResourceExhausted); // refused from the 5-byte header, before buffering the body
            }
            if self.buffer.len() < 5 + n {
                break;
            }
            out.push(self.buffer[5..5 + n].to_vec());
            self.buffer.drain(..5 + n);
        }
        Ok(out)
    }
    /// A stream that ends in the middle of a message is broken.
    fn finish(&self) -> Result<(), Code> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(Code::Internal)
        }
    }
}
const UNITS: [(char, u64); 6] = [
    ('n', 1),
    ('u', 1_000),
    ('m', 1_000_000),
    ('S', 1_000_000_000),
    ('M', 60_000_000_000),
    ('H', 3_600_000_000_000),
];
/// At most 8 digits: use the finest unit that fits, rounding UP so the deadline is never shortened to zero.
fn encode_timeout(nanos: u64) -> String {
    for (unit, size) in UNITS {
        let v = nanos.div_ceil(size);
        if v < 100_000_000 {
            return format!("{v}{unit}");
        }
    }
    "99999999H".to_string()
}
fn parse_timeout(s: &str) -> Option<u64> {
    let unit = s.chars().last()?;
    let digits = &s[..s.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let size = UNITS.iter().find(|u| u.0 == unit)?.1;
    Some(digits.parse::<u64>().ok()?.saturating_mul(size))
}
/// `grpc-message`: printable ASCII except '%' goes as is, every other byte as %XX.
fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if (0x20..=0x7e).contains(&b) && b != b'%' {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (vec![], 0);
    while i < b.len() {
        let escaped = (b[i] == b'%')
            .then(|| b.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        out.push(escaped.unwrap_or(b[i])); // invalid escapes pass through rather than losing the error text
        i += if escaped.is_some() { 3 } else { 1 };
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn framing_test() {
    assert_eq!(
        frame_message(&[0x08, 0x96, 0x01]),
        [0, 0, 0, 0, 3, 0x08, 0x96, 0x01]
    );
    let mut r = Rng(3);
    for _ in 0..2000 {
        let messages: Vec<Vec<u8>> = (0..r.below(6))
            .map(|_| (0..r.below(300)).map(|_| r.next() as u8).collect())
            .collect();
        let stream: Vec<u8> = messages.iter().flat_map(|m| frame_message(m)).collect();
        let (mut d, mut got, mut at) = (Deframer::default(), vec![], 0);
        while at < stream.len() {
            let largest = if r.below(2) == 0 { 4 } else { 700 }; // mostly tiny splits, sometimes several messages at once
            let n = (1 + r.below(largest) as usize).min(stream.len() - at);
            got.extend(d.push(&stream[at..at + n]).unwrap());
            at += n;
        }
        assert_eq!(got, messages, "DATA frame boundaries are irrelevant");
        assert_eq!(d.finish(), Ok(()));
    }
    let mut d = Deframer::default();
    assert_eq!(d.push(&[0, 0xff, 0, 0, 0]), Err(Code::ResourceExhausted));
    assert_eq!(
        Deframer::default().push(&[1, 0, 0, 0, 0]),
        Err(Code::Internal)
    );
    let mut d = Deframer::default();
    assert!(d.push(&frame_message(b"abc")[..6]).unwrap().is_empty());
    assert_eq!(d.finish(), Err(Code::Internal));

    assert_eq!(parse_timeout("1S"), Some(1_000_000_000));
    assert_eq!(encode_timeout(250_000_000), "250000u");
    for bad in ["", "S", "123456789S", "-1S", "1s", "1.5S", "1é"] {
        assert_eq!(parse_timeout(bad), None, "{bad:?}");
    }
    for _ in 0..5000 {
        let ns = r.next() >> r.below(60).max(10);
        let text = encode_timeout(ns);
        let back = parse_timeout(&text).unwrap();
        assert!(text.len() <= 9 && back >= ns, "{ns} -> {text}");
        // a coarser unit is used only when the finer one needs more than 8 digits, so one unit is at most 1000 x ns / 1e8
        assert!(
            back - ns <= ns / 100_000,
            "rounding error below one unit: {ns} -> {text}"
        );
        let message: String = (0..r.below(12))
            .map(|_| ['a', ' ', '%', '\n', 'é', '€', '4', '1'][r.below(8) as usize])
            .collect();
        let encoded = percent_encode(&message);
        assert!(encoded.bytes().all(|b| (0x20..=0x7e).contains(&b)));
        assert_eq!(percent_decode(&encoded), message);
    }
    println!(
        "  2000 random chunkings reassembled exactly; grpc-timeout and grpc-message encodings round-trip"
    );
}

// ===========================================================================
// 8. streaming with flow control, deadlines and cancellation
// ===========================================================================
#[derive(Debug, Clone, PartialEq)]
enum H2 {
    Data(Vec<u8>),
    EndStream,             // client half-close
    Trailers(u32, String), // grpc-status and percent-encoded grpc-message; ends the response
    RstStreamCancel,
}
const WINDOW: usize = 64;
const MAX_FRAME: usize = 24;
/// One direction of a stream. `credit` is what the SENDER may still put on the wire.
struct Half {
    wire: VecDeque<H2>,
    credit: usize,
    outbox: VecDeque<u8>,   // framed bytes waiting for credit
    last_frame: Option<H2>, // sent once the outbox has drained
    deframer: Deframer,
    unread: VecDeque<Vec<u8>>, // complete messages the application has not consumed
    unacked: usize,            // bytes received and not yet returned as window
    updates: VecDeque<usize>,  // WINDOW_UPDATE frames travelling back to the sender
}
impl Half {
    fn new() -> Self {
        Half {
            wire: VecDeque::new(),
            credit: WINDOW,
            outbox: VecDeque::new(),
            last_frame: None,
            deframer: Deframer::default(),
            unread: VecDeque::new(),
            unacked: 0,
            updates: VecDeque::new(),
        }
    }
    fn send(&mut self, message: &[u8]) {
        self.outbox.extend(frame_message(message));
    }
    fn flush(&mut self) {
        while self.credit > 0 && !self.outbox.is_empty() {
            let n = self.outbox.len().min(self.credit).min(MAX_FRAME);
            self.credit -= n;
            self.wire
                .push_back(H2::Data(self.outbox.drain(..n).collect()));
        }
        if self.outbox.is_empty() {
            if let Some(frame) = self.last_frame.take() {
                self.wire.push_back(frame); // HEADERS/trailers are not flow controlled
            }
        }
    }
    fn receive_data(&mut self, bytes: &[u8]) {
        self.unacked += bytes.len();
        let messages = self
            .deframer
            .push(bytes)
            .expect("our peer frames correctly");
        self.unread.extend(messages);
    }
    /// Return window only while the application keeps up: that is the backpressure. (A PARTIAL message is acknowledged
    /// at once, otherwise a message larger than the window could never complete: deadlock.)
    fn replenish(&mut self) {
        if self.unread.is_empty() && self.unacked > 0 {
            self.updates.push_back(std::mem::take(&mut self.unacked));
        }
    }
    /// Conservation law: every byte of window is exactly one of: unspent, on the wire, held by the receiver, or on its way back.
    fn check_window(&self) {
        let in_flight: usize = self
            .wire
            .iter()
            .map(|f| if let H2::Data(d) = f { d.len() } else { 0 })
            .sum();
        assert_eq!(
            self.credit + in_flight + self.unacked + self.updates.iter().sum::<usize>(),
            WINDOW
        );
    }
}
#[derive(Debug, PartialEq, Clone, Copy)]
enum Mode {
    Unary,
    ServerStreaming,
    ClientStreaming,
    Bidi,
}
#[derive(Debug)]
struct CallResult {
    status: u32,
    message: String,
    responses: Vec<Vec<u8>>,
    server_handled: usize,
    server_saw_cancel: bool,
    max_buffered: usize,
    ticks: u64,
}
/// The service upper-cases bytes; a request starting with '!' is rejected with a non-ASCII error text.
/// `timeout` is the `grpc-timeout` header value; one tick stands for one millisecond.
fn call(
    mode: Mode,
    requests: &[Vec<u8>],
    client_reads_every: u64,
    timeout: &str,
    cancel_at: Option<u64>,
    seed: u64,
) -> CallResult {
    let (mut up, mut down) = (Half::new(), Half::new());
    let mut r = Rng(seed);
    requests.iter().for_each(|m| up.send(m));
    up.last_frame = Some(H2::EndStream);
    let deadline = parse_timeout(timeout).expect("valid grpc-timeout") / 1_000_000;
    let mut out = CallResult {
        status: Code::Ok as u32,
        message: String::new(),
        responses: vec![],
        server_handled: 0,
        server_saw_cancel: false,
        max_buffered: 0,
        ticks: 0,
    };
    let (mut collected, mut server_saw_end, mut server_done, mut finished, mut abandoned) =
        (vec![], false, false, false, None);
    for tick in 0u64.. {
        out.ticks = tick;
        if abandoned.is_none() && (tick >= deadline || cancel_at == Some(tick)) {
            // the client gives up: both cases are an RST_STREAM on the wire, with different local status
            abandoned = Some(if tick >= deadline {
                Code::DeadlineExceeded
            } else {
                Code::Cancelled
            });
            up.outbox.clear();
            up.last_frame = None;
            up.wire.push_back(H2::RstStreamCancel);
        }
        up.flush();
        down.flush();
        // the network delivers a random number of frames in each direction, in order
        for _ in 0..r.below(3) {
            match up.wire.pop_front() {
                None => break,
                Some(H2::Data(bytes)) => up.receive_data(&bytes),
                Some(H2::EndStream) => {
                    up.deframer.finish().expect("no truncated request");
                    server_saw_end = true;
                }
                Some(H2::RstStreamCancel) => {
                    out.server_saw_cancel = true;
                    server_done = true;
                }
                Some(other) => panic!("unexpected upstream frame {other:?}"),
            }
        }
        if out.server_saw_cancel {
            out.status = abandoned.expect("only the client cancels") as u32;
            return out;
        }
        // server application: handle one request only when its own output is not backed up
        if !server_done && down.outbox.is_empty() {
            if let Some(m) = up.unread.pop_front() {
                out.server_handled += 1;
                let reply = m.to_ascii_uppercase();
                if m.first() == Some(&b'!') {
                    down.last_frame = Some(H2::Trailers(
                        Code::InvalidArgument as u32,
                        percent_encode("bad input: ‘!’ 100%"),
                    ));
                    server_done = true;
                } else {
                    match mode {
                        Mode::Unary | Mode::Bidi => down.send(&reply),
                        Mode::ServerStreaming => reply.chunks(8).for_each(|c| down.send(c)),
                        Mode::ClientStreaming => collected.extend(reply),
                    }
                }
            } else if server_saw_end {
                if mode == Mode::ClientStreaming {
                    down.send(&collected);
                }
                down.last_frame = Some(H2::Trailers(Code::Ok as u32, String::new()));
                server_done = true;
            }
        }
        for _ in 0..r.below(3) {
            match down.wire.pop_front() {
                None => break,
                Some(H2::Data(bytes)) => down.receive_data(&bytes),
                Some(H2::Trailers(code, text)) => {
                    down.deframer.finish().expect("no truncated response");
                    out.status = code;
                    out.message = percent_decode(&text);
                    finished = true;
                }
                Some(other) => panic!("unexpected downstream frame {other:?}"),
            }
        }
        let buffered =
            down.unread.iter().map(|m| m.len() + 5).sum::<usize>() + down.deframer.buffer.len();
        out.max_buffered = out.max_buffered.max(buffered);
        // client application, possibly slow
        if abandoned.is_none() && tick % client_reads_every == 0 {
            out.responses.extend(down.unread.pop_front());
        }
        for half in [&mut up, &mut down] {
            half.replenish();
            if r.below(2) == 0 {
                half.credit += half.updates.pop_front().unwrap_or(0);
            }
            half.check_window();
        }
        if finished && down.unread.is_empty() && abandoned.is_none() {
            return out;
        }
        assert!(
            tick < 1_000_000,
            "the stream stalled: flow-control deadlock"
        );
    }
    unreachable!()
}
fn streaming_test() {
    let mut r = Rng(4);
    for run in 0..600 {
        let mode = [
            Mode::Unary,
            Mode::ServerStreaming,
            Mode::ClientStreaming,
            Mode::Bidi,
        ][run % 4];
        let count = if matches!(mode, Mode::Unary | Mode::ServerStreaming) {
            1
        } else {
            r.below(8)
        };
        let requests: Vec<Vec<u8>> = (0..count)
            .map(|_| {
                (0..r.below(200))
                    .map(|_| b'a' + r.below(26) as u8)
                    .collect()
            })
            .collect();
        let result = call(mode, &requests, 1 + r.below(10), "1H", None, run as u64);
        assert_eq!(result.status, Code::Ok as u32, "{mode:?} run {run}");
        let upper: Vec<Vec<u8>> = requests.iter().map(|m| m.to_ascii_uppercase()).collect();
        match mode {
            Mode::Unary | Mode::Bidi => assert_eq!(result.responses, upper),
            Mode::ClientStreaming => assert_eq!(result.responses, [upper.concat()]),
            Mode::ServerStreaming => assert_eq!(result.responses.concat(), upper.concat()),
        }
    }
    let failed = call(
        Mode::Bidi,
        &[b"ok".to_vec(), b"!no".to_vec(), b"never".to_vec()],
        1,
        "1H",
        None,
        9,
    );
    assert_eq!(
        (failed.status, failed.message.as_str()),
        (Code::InvalidArgument as u32, "bad input: ‘!’ 100%")
    );
    assert_eq!(
        (failed.responses, failed.server_handled),
        (vec![b"OK".to_vec()], 2),
        "earlier responses arrive, later requests are not handled"
    );

    let big: Vec<Vec<u8>> = vec![vec![b'x'; 40]; 100];
    let fast = call(Mode::Bidi, &big, 1, "1H", None, 1);
    let slow = call(Mode::Bidi, &big, 50, "1H", None, 1);
    assert!(slow.status == 0 && slow.ticks > 3 * fast.ticks);
    assert!(
        slow.max_buffered <= 2 * WINDOW + 45,
        "a slow reader bounds memory: {} bytes of 4500",
        slow.max_buffered
    );
    let timed_out = call(Mode::Bidi, &big, 50, "500m", None, 1);
    assert_eq!(timed_out.status, Code::DeadlineExceeded as u32);
    assert!(
        timed_out.server_saw_cancel
            && timed_out.server_handled < 100
            && timed_out.responses.len() < 100
    );
    let cancelled = call(Mode::Bidi, &big, 1, "1H", Some(100), 1);
    assert_eq!(cancelled.status, Code::Cancelled as u32);
    assert!(
        cancelled.server_saw_cancel && cancelled.server_handled < 100 && cancelled.ticks >= 100
    );
    println!(
        "  600 calls across 4 streaming modes correct; slow reader held at most {} bytes of 4500; trailers, deadline and cancel observed",
        slow.max_buffered
    );
}

fn main() {
    println!("== 1. official encoding examples ==");
    official_vectors();
    println!("== 2. random messages, merge law, fuzzing ==");
    random_and_fuzz();
    println!("== 3. schema evolution ==");
    compatibility();
    println!("== 4. gRPC framing ==");
    framing_test();
    println!("== 5. streaming and flow control ==");
    streaming_test();
    println!("all protobuf and gRPC checks passed");
}
