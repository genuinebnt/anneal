//! RECIPE 451 (encoding): serde beyond derive: hand-written impls, attributes, borrowing, seeds, and a custom data format
//! LEVEL: advanced
//! Use in: wire formats, config schemas, protocol codecs, zero-copy parsers.   Run: cargo run --release --example serde_custom
#![allow(dead_code)]
//!
//! WHAT: five layers of serde control, each asserted: (1) hand-written `Serialize`/`Deserialize` with a `Visitor` for types
//! whose wire form differs from their fields (`"1h30m"`, `"#rrggbb"` or a 3-tuple, a flag set as a list of names);
//! (2) the attribute toolbox (`with`, `serialize_with`, `flatten`, `untagged`, `tag/content`, `deny_unknown_fields`,
//! `default`, `skip_serializing_if`, `rename_all`, `alias`) including the error messages each one produces;
//! (3) BORROWED deserialisation (`&str`, `Cow<str>` + `#[serde(borrow)]`) checked by pointer-range;
//! (4) `DeserializeSeed` for context-dependent parsing (string interning, shape validation);
//! (5) a COMPLETE custom data format: a `Serializer` and `Deserializer` for a compact self-describing binary format
//! (varint ints, length-prefixed strings, tagged enums) that works with derived impls on nested types.
//! WHY: serde separates the data MODEL (29 types) from the data FORMAT; once you implement either side by hand you see
//! the whole contract. HOW: the format is ~300 lines; the demos round-trip 2000 random values and fuzz malformed input.
//!
//! COMMON USES: human-friendly config values (durations, colours, sizes), versioned protocols with tagged enums,
//! zero-copy log/record parsing, interning repeated strings while decoding, internal RPC/storage codecs
//! (bincode, postcard, rmp-serde are exactly this shape), strict validation that reports the offending byte.
//!
//! Read: serde.rs: Custom serialization https://serde.rs/custom-serialization.html ; Deserializer lifetimes https://serde.rs/lifetimes.html
//! Read: Implementing a Serializer https://serde.rs/impl-serializer.html ; Implementing a Deserializer https://serde.rs/impl-deserializer.html
//! Read: Container/field attributes https://serde.rs/attributes.html ; Enum representations https://serde.rs/enum-representations.html
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::fmt::{self, Display};
use std::rc::Rc;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// =================================================================================================================
// 1. Hand-written Serialize / Deserialize
// =================================================================================================================

/// Duration in whole seconds, written "1h30m5s". Fields are one integer; the wire form is text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HumanDuration(u64);

impl Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (h, m, s) = (self.0 / 3600, self.0 / 60 % 60, self.0 % 60);
        if self.0 == 0 {
            return f.write_str("0s");
        }
        if h > 0 {
            write!(f, "{h}h")?;
        }
        if m > 0 {
            write!(f, "{m}m")?;
        }
        if s > 0 {
            write!(f, "{s}s")?;
        }
        Ok(())
    }
}
fn parse_duration(s: &str) -> Option<u64> {
    let (mut total, mut num, mut last_rank) = (0u64, None::<u64>, 0u8);
    for c in s.chars() {
        if let Some(d) = c.to_digit(10) {
            num = Some(num.unwrap_or(0).checked_mul(10)?.checked_add(d as u64)?);
        } else {
            let (rank, mult) = match c {
                'h' => (1, 3600),
                'm' => (2, 60),
                's' => (3, 1),
                _ => return None,
            };
            if rank <= last_rank {
                return None; // units must appear once, largest first
            }
            last_rank = rank;
            total = total.checked_add(num.take()?.checked_mul(mult)?)?;
        }
    }
    (num.is_none() && last_rank > 0).then_some(total)
}

impl Serialize for HumanDuration {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // is_human_readable lets one type serve JSON (text) and compact binary formats (raw integer)
        if s.is_human_readable() {
            s.collect_str(self)
        } else {
            s.serialize_u64(self.0)
        }
    }
}
impl<'de> Deserialize<'de> for HumanDuration {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = HumanDuration;
            // `expecting` completes the sentence "invalid type: X, expected ..." in every error message
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a duration like \"1h30m\" or a number of seconds")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<HumanDuration, E> {
                parse_duration(v)
                    .map(HumanDuration)
                    .ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &self))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<HumanDuration, E> {
                Ok(HumanDuration(v))
            }
        }
        // deserialize_any: JSON can tell string from number; a non-self-describing format would have to pick one
        d.deserialize_any(V)
    }
}

/// Colour accepted as "#rrggbb", `[r,g,b]` or `{"r":..,"g":..,"b":..}`; written as hex text (human) or a 3-tuple (binary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Color(u8, u8, u8);
impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.collect_str(&format_args!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2))
        } else {
            let mut t = s.serialize_tuple(3)?;
            t.serialize_element(&self.0)?;
            t.serialize_element(&self.1)?;
            t.serialize_element(&self.2)?;
            t.end()
        }
    }
}
impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Color;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a colour as \"#rrggbb\", [r, g, b] or {r, g, b}")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Color, E> {
                let h = v
                    .strip_prefix('#')
                    .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()));
                let h = h.ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &self))?;
                let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
                Ok(Color(p(0), p(2), p(4)))
            }
            // Called by formats that can lend a slice of their input. Delegating keeps one parser; a type that
            // wants zero-copy overrides this one (and `visit_str` copies).
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Color, E> {
                self.visit_str(v)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Color, A::Error> {
                let mut next = |i| {
                    a.next_element()?
                        .ok_or_else(|| de::Error::invalid_length(i, &"3 components"))
                };
                Ok(Color(next(0)?, next(1)?, next(2)?))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Color, A::Error> {
                let (mut r, mut g, mut b) = (None, None, None);
                while let Some(k) = a.next_key::<String>()? {
                    let slot = match k.as_str() {
                        "r" => &mut r,
                        "g" => &mut g,
                        "b" => &mut b,
                        other => return Err(de::Error::unknown_field(other, &["r", "g", "b"])),
                    };
                    if slot.replace(a.next_value::<u8>()?).is_some() {
                        return Err(de::Error::duplicate_field("r/g/b")); // serde's own derive reports duplicates too
                    }
                }
                Ok(Color(
                    r.ok_or_else(|| de::Error::missing_field("r"))?,
                    g.ok_or_else(|| de::Error::missing_field("g"))?,
                    b.ok_or_else(|| de::Error::missing_field("b"))?,
                ))
            }
        }
        if d.is_human_readable() {
            d.deserialize_any(V)
        } else {
            d.deserialize_tuple(3, V)
        }
    }
}

/// Bitflags-like permission set written as `["read","exec"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Perms(u8);
const PERM_NAMES: [&str; 3] = ["read", "write", "exec"];
impl Serialize for Perms {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // collect first: a `filter` iterator has no exact length, and length-prefixed formats (like ours) need one
        let names: Vec<_> = PERM_NAMES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.0 >> i & 1 == 1)
            .map(|(_, n)| n)
            .collect();
        s.collect_seq(names)
    }
}
impl<'de> Deserialize<'de> for Perms {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Perms;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of permission names")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Perms, A::Error> {
                let mut bits = 0u8;
                while let Some(name) = a.next_element::<Cow<str>>()? {
                    let i = PERM_NAMES
                        .iter()
                        .position(|n| *n == name)
                        .ok_or_else(|| de::Error::unknown_variant(&name, &PERM_NAMES))?;
                    bits |= 1 << i;
                }
                Ok(Perms(bits))
            }
        }
        d.deserialize_seq(V)
    }
}

fn custom_impls_demo() {
    let mut rng = Rng(11);
    for _ in 0..2000 {
        let d = HumanDuration(rng.below(10_000_000));
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<HumanDuration>(&json).unwrap(), d);
        let c = Color(rng.next() as u8, rng.next() as u8, rng.next() as u8);
        assert_eq!(
            serde_json::from_str::<Color>(&serde_json::to_string(&c).unwrap()).unwrap(),
            c
        );
        let p = Perms(rng.below(8) as u8);
        assert_eq!(
            serde_json::from_str::<Perms>(&serde_json::to_string(&p).unwrap()).unwrap(),
            p
        );
    }
    assert_eq!(
        serde_json::to_string(&HumanDuration(5400)).unwrap(),
        "\"1h30m\""
    );
    assert_eq!(
        serde_json::from_str::<HumanDuration>("90").unwrap(),
        HumanDuration(90)
    );
    assert_eq!(
        serde_json::to_string(&Perms(0b101)).unwrap(),
        r#"["read","exec"]"#
    );
    for (input, c) in [
        (r##""#ff8000""##, Color(255, 128, 0)),
        ("[1,2,3]", Color(1, 2, 3)),
        (r#"{"b":3,"r":1,"g":2}"#, Color(1, 2, 3)),
    ] {
        assert_eq!(serde_json::from_str::<Color>(input).unwrap(), c);
    }
    // `expecting` shows up in messages; the Unexpected value says what was actually found
    let e = serde_json::from_str::<HumanDuration>("\"5x\"")
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("invalid value: string \"5x\", expected a duration like"),
        "{e}"
    );
    let e = serde_json::from_str::<Color>("true")
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("invalid type: boolean `true`, expected a colour as"),
        "{e}"
    );
    assert!(
        serde_json::from_str::<Color>("[1,2]")
            .unwrap_err()
            .to_string()
            .contains("invalid length 2, expected 3 components")
    );
    assert!(
        serde_json::from_str::<Color>(r#"{"r":1,"x":2}"#)
            .unwrap_err()
            .to_string()
            .contains("unknown field `x`, expected one of `r`, `g`, `b`")
    );
    assert!(
        serde_json::from_str::<Perms>(r#"["fly"]"#)
            .unwrap_err()
            .to_string()
            .contains("unknown variant `fly`")
    );
    for bad in ["", "h", "1", "1m1h", "1h1h", "99999999999999999999s", "1 h"] {
        assert_eq!(parse_duration(bad), None, "{bad:?}");
    }
    println!(
        "1. hand-written impls: 2000 random round trips; 5400s -> {}; error: {e}",
        serde_json::to_string(&HumanDuration(5400)).unwrap()
    );
}

// =================================================================================================================
// 2. The attribute toolbox (serde_json is the format here)
// =================================================================================================================

/// `#[serde(with = "hex_bytes")]` names a module holding `serialize` and `deserialize` functions for ONE field.
mod hex_bytes {
    use super::*;
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() % 2 != 0 || !s.is_ascii() {
            return Err(de::Error::custom(format!(
                "hex string of odd length or non-ASCII: {s:?}"
            )));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(de::Error::custom))
            .collect()
    }
}
// serialize_with / deserialize_with are the same idea for a single direction, as free functions.
fn upper<S: Serializer>(v: &String, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_uppercase())
}
fn trimmed<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(String::deserialize(d)?.trim().to_owned())
}
fn default_port() -> u16 {
    8080
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)] // typo "hostname" is an error, not silently ignored
struct Server {
    host_name: String, // wire name: hostName
    #[serde(default = "default_port")]
    port: u16,
    #[serde(alias = "timeout", alias = "ttl")] // accept old spellings, always write requestTimeout
    request_timeout: HumanDuration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // absent in output when None, absent in input is fine
    tls_cert: Option<String>,
    #[serde(default, with = "hex_bytes")]
    secret: Vec<u8>,
    #[serde(serialize_with = "upper", deserialize_with = "trimmed")]
    region: String,
    #[serde(default)]
    perms: Perms,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Page {
    #[serde(default = "one")]
    page: u32,
    per_page: u32,
}
fn one() -> u32 {
    1
}
/// flatten merges a struct's fields (or a catch-all map) into the parent object. Costs: serde buffers the whole object
/// first, and `deny_unknown_fields` cannot be combined with flatten.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Query {
    q: String,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

/// Untagged: tries each variant in order. Convenient, but every failure collapses into one vague message.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Id {
    Num(u64),
    Text(String),
}
/// Adjacent tagging {"t": name, "c": payload}: works with any variant shape, friendly to streaming parsers.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
enum Event {
    Click { x: i32, y: i32 },
    Key(char),
    Quit,
}
/// Internal tagging {"kind": name, ...fields}: nicest JSON, but only struct-like/unit variants and it buffers.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Cmd {
    Run { prog: String },
    Stop,
}

fn attributes_demo() {
    let json = r#"{"hostName":"a.example","timeout":"1h30m","secret":"00ff10","region":"  eu-west ","perms":["read","write"]}"#;
    let s: Server = serde_json::from_str(json).unwrap();
    assert_eq!(s.port, 8080, "default fn");
    assert_eq!(s.request_timeout, HumanDuration(5400), "alias");
    assert_eq!(s.secret, [0, 255, 16], "with = module");
    assert_eq!(s.region, "eu-west", "deserialize_with trims");
    let out = serde_json::to_string(&s).unwrap();
    assert!(
        out.contains(r#""requestTimeout":"1h30m""#) && out.contains(r#""region":"EU-WEST""#),
        "{out}"
    );
    assert!(!out.contains("tlsCert"), "skip_serializing_if drops None");
    let e = serde_json::from_str::<Server>(&json.replace("hostName", "hostname"))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("unknown field `hostname`, expected one of `hostName`"),
        "{e}"
    );
    let e = serde_json::from_str::<Server>(
        r#"{"hostName":"x","secret":"abc","requestTimeout":1,"region":""}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("odd length"), "{e}");
    // errors inside nested hand-written code still carry the JSON line/column
    assert!(e.contains("line 1 column"), "{e}");

    let q: Query = serde_json::from_str(r#"{"q":"cats","per_page":10,"utm":"a","n":5}"#).unwrap();
    assert_eq!(
        q.page,
        Page {
            page: 1,
            per_page: 10
        }
    );
    assert_eq!(q.extra.len(), 2);
    assert_eq!(serde_json::to_value(&q).unwrap()["utm"], "a");

    assert_eq!(serde_json::from_str::<Id>("7").unwrap(), Id::Num(7));
    assert_eq!(
        serde_json::from_str::<Id>("\"x\"").unwrap(),
        Id::Text("x".into())
    );
    let e = serde_json::from_str::<Id>("[1]").unwrap_err().to_string();
    assert!(
        e.contains("data did not match any variant of untagged enum Id"),
        "{e}"
    ); // no hint WHICH variant or why
    let e = serde_json::from_str::<Id>("-1").unwrap_err().to_string();
    assert!(
        e.contains("did not match any variant"),
        "-1 is neither u64 nor String: {e}"
    );

    let ev: Event = serde_json::from_str(r#"{"t":"Click","c":{"x":1,"y":2}}"#).unwrap();
    assert_eq!(ev, Event::Click { x: 1, y: 2 });
    assert_eq!(
        serde_json::to_string(&Event::Key('q')).unwrap(),
        r#"{"t":"Key","c":"q"}"#
    );
    assert_eq!(
        serde_json::from_str::<Event>(r#"{"t":"Quit"}"#).unwrap(),
        Event::Quit
    );
    let e = serde_json::from_str::<Event>(r#"{"t":"Zoom","c":1}"#)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("unknown variant `Zoom`, expected one of `Click`, `Key`, `Quit`"),
        "{e}"
    );
    let e = serde_json::from_str::<Event>(r#"{"t":"Click"}"#)
        .unwrap_err()
        .to_string();
    assert!(e.contains("missing field `c`"), "{e}");

    assert_eq!(
        serde_json::from_str::<Cmd>(r#"{"kind":"run","prog":"ls"}"#).unwrap(),
        Cmd::Run { prog: "ls".into() }
    );
    assert!(
        serde_json::from_str::<Cmd>(r#"{"prog":"ls"}"#)
            .unwrap_err()
            .to_string()
            .contains("missing field `kind`")
    );
    // internal tagging reports the inner type error well, unlike untagged:
    let e = serde_json::from_str::<Cmd>(r#"{"kind":"run","prog":5}"#)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("invalid type: integer `5`, expected a string"),
        "{e}"
    );
    println!(
        "2. attributes: defaults, alias, with, flatten, tagged enums verified; untagged error: {:?}",
        "data did not match any variant of untagged enum Id"
    );
}

// =================================================================================================================
// 3. Borrowed (zero-copy) deserialisation
// =================================================================================================================

#[derive(Debug, Deserialize)]
struct Msg<'a> {
    from: &'a str, // a plain `&str` field borrows automatically; fails if the input needs unescaping
    #[serde(borrow)]
    subject: Cow<'a, str>, // Cow needs #[serde(borrow)]: borrows when possible, allocates when it must (escapes)
    #[serde(borrow)]
    to: Vec<&'a str>,
}
fn within(s: &str, input: &str) -> bool {
    let (base, p) = (input.as_ptr() as usize, s.as_ptr() as usize);
    p >= base && p + s.len() <= base + input.len()
}

fn borrowed_demo() {
    let input = r#"{"from":"alice","subject":"hello world","to":["bob","carol"]}"#;
    let m: Msg = serde_json::from_str(input).unwrap();
    assert!(within(m.from, input) && matches!(m.subject, Cow::Borrowed(s) if within(s, input)));
    assert!(
        m.to.iter().all(|t| within(t, input)),
        "no byte of the payload was copied"
    );
    // an escape forces a copy: Cow falls back to Owned, which is exactly why it exists
    let escaped = r#"{"from":"alice","subject":"line1\nline2","to":[]}"#;
    let m: Msg = serde_json::from_str(escaped).unwrap();
    assert!(matches!(m.subject, Cow::Owned(_)) && m.subject == "line1\nline2");
    // but &str cannot fall back: the error message names the problem
    let e = serde_json::from_str::<Msg>(r#"{"from":"a\tb","subject":"s","to":[]}"#)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("invalid type: string \"a\\tb\", expected a borrowed string"),
        "{e}"
    );
    // from_reader / from_value cannot lend: DeserializeOwned means "no borrowing from the input"
    fn needs_owned<T: serde::de::DeserializeOwned>() {}
    needs_owned::<Server>();
    println!(
        "3. borrowed: from/to/subject point inside the input buffer; escapes degrade to Owned"
    );
}

// =================================================================================================================
// 4. DeserializeSeed: stateful, context-dependent deserialisation
// =================================================================================================================

/// Plain `Deserialize` has no way to receive context (it is a static trait method). A seed is a value that carries
/// context and implements `deserialize(self, d)`, so state can flow *into* the parse.
struct Interner(RefCell<HashSet<Rc<str>>>);
struct InternSeed<'a>(&'a Interner);
impl<'de> DeserializeSeed<'de> for InternSeed<'_> {
    type Value = Rc<str>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Rc<str>, D::Error> {
        let s = String::deserialize(d)?;
        let mut pool = (self.0).0.borrow_mut();
        if let Some(existing) = pool.get(s.as_str()) {
            return Ok(Rc::clone(existing)); // share one allocation among all equal strings
        }
        let rc: Rc<str> = Rc::from(s);
        pool.insert(Rc::clone(&rc));
        Ok(rc)
    }
}
struct InternList<'a>(&'a Interner);
impl<'de> DeserializeSeed<'de> for InternList<'_> {
    type Value = Vec<Rc<str>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        struct V<'a>(&'a Interner);
        impl<'de> Visitor<'de> for V<'_> {
            type Value = Vec<Rc<str>>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of strings")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(a.size_hint().unwrap_or(0).min(1024));
                while let Some(x) = a.next_element_seed(InternSeed(self.0))? {
                    out.push(x);
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V(self.0))
    }
}
/// Schema-driven validation: the number of columns is known only at runtime (from a header), so a static type
/// cannot express "every row has `cols` entries". The seed checks it while parsing, with a precise message.
struct Rows {
    cols: usize,
}
impl<'de> DeserializeSeed<'de> for Rows {
    type Value = Vec<Vec<f64>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        struct V(usize);
        impl<'de> Visitor<'de> for V {
            type Value = Vec<Vec<f64>>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "rows of exactly {} numbers", self.0)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut rows = Vec::new();
                while let Some(row) = a.next_element::<Vec<f64>>()? {
                    if row.len() != self.0 {
                        return Err(de::Error::custom(format!(
                            "row {} has {} columns, schema says {}",
                            rows.len(),
                            row.len(),
                            self.0
                        )));
                    }
                    rows.push(row);
                }
                Ok(rows)
            }
        }
        d.deserialize_seq(V(self.cols))
    }
}

fn seed_demo() {
    let interner = Interner(RefCell::new(HashSet::new()));
    let mut de = serde_json::Deserializer::from_str(r#"["us","eu","us","us","eu"]"#);
    let v = InternList(&interner).deserialize(&mut de).unwrap();
    de.end().unwrap();
    assert!(Rc::ptr_eq(&v[0], &v[2]) && Rc::ptr_eq(&v[0], &v[3]) && Rc::ptr_eq(&v[1], &v[4]));
    assert_eq!(interner.0.borrow().len(), 2, "5 strings, 2 allocations");
    let mut de = serde_json::Deserializer::from_str("[[1,2],[3,4]]");
    assert_eq!(
        Rows { cols: 2 }.deserialize(&mut de).unwrap(),
        vec![vec![1.0, 2.0], vec![3.0, 4.0]]
    );
    let mut de = serde_json::Deserializer::from_str("[[1,2],[3]]");
    let e = Rows { cols: 2 }
        .deserialize(&mut de)
        .unwrap_err()
        .to_string();
    assert!(e.contains("row 1 has 1 columns, schema says 2"), "{e}");
    println!(
        "4. seeds: 5 strings interned into {} allocations; schema error: {e}",
        interner.0.borrow().len()
    );
}

// =================================================================================================================
// 5. A complete custom data format: "tiny" (compact, self-describing, binary)
// =================================================================================================================
//
// Every value starts with a one-byte TAG so the decoder can implement `deserialize_any` (self-describing: no schema
// needed to walk the bytes). Structs and tuples are sequences WITHOUT field names, which is what makes it compact;
// enums are (variant INDEX varint, payload). Integers are LEB128 varints (signed ones zigzag-encoded).
//
//   NONE=0  FALSE=1  TRUE=2  UINT=3 varint  SINT=4 zigzag-varint  F64=5 8-byte LE  STR=6 len bytes  BYTES=7 len bytes
//   SOME=8 value  SEQ=9 len values  MAP=10 len (key value)*  ENUM=11 index payload  UNIT=12

const T_NONE: u8 = 0;
const T_FALSE: u8 = 1;
const T_TRUE: u8 = 2;
const T_UINT: u8 = 3;
const T_SINT: u8 = 4;
const T_F64: u8 = 5;
const T_STR: u8 = 6;
const T_BYTES: u8 = 7;
const T_SOME: u8 = 8;
const T_SEQ: u8 = 9;
const T_MAP: u8 = 10;
const T_ENUM: u8 = 11;
const T_UNIT: u8 = 12;
const MAX_DEPTH: usize = 64; // untrusted input must not be able to overflow our stack

/// Every decode error carries the byte offset where it was detected, so a corrupt file can be diagnosed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
enum Error {
    #[error("{0}")]
    Message(String),
    #[error("at byte {pos}: {msg}")]
    At { pos: usize, msg: String },
    #[error("at byte {pos}: unexpected end of input")]
    Eof { pos: usize },
    #[error("at byte {pos}: invalid tag {tag}")]
    BadTag { pos: usize, tag: u8 },
    #[error("at byte {pos}: invalid UTF-8")]
    Utf8 { pos: usize },
    #[error("at byte {pos}: varint too long")]
    VarintOverflow { pos: usize },
    #[error("at byte {pos}: length {len} exceeds remaining input")]
    LengthTooLarge { pos: usize, len: u64 },
    #[error("at byte {pos}: nesting deeper than {MAX_DEPTH}")]
    TooDeep { pos: usize },
    #[error("at byte {pos}: {n} trailing bytes after value")]
    Trailing { pos: usize, n: usize },
    #[error("at byte {pos}: sequence has unread elements")]
    UnreadItems { pos: usize },
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}
impl ser::Error for Error {
    fn custom<T: Display>(m: T) -> Self {
        Error::Message(m.to_string())
    }
}
impl de::Error for Error {
    fn custom<T: Display>(m: T) -> Self {
        Error::Message(m.to_string())
    }
}
type Result<T, E = Error> = std::result::Result<T, E>;

// ---------------------------------------------- serializer ----------------------------------------------------

struct Ser {
    out: Vec<u8>,
}
impl Ser {
    fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.out.push(v as u8 | 0x80);
            v >>= 7;
        }
        self.out.push(v as u8);
    }
    fn header(&mut self, tag: u8, len: usize) {
        self.out.push(tag);
        self.varint(len as u64);
    }
}
/// One helper type implements all the compound traits: seq, tuple, struct, map and their enum variants are all
/// "a header was written, now write the children" here.
struct Compound<'a>(&'a mut Ser);

fn to_bytes<T: Serialize + ?Sized>(v: &T) -> Result<Vec<u8>> {
    let mut s = Ser { out: Vec::new() };
    v.serialize(&mut s)?;
    Ok(s.out)
}

impl<'a> Serializer for &'a mut Ser {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    // binary formats should say so: types like Color/HumanDuration then pick their compact representation
    fn is_human_readable(&self) -> bool {
        false
    }
    fn serialize_bool(self, v: bool) -> Result<()> {
        self.out.push(if v { T_TRUE } else { T_FALSE });
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<()> {
        self.out.push(T_SINT);
        self.varint(((v << 1) ^ (v >> 63)) as u64); // zigzag: small magnitudes stay small, sign in the low bit
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<()> {
        self.out.push(T_UINT);
        self.varint(v);
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(v.into())
    }
    fn serialize_f64(self, v: f64) -> Result<()> {
        self.out.push(T_F64);
        self.out.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }
    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_str(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_str(self, v: &str) -> Result<()> {
        self.header(T_STR, v.len());
        self.out.extend_from_slice(v.as_bytes());
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.header(T_BYTES, v.len());
        self.out.extend_from_slice(v);
        Ok(())
    }
    fn serialize_none(self) -> Result<()> {
        self.out.push(T_NONE);
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<()> {
        self.out.push(T_SOME);
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<()> {
        self.out.push(T_UNIT);
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<()> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(self, _: &'static str, idx: u32, _: &'static str) -> Result<()> {
        self.out.push(T_ENUM);
        self.varint(idx.into());
        self.serialize_unit()
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, v: &T) -> Result<()> {
        v.serialize(self) // transparent: a newtype costs nothing on the wire
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        idx: u32,
        _: &'static str,
        v: &T,
    ) -> Result<()> {
        self.out.push(T_ENUM);
        self.varint(idx.into());
        v.serialize(self)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Compound<'a>> {
        // a length prefix needs the length up front; streaming unknown-length sequences would need a terminator
        let len = len.ok_or(Error::Unsupported("sequences of unknown length"))?;
        self.header(T_SEQ, len);
        Ok(Compound(self))
    }
    fn serialize_tuple(self, len: usize) -> Result<Compound<'a>> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Compound<'a>> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        idx: u32,
        _: &'static str,
        len: usize,
    ) -> Result<Compound<'a>> {
        self.out.push(T_ENUM);
        self.varint(idx.into());
        self.serialize_seq(Some(len))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Compound<'a>> {
        let len = len.ok_or(Error::Unsupported("maps of unknown length"))?;
        self.header(T_MAP, len);
        Ok(Compound(self))
    }
    fn serialize_struct(self, _: &'static str, len: usize) -> Result<Compound<'a>> {
        // structs are positional sequences: field names are NOT written. Consequence: `skip_serializing_if` and
        // `flatten` are incompatible with this format, and field order is part of the wire contract.
        self.serialize_seq(Some(len))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        idx: u32,
        _: &'static str,
        len: usize,
    ) -> Result<Compound<'a>> {
        self.serialize_tuple_variant("", idx, "", len)
    }
}
// All seven compound traits do the same thing here (write the child with no extra framing), so one macro stamps them out.
macro_rules! compound {
    ($($tr:ident :: $m:ident),*) => {$(
        impl $tr for Compound<'_> {
            type Ok = ();
            type Error = Error;
            fn $m<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
                v.serialize(&mut *self.0)
            }
            fn end(self) -> Result<()> {
                Ok(())
            }
        }
    )*};
}
compound!(
    SerializeSeq::serialize_element,
    SerializeTuple::serialize_element,
    SerializeTupleStruct::serialize_field,
    SerializeTupleVariant::serialize_field
);
// struct fields carry a name we deliberately drop; maps have separate key and value calls
impl SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, _name: &'static str, v: &T) -> Result<()> {
        v.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        Ok(())
    }
}
impl SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, _name: &'static str, v: &T) -> Result<()> {
        v.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        Ok(())
    }
}
impl SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> Result<()> {
        k.serialize(&mut *self.0)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<()> {
        v.serialize(&mut *self.0)
    }
    fn end(self) -> Result<()> {
        Ok(())
    }
}

// --------------------------------------------- deserializer ---------------------------------------------------

struct De<'de> {
    input: &'de [u8],
    pos: usize,
    depth: usize,
}
impl<'de> De<'de> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .input
            .get(self.pos)
            .ok_or(Error::Eof { pos: self.pos })?;
        self.pos += 1;
        Ok(b)
    }
    fn take(&mut self, n: usize) -> Result<&'de [u8]> {
        // `checked_add`/`get` so a hostile length can never overflow or index out of range
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.input.len())
            .ok_or(Error::Eof {
                pos: self.input.len(),
            })?;
        let s = &self.input[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn varint(&mut self) -> Result<u64> {
        let (mut v, mut shift) = (0u64, 0u32);
        loop {
            let b = self.byte()?;
            if shift == 63 && b > 1 {
                return Err(Error::VarintOverflow { pos: self.pos - 1 }); // 10th byte may only carry 1 bit
            }
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 63 {
                return Err(Error::VarintOverflow { pos: self.pos - 1 });
            }
        }
    }
    /// A length must not exceed the bytes left (every element takes at least one byte), so a 10-byte header can never
    /// make us allocate or loop for gigabytes.
    fn len(&mut self) -> Result<usize> {
        let at = self.pos;
        let n = self.varint()?;
        if n > (self.input.len() - self.pos) as u64 {
            return Err(Error::LengthTooLarge { pos: at, len: n });
        }
        Ok(n as usize)
    }
    fn str(&mut self) -> Result<&'de str> {
        let n = self.len()?;
        let start = self.pos;
        std::str::from_utf8(self.take(n)?).map_err(|_| Error::Utf8 { pos: start })
    }
    fn any<V: Visitor<'de>>(&mut self, v: V) -> Result<V::Value> {
        let tag_pos = self.pos;
        match self.byte()? {
            T_NONE => v.visit_none(),
            T_UNIT => v.visit_unit(),
            T_FALSE => v.visit_bool(false),
            T_TRUE => v.visit_bool(true),
            T_UINT => v.visit_u64(self.varint()?),
            T_SINT => {
                let z = self.varint()?;
                v.visit_i64((z >> 1) as i64 ^ -((z & 1) as i64))
            }
            T_F64 => v.visit_f64(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            T_STR => v.visit_borrowed_str(self.str()?), // lend the input: zero-copy for `&str` and Cow fields
            T_BYTES => {
                let n = self.len()?;
                v.visit_borrowed_bytes(self.take(n)?)
            }
            T_SOME => v.visit_some(&mut *self),
            T_SEQ => {
                let left = self.len()?;
                let mut acc = Counted {
                    de: &mut *self,
                    left,
                };
                let out = v.visit_seq(&mut acc)?;
                acc.finish(out)
            }
            T_MAP => {
                let left = self.len()?;
                let mut acc = Counted {
                    de: &mut *self,
                    left,
                };
                let out = v.visit_map(&mut acc)?;
                acc.finish(out)
            }
            T_ENUM => v.visit_enum(EnumAcc { de: self }),
            tag => Err(Error::BadTag { pos: tag_pos, tag }),
        }
    }
    fn guarded<R>(&mut self, f: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
        if self.depth >= MAX_DEPTH {
            return Err(Error::TooDeep { pos: self.pos });
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }
}

fn from_bytes<'de, T: Deserialize<'de>>(input: &'de [u8]) -> Result<T> {
    let mut d = De {
        input,
        pos: 0,
        depth: 0,
    };
    let v = T::deserialize(&mut d).map_err(|e| match e {
        Error::Message(msg) => Error::At { pos: d.pos, msg }, // attach the position to errors raised by serde/derive code
        other => other,
    })?;
    match input.len() - d.pos {
        0 => Ok(v),
        n => Err(Error::Trailing { pos: d.pos, n }),
    }
}

struct Counted<'a, 'de> {
    de: &'a mut De<'de>,
    left: usize,
}
impl Counted<'_, '_> {
    fn finish<T>(&self, v: T) -> Result<T> {
        // the visitor returned without consuming everything: a bug or a mismatched type; surface it
        if self.left == 0 {
            Ok(v)
        } else {
            Err(Error::UnreadItems { pos: self.de.pos })
        }
    }
}
impl<'de> SeqAccess<'de> for Counted<'_, 'de> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(&mut *self.de).map(Some)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}
impl<'de> MapAccess<'de> for Counted<'_, 'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(&mut *self.de).map(Some)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(&mut *self.de)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}
struct EnumAcc<'a, 'de> {
    de: &'a mut De<'de>,
}
impl<'de> EnumAccess<'de> for EnumAcc<'_, 'de> {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self)> {
        let idx = self.de.varint()?;
        let idx = u32::try_from(idx)
            .map_err(|_| Error::Message(format!("variant index {idx} out of range")))?;
        // the derived variant identifier accepts a u32 index, so hand it a deserializer over the integer
        let d: de::value::U32Deserializer<Error> = idx.into_deserializer();
        Ok((seed.deserialize(d)?, self))
    }
}
impl<'de> VariantAccess<'de> for EnumAcc<'_, 'de> {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        let pos = self.de.pos;
        match self.de.byte()? {
            T_UNIT => Ok(()),
            tag => Err(Error::BadTag { pos, tag }),
        }
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(&mut *self.de)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, v: V) -> Result<V::Value> {
        self.de.guarded(|d| d.any(v))
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value> {
        self.de.guarded(|d| d.any(v))
    }
}

impl<'de> Deserializer<'de> for &mut De<'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        self.guarded(|d| d.any(v))
    }
    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> Result<V::Value> {
        self.guarded(|d| d.any(v)) // NONE -> visit_none, SOME -> visit_some: the tags already say it
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        v: V,
    ) -> Result<V::Value> {
        v.visit_newtype_struct(self) // serializer wrote the inner value directly
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        v: V,
    ) -> Result<V::Value> {
        self.guarded(|d| d.any(v))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
    // everything else is dictated by the tag byte, so one entry point serves all of them
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit unit_struct
        seq tuple tuple_struct map struct identifier ignored_any
    }
}

// ----------------------------------------- types and the test harness -----------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Shape {
    Dot,
    Circle(f64),
    Rect { w: u32, h: u32 },
    Poly(Vec<(i16, i16)>),
    Pair(u8, String),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Item {
    id: u64,
    delta: i64,
    name: String,
    ratio: f64,
    ok: bool,
    tags: Vec<String>,
    shape: Shape,
    attrs: BTreeMap<String, Option<i32>>,
    color: Color,
    ttl: HumanDuration,
    perms: Perms,
    note: Option<String>,
    unit: (),
    child: Option<Box<Item>>,
}
fn rand_string(r: &mut Rng) -> String {
    const ALPHA: [&str; 8] = ["a", "b", "z", "0", "-", "é", "日", "😀"];
    (0..r.below(10))
        .map(|_| ALPHA[r.below(8) as usize])
        .collect()
}
fn rand_item(r: &mut Rng, depth: u32) -> Item {
    Item {
        id: r.next() >> r.below(64),
        delta: (r.next() as i64) >> r.below(64),
        name: rand_string(r),
        ratio: (r.below(1 << 20) as f64 - 524_288.0) / 8.0, // exactly representable: JSON round-trips it bit-for-bit
        ok: r.below(2) == 0,
        tags: (0..r.below(4)).map(|_| rand_string(r)).collect(),
        shape: match r.below(5) {
            0 => Shape::Dot,
            1 => Shape::Circle(r.below(100) as f64 / 4.0),
            2 => Shape::Rect {
                w: r.next() as u32,
                h: r.below(300) as u32,
            },
            3 => Shape::Poly(
                (0..r.below(4))
                    .map(|_| (r.next() as i16, r.next() as i16))
                    .collect(),
            ),
            _ => Shape::Pair(r.next() as u8, rand_string(r)),
        },
        attrs: (0..r.below(3))
            .map(|_| (rand_string(r), (r.below(2) == 0).then(|| r.next() as i32)))
            .collect(),
        color: Color(r.next() as u8, r.next() as u8, r.next() as u8),
        ttl: HumanDuration(r.below(100_000)),
        perms: Perms(r.below(8) as u8),
        note: (r.below(2) == 0).then(|| rand_string(r)),
        unit: (),
        child: (depth > 0 && r.below(2) == 0).then(|| Box::new(rand_item(r, depth - 1))),
    }
}

fn format_demo() {
    // exact bytes for tiny cases pin the wire format down
    assert_eq!(to_bytes(&300u32).unwrap(), [T_UINT, 0xac, 0x02]);
    assert_eq!(to_bytes(&-1i32).unwrap(), [T_SINT, 1]); // zigzag(-1) = 1
    assert_eq!(to_bytes("hi").unwrap(), [T_STR, 2, b'h', b'i']);
    assert_eq!(to_bytes(&Some(true)).unwrap(), [T_SOME, T_TRUE]);
    assert_eq!(to_bytes(&Shape::Circle(0.0)).unwrap()[..2], [T_ENUM, 1]);
    for v in [0u64, 1, 127, 128, 16_383, 16_384, u64::MAX] {
        assert_eq!(from_bytes::<u64>(&to_bytes(&v).unwrap()).unwrap(), v);
    }
    for v in [0i64, -1, 1, i64::MIN, i64::MAX] {
        assert_eq!(from_bytes::<i64>(&to_bytes(&v).unwrap()).unwrap(), v);
    }

    let mut rng = Rng(0xfeed_beef);
    let (mut bin_total, mut json_total, mut nested) = (0usize, 0usize, 0usize);
    for _ in 0..2000 {
        let item = rand_item(&mut rng, 3);
        nested += item.child.is_some() as usize;
        let bin = to_bytes(&item).unwrap();
        assert_eq!(from_bytes::<Item>(&bin).unwrap(), item, "binary round trip");
        let json = serde_json::to_vec(&item).unwrap();
        assert_eq!(
            serde_json::from_slice::<Item>(&json).unwrap(),
            item,
            "json round trip of the same hand-written types"
        );
        bin_total += bin.len();
        json_total += json.len();
    }
    assert!(
        bin_total * 2 < json_total,
        "positional + varint should be well under half of JSON"
    );
    println!(
        "5a. format: 2000 random nested Items ({nested} with children): tiny {bin_total} B vs JSON {json_total} B ({:.0}%)",
        100.0 * bin_total as f64 / json_total as f64
    );

    // the other containers and the Option/unit edge cases through the same derive-driven path
    let mut m: BTreeMap<String, Vec<Option<(u8, bool)>>> = BTreeMap::new();
    m.insert("k".into(), vec![None, Some((1, true))]);
    assert_eq!(
        from_bytes::<BTreeMap<String, Vec<Option<(u8, bool)>>>>(&to_bytes(&m).unwrap()).unwrap(),
        m
    );
    assert_eq!(
        from_bytes::<Option<Option<()>>>(&to_bytes(&Some(None::<()>)).unwrap()).unwrap(),
        Some(None)
    );
    assert_eq!(
        from_bytes::<[u8; 3]>(&to_bytes(&[1u8, 2, 3]).unwrap()).unwrap(),
        [1, 2, 3]
    );
    // type mismatch is an error with a position, not garbage
    let e = from_bytes::<String>(&to_bytes(&5u8).unwrap())
        .unwrap_err()
        .to_string();
    assert!(
        e.starts_with("at byte 2: invalid type: integer `5`, expected a string"),
        "{e}"
    );
    // zero-copy from the binary buffer too
    #[derive(Deserialize)]
    struct Borrowing<'a> {
        name: &'a str,
        #[serde(borrow)]
        tags: Vec<&'a str>,
    }
    let buf = to_bytes(&("alice", vec!["x", "yy"])).unwrap();
    let b: Borrowing = from_bytes(&buf).unwrap();
    let range = buf.as_ptr() as usize..buf.as_ptr() as usize + buf.len();
    assert!(
        range.contains(&(b.name.as_ptr() as usize))
            && b.tags
                .iter()
                .all(|t| range.contains(&(t.as_ptr() as usize)))
    );
}
fn fuzz_demo() {
    let mut rng = Rng(0xbad_f00d);
    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut classify = |r: Result<Item>| match r {
        Ok(_) => *kinds.entry("ok").or_default() += 1,
        Err(e) => {
            let _ = e.to_string(); // formatting must not panic either
            *kinds
                .entry(match e {
                    Error::Message(_) => "message",
                    Error::At { .. } => "at",
                    Error::Eof { .. } => "eof",
                    Error::BadTag { .. } => "bad-tag",
                    Error::Utf8 { .. } => "utf8",
                    Error::VarintOverflow { .. } => "varint",
                    Error::LengthTooLarge { .. } => "length",
                    Error::TooDeep { .. } => "deep",
                    Error::Trailing { .. } => "trailing",
                    Error::UnreadItems { .. } => "unread",
                    Error::Unsupported(_) => "unsupported",
                })
                .or_default() += 1;
        }
    };
    let mut cases = 0usize;
    for _ in 0..150 {
        let good = to_bytes(&rand_item(&mut rng, 2)).unwrap();
        // 1. every truncation must be an Eof-style error (never Ok: the value is incomplete) and never panic
        for cut in (0..good.len()).step_by(3) {
            let r = from_bytes::<Item>(&good[..cut]);
            assert!(
                r.is_err(),
                "truncated input at {cut}/{} decoded successfully",
                good.len()
            );
            if let Err(Error::Eof { pos }) = r {
                assert!(pos <= cut, "error position {pos} beyond input end {cut}");
            }
            classify(r);
            cases += 1;
        }
        // 2. random byte flips and 3. extra trailing garbage
        for _ in 0..40 {
            let mut bad = good.clone();
            for _ in 0..1 + rng.below(3) {
                let i = rng.below(bad.len() as u64) as usize;
                bad[i] = rng.next() as u8;
            }
            classify(from_bytes::<Item>(&bad));
            cases += 1;
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(matches!(
            from_bytes::<Item>(&extra),
            Err(Error::Trailing { n: 1, .. })
        ));
    }
    // 4. pure noise, with a bias to small tag bytes so the decoder gets past the first byte
    for _ in 0..3000 {
        let noise: Vec<u8> = (0..rng.below(64))
            .map(|_| {
                if rng.below(2) == 0 {
                    rng.below(14) as u8
                } else {
                    rng.next() as u8
                }
            })
            .collect();
        classify(from_bytes::<Item>(&noise));
        cases += 1;
    }
    // 5. targeted attacks
    let huge_len = [&[T_SEQ][..], &[0xff; 9], &[0x01]].concat();
    assert!(
        matches!(
            from_bytes::<Vec<u8>>(&huge_len),
            Err(Error::LengthTooLarge { pos: 1, .. })
        ),
        "{:?}",
        from_bytes::<Vec<u8>>(&huge_len)
    );
    let long_varint = [&[T_UINT][..], &[0x80; 12]].concat();
    assert!(matches!(
        from_bytes::<u64>(&long_varint),
        Err(Error::VarintOverflow { .. })
    ));
    let overflow = [&[T_UINT][..], &[0xff; 9], &[0x02]].concat(); // 10th byte carries >1 bit
    assert!(matches!(
        from_bytes::<u64>(&overflow),
        Err(Error::VarintOverflow { .. })
    ));
    let deep = vec![T_SOME; 100_000];
    assert!(matches!(from_bytes::<Option<Option<u8>>>(&deep), Err(_)));
    let mut nest = vec![T_SOME; 100_000];
    nest.push(T_NONE);
    assert!(matches!(from_bytes::<Box<Item>>(&nest), Err(_))); // wrong type, but must fail cleanly rather than overflow
    let bad_utf8 = [T_STR, 2, 0xff, 0xfe];
    assert_eq!(from_bytes::<String>(&bad_utf8), Err(Error::Utf8 { pos: 2 }));
    let bad_variant = [T_ENUM, 99, T_UNIT];
    let e = from_bytes::<Shape>(&bad_variant).unwrap_err().to_string();
    assert!(e.contains("at byte") && e.contains("`99`"), "{e}");
    let e = from_bytes::<u8>(&[T_UINT, 0x80, 0x02])
        .unwrap_err()
        .to_string(); // 256 does not fit u8
    assert!(
        e.starts_with("at byte 3: invalid value: integer `256`"),
        "{e}"
    );
    assert!(
        from_bytes::<u8>(&[])
            .unwrap_err()
            .to_string()
            .contains("unexpected end of input")
    );
    println!("5b. fuzz: {cases} malformed inputs, no panics; outcomes {kinds:?}");
}

fn main() {
    custom_impls_demo();
    attributes_demo();
    borrowed_demo();
    seed_demo();
    format_demo();
    fuzz_demo();
    println!("all serde_custom checks passed");
}
