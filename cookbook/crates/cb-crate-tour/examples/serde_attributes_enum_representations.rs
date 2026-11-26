//! RECIPE 1102 (crates): serde derive attributes in depth: enum representations, defaults, flatten, with, borrow, Value, RawValue, streaming
//! LEVEL: intermediate
//! Use in: API payloads, config files, event logs, anything that must evolve its schema without breaking readers.   Run: cargo run --release -p cb-crate-tour --example serde_attributes_enum_representations
//! COMMON USES: REST/JSON APIs, Kubernetes-style tagged objects, event sourcing envelopes, config with defaults, forwarding unparsed JSON (RawValue), NDJSON log pipelines.
//! METHODS: serde::Serialize, serde::Deserialize, #[serde(rename)], #[serde(rename_all)], #[serde(alias)], #[serde(default)], #[serde(skip)], #[serde(skip_serializing_if)], #[serde(flatten)], #[serde(with)], #[serde(serialize_with)], #[serde(deserialize_with)], #[serde(borrow)], #[serde(bound)], #[serde(transparent)], #[serde(from)], #[serde(into)], #[serde(try_from)], #[serde(deny_unknown_fields)], #[serde(tag)], #[serde(content)], #[serde(untagged)], serde_json::to_string, serde_json::from_str, serde_json::to_value, serde_json::from_value, serde_json::json, serde_json::Value::pointer, serde_json::Value::get, serde_json::Value::as_str, serde_json::value::RawValue, serde_json::Deserializer::from_str, serde_json::StreamDeserializer, serde::de::Visitor, serde_json::Error::line, serde_json::Error::column, serde_json::Error::classify
//! DOCS: https://serde.rs/ and https://docs.rs/serde_json/1/serde_json/
//! CRATES: serde = "1" (1.0.229 current), serde_json = "1" (1.0.154 current, feature raw_value)
//! NOT COVERED (and why): hand-written Serializer/Deserializer formats (see examples/encoding_compression/serde_custom.rs), `serde_with` (external crate), `#[serde(getter)]`/remote derive (rare).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   snake_case JSON keys from camelCase  -> #[serde(rename_all = "camelCase")]  (container) ; #[serde(rename = "x")] (field)
//!   accept old names too                 -> #[serde(alias = "old")]
//!   missing field gets a value           -> #[serde(default)] / #[serde(default = "path::fn")]
//!   omit None / empty on output          -> #[serde(skip_serializing_if = "Option::is_none")]
//!   reject typos in config               -> #[serde(deny_unknown_fields)]  (NOT with flatten!)
//!   merge a nested struct's keys         -> #[serde(flatten)]  ; capture extras -> flatten a HashMap<String, Value>
//!   newtype ID serialised as inner       -> #[serde(transparent)]
//!   validate while deserialising         -> #[serde(try_from = "Raw")]
//!   tagged union, readable               -> #[serde(tag = "type")] (internal) ; adjacent = tag + content ; untagged = try each in order
//!   custom string<->type                 -> #[serde(with = "module")] or serialize_with/deserialize_with
//!   avoid allocating                     -> &str fields, or #[serde(borrow)] on Cow<str>, + from_str/from_slice (from_reader requires an owned type)
//!   pass through unparsed JSON           -> Box<RawValue>
//!   process a stream of JSON docs        -> Deserializer::from_str(s).into_iter::<T>()
//!   explore unknown JSON                 -> serde_json::Value + .pointer("/a/0/b")
//!   error position                       -> err.line(), err.column(), err.classify()
//!
//! Read: https://serde.rs/attributes.html ; https://serde.rs/enum-representations.html ; https://docs.rs/serde_json/latest/serde_json/value/struct.RawValue.html ; Rust API Guidelines C-SERDE
//! TRY NEXT:
//!   - [S] Add `#[serde(deny_unknown_fields)]` to `Settings` and flatten a struct into it; what breaks and why (serde issue #1547)?
//!   - [S] Predict the JSON for `Shape::Circle` under each of the four representations before reading the asserts.
//!   - [M] Write an untagged enum whose first variant swallows too much; reorder the variants and observe the error message quality.
//!   - [M] Replace `Settings::timeout` with a `Duration` newtype accepting "1500ms" and 1.5 using a Visitor.
//!   - [A] PROJECT: pair with `serde_formats_json_toml_yaml_bincode` (the same Envelope in four formats) and `chrono_dates_times_zones` (RFC 3339 `with` module).
#![allow(dead_code)]
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

// =================================================================================================================
// 1. Everyday core: rename, default, skip, Option, alias
// =================================================================================================================
fn default_port() -> u16 {
    8080
}
fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    host_name: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    verbose: bool, // missing => false
    #[serde(alias = "maxConn", alias = "max_connections")]
    max_conns: Option<u32>, // missing => None (Option is special-cased)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    note: Option<String>,
    #[serde(skip_serializing_if = "is_zero", default)]
    retries: u32,
    #[serde(skip)]
    runtime_cache: Vec<u8>, // never (de)serialised; needs Default
    #[serde(rename = "type")]
    kind: String,
}

fn everyday_core() {
    let s: Settings = serde_json::from_str(r#"{"hostName":"a","maxConn":5,"type":"web"}"#).unwrap();
    assert_eq!(s.port, 8080);
    assert!(!s.verbose);
    assert_eq!(s.max_conns, Some(5)); // alias worked
    assert_eq!(
        serde_json::to_string(&s).unwrap(),
        r#"{"hostName":"a","port":8080,"verbose":false,"maxConns":5,"type":"web"}"#
    );
    // Option vs default vs missing: three different questions.
    //   Option<T> w/o default: missing => None, null => None.
    //   T with #[serde(default)]: missing => T::default(), null => ERROR for non-Option T.
    let err =
        serde_json::from_str::<Settings>(r#"{"hostName":"a","port":null,"type":"x"}"#).unwrap_err();
    assert!(err.is_data(), "{err}");
    // Missing required field names the field.
    let err = serde_json::from_str::<Settings>(r#"{"port":1,"type":"x"}"#).unwrap_err();
    assert!(
        err.to_string().contains("missing field `hostName`"),
        "{err}"
    );
    // Unknown fields are ignored by default (forward compatible).
    assert!(serde_json::from_str::<Settings>(r#"{"hostName":"a","type":"x","future":1}"#).is_ok());
}

// =================================================================================================================
// 2. deny_unknown_fields, transparent, from/into/try_from
// =================================================================================================================
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Strict {
    a: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct UserId(u64); // JSON: 7, not {"0":7}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawPercent")]
struct Percent(u8);
#[derive(Deserialize)]
struct RawPercent(i64);
impl TryFrom<RawPercent> for Percent {
    type Error = String;
    fn try_from(r: RawPercent) -> Result<Self, String> {
        u8::try_from(r.0)
            .ok()
            .filter(|v| *v <= 100)
            .map(Percent)
            .ok_or_else(|| format!("{} is not a percentage", r.0))
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(from = "String", into = "String")]
struct Tags(Vec<String>); // wire form: "a,b,c"
impl From<String> for Tags {
    fn from(s: String) -> Self {
        Tags(
            s.split(',')
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }
}
impl From<Tags> for String {
    fn from(t: Tags) -> String {
        t.0.join(",")
    }
}

fn container_attributes() {
    let e = serde_json::from_str::<Strict>(r#"{"a":1,"b":2}"#).unwrap_err();
    assert!(
        e.to_string().contains("unknown field `b`, expected `a`"),
        "{e}"
    );
    assert_eq!(serde_json::to_string(&UserId(7)).unwrap(), "7");
    assert_eq!(serde_json::from_str::<UserId>("7").unwrap(), UserId(7));
    assert_eq!(serde_json::from_str::<Percent>("42").unwrap(), Percent(42));
    let e = serde_json::from_str::<Percent>("142").unwrap_err();
    assert!(e.to_string().contains("142 is not a percentage"));
    let t: Tags = serde_json::from_str(r#""a,b,c""#).unwrap();
    assert_eq!(t.0, ["a", "b", "c"]);
    assert_eq!(serde_json::to_string(&t).unwrap(), r#""a,b,c""#);
}

// =================================================================================================================
// 3. flatten: embedding and catch-all
// =================================================================================================================
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Pagination {
    page: u32,
    per_page: u32,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ListRequest {
    q: String,
    #[serde(flatten)]
    paging: Pagination,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>, // keeps unknown keys (round-trips plugins' fields)
}

fn flatten_demo() {
    let r: ListRequest =
        serde_json::from_value(json!({"q":"x","page":2,"per_page":50,"utm":"mail","n":1})).unwrap();
    assert_eq!(
        r.paging,
        Pagination {
            page: 2,
            per_page: 50
        }
    );
    assert_eq!(r.extra.len(), 2);
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        json!({"q":"x","page":2,"per_page":50,"utm":"mail","n":1})
    );
    // PITFALLS: flatten buffers into an intermediate map (slower), and numbers lose their u64/i64 identity when buffered in some formats;
    // and it cannot be combined with deny_unknown_fields.
}

// =================================================================================================================
// 4. The four enum representations, exact JSON
// =================================================================================================================
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum External {
    Unit,
    Circle(f64),
    Rect { w: u32, h: u32 },
    Pair(u8, u8),
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Internal {
    Unit,
    Rect { w: u32, h: u32 },
    Wrapped(Pagination), // newtype of a struct works; Circle(f64) would fail at runtime
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    Unit,
    Circle(f64),
    Rect { w: u32, h: u32 },
    Pair(u8, u8),
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Untagged {
    Num(i64),
    Text(String),
    Rect { w: u32, h: u32 },
    List(Vec<i64>),
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Level {
    #[serde(alias = "warning")]
    Warn,
    #[serde(rename = "ERR")]
    Error,
    #[serde(other)]
    Unknown, // forward compat for unit-like enums: unknown strings land here (deserialisation only)
}

fn enum_representations() {
    let j = |v: &dyn erased::Ser| v.json();
    // externally tagged (default)
    assert_eq!(j(&External::Unit), r#""Unit""#);
    assert_eq!(j(&External::Circle(1.5)), r#"{"Circle":1.5}"#);
    assert_eq!(
        j(&External::Rect { w: 1, h: 2 }),
        r#"{"Rect":{"w":1,"h":2}}"#
    );
    assert_eq!(j(&External::Pair(1, 2)), r#"{"Pair":[1,2]}"#);
    // internally tagged
    assert_eq!(j(&Internal::Unit), r#"{"type":"Unit"}"#);
    assert_eq!(
        j(&Internal::Rect { w: 1, h: 2 }),
        r#"{"type":"Rect","w":1,"h":2}"#
    );
    assert_eq!(
        j(&Internal::Wrapped(Pagination {
            page: 1,
            per_page: 2
        })),
        r#"{"type":"Wrapped","page":1,"per_page":2}"#
    );
    // adjacently tagged
    assert_eq!(j(&Adjacent::Unit), r#"{"t":"Unit"}"#);
    assert_eq!(j(&Adjacent::Circle(1.5)), r#"{"t":"Circle","c":1.5}"#);
    assert_eq!(j(&Adjacent::Pair(1, 2)), r#"{"t":"Pair","c":[1,2]}"#);
    assert_eq!(
        j(&Adjacent::Rect { w: 1, h: 2 }),
        r#"{"t":"Rect","c":{"w":1,"h":2}}"#
    );
    // untagged: no tag at all
    assert_eq!(j(&Untagged::Num(3)), "3");
    assert_eq!(j(&Untagged::Text("a".into())), r#""a""#);
    assert_eq!(j(&Untagged::Rect { w: 1, h: 2 }), r#"{"w":1,"h":2}"#);
    // round trips
    assert_eq!(
        serde_json::from_str::<Internal>(r#"{"w":1,"type":"Rect","h":2}"#).unwrap(),
        Internal::Rect { w: 1, h: 2 }
    );
    assert_eq!(
        serde_json::from_str::<Untagged>("[1,2]").unwrap(),
        Untagged::List(vec![1, 2])
    );
    assert_eq!(
        serde_json::from_str::<Untagged>(r#""x""#).unwrap(),
        Untagged::Text("x".into())
    );
    // PITFALL: untagged errors are useless ("data did not match any variant") and the FIRST matching variant wins.
    let e = serde_json::from_str::<Untagged>("true").unwrap_err();
    assert!(e.to_string().contains("did not match any variant"), "{e}");
    // PITFALL: internally tagged with a non-map newtype fails at serialisation time, not compile time.
    #[derive(Serialize)]
    #[serde(tag = "type")]
    enum Bad {
        N(u32),
    }
    assert!(serde_json::to_string(&Bad::N(1)).is_err());
    // Unit-variant enums as strings with rename, alias, other.
    assert_eq!(
        serde_json::from_str::<Level>(r#""warning""#).unwrap(),
        Level::Warn
    );
    assert_eq!(
        serde_json::from_str::<Level>(r#""ERR""#).unwrap(),
        Level::Error
    );
    assert_eq!(
        serde_json::from_str::<Level>(r#""fatal""#).unwrap(),
        Level::Unknown
    );
}

/// Tiny erasure helper so the assertions above can take any Serialize with one closure.
mod erased {
    pub trait Ser {
        fn json(&self) -> String;
    }
    impl<T: serde::Serialize> Ser for T {
        fn json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }
}

// =================================================================================================================
// 5. with / serialize_with / deserialize_with and a Visitor (durations, base64-free hex bytes)
// =================================================================================================================
mod duration_ms_or_str {
    use super::*;
    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_millis() as u64)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Duration;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("milliseconds as an integer or a string like \"1500ms\" / \"2s\"")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Duration, E> {
                Ok(Duration::from_millis(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Duration, E> {
                u64::try_from(v)
                    .map(Duration::from_millis)
                    .map_err(|_| E::custom("negative duration"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Duration, E> {
                if let Some(ms) = v.strip_suffix("ms") {
                    ms.trim()
                        .parse()
                        .map(Duration::from_millis)
                        .map_err(E::custom)
                } else if let Some(s) = v.strip_suffix('s') {
                    s.trim().parse().map(Duration::from_secs).map_err(E::custom)
                } else {
                    Err(E::invalid_value(de::Unexpected::Str(v), &self))
                }
            }
        }
        d.deserialize_any(V) // deserialize_any: needs a self-describing format (JSON yes, bincode no)
    }
}
mod hex_bytes {
    use super::*;
    pub fn serialize<S: Serializer>(b: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&b.iter().map(|x| format!("{x:02x}")).collect::<String>())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() % 2 != 0 {
            return Err(de::Error::custom("odd hex length"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(de::Error::custom))
            .collect()
    }
}
fn upper<S: Serializer>(v: &String, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_uppercase())
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Job {
    #[serde(with = "duration_ms_or_str")]
    timeout: Duration,
    #[serde(with = "hex_bytes")]
    digest: Vec<u8>,
    #[serde(serialize_with = "upper", deserialize_with = "trimmed")]
    // each direction can be customised on its own
    region: String,
}
fn trimmed<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(String::deserialize(d)?.trim().to_owned())
}

fn with_modules() {
    let j: Job =
        serde_json::from_str(r#"{"timeout":"1500ms","digest":"00ff10","region":"  eu "}"#).unwrap();
    assert_eq!(j.timeout, Duration::from_millis(1500));
    assert_eq!(j.digest, vec![0, 255, 16]);
    assert_eq!(
        serde_json::to_string(&j).unwrap(),
        r#"{"timeout":1500,"digest":"00ff10","region":"EU"}"#
    );
    assert_eq!(
        serde_json::from_str::<Job>(r#"{"timeout":2,"digest":"","region":""}"#)
            .unwrap()
            .timeout,
        Duration::from_millis(2)
    );
    assert_eq!(
        serde_json::from_str::<Job>(r#"{"timeout":"2s","digest":"","region":""}"#)
            .unwrap()
            .timeout,
        Duration::from_secs(2)
    );
    let e =
        serde_json::from_str::<Job>(r#"{"timeout":"soon","digest":"","region":""}"#).unwrap_err();
    assert!(e.to_string().contains("expected milliseconds"), "{e}");
    assert!(serde_json::from_str::<Job>(r#"{"timeout":1,"digest":"abc","region":""}"#).is_err());
}

// =================================================================================================================
// 6. Zero-copy: borrow, Cow, bound
// =================================================================================================================
#[derive(Debug, Deserialize)]
struct Borrowed<'a> {
    name: &'a str, // &str fields borrow automatically; fails if the JSON string needs unescaping
    #[serde(borrow)]
    title: Cow<'a, str>, // Cow borrows when possible, owns when it must (escapes)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de> + Default"))]
struct Wrapper<T> {
    #[serde(default)]
    inner: T,
}

fn zero_copy() {
    let src = String::from(r#"{"name":"plain","title":"also plain"}"#);
    let b: Borrowed = serde_json::from_str(&src).unwrap();
    let range = src.as_ptr() as usize..src.as_ptr() as usize + src.len();
    assert!(
        range.contains(&(b.name.as_ptr() as usize)),
        "name points into the input"
    );
    assert!(matches!(b.title, Cow::Borrowed(_)));
    let esc = r#"{"name":"x","title":"has \"escape\""}"#;
    let b2: Borrowed = serde_json::from_str(esc).unwrap();
    assert!(
        matches!(b2.title, Cow::Owned(_)),
        "escapes force an owned copy"
    );
    // PITFALL: borrowing `&str` with an escape is an error.
    assert!(serde_json::from_str::<Borrowed>(r#"{"name":"a\nb","title":"t"}"#).is_err());
    // PITFALL: a reader has no long-lived input buffer to point into. `serde_json::from_reader` demands
    // `T: DeserializeOwned` (= `for<'de> Deserialize<'de>`), so this does not even COMPILE:
    //     serde_json::from_reader::<_, Borrowed>(reader)  // error: implementation of `Deserialize` is not general enough
    // The low-level `Deserializer::from_reader` lets the same type through the type checker, and then the data decides:
    let mut de = serde_json::Deserializer::from_reader(src.as_bytes());
    let e = Borrowed::deserialize(&mut de).unwrap_err(); // `&str` can only borrow, so even escape-free input fails
    assert!(
        e.is_data() && e.to_string().contains("expected a borrowed string"),
        "{e}"
    );
    #[derive(Deserialize)]
    struct CowOnly<'a> {
        #[serde(borrow)]
        title: Cow<'a, str>,
    }
    let mut de = serde_json::Deserializer::from_reader(src.as_bytes());
    let c = CowOnly::deserialize(&mut de).unwrap(); // `Cow` can fall back to owning
    assert!(
        matches!(c.title, Cow::Owned(_)),
        "same input that from_str borrowed from is copied when read from a reader"
    );
    // The usual fix when the bytes come from a reader: either deserialize into an owned type,
    // or read everything into a buffer YOU own and borrow from that buffer.
    #[derive(Deserialize)]
    struct Owned {
        name: String,
    }
    assert_eq!(
        serde_json::from_reader::<_, Owned>(src.as_bytes())
            .unwrap()
            .name,
        "plain"
    );
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut src.as_bytes(), &mut buf).unwrap();
    let from_buf: Borrowed = serde_json::from_slice(&buf).unwrap();
    assert!(buf.as_ptr_range().contains(&from_buf.name.as_ptr()));
    let w: Wrapper<Vec<u8>> = serde_json::from_str("{}").unwrap();
    assert!(w.inner.is_empty());
}

// =================================================================================================================
// 7. serde_json::Value, RawValue, streaming, error locations
// =================================================================================================================
#[derive(Deserialize)]
struct Envelope<'a> {
    kind: String,
    #[serde(borrow)]
    payload: &'a serde_json::value::RawValue, // not parsed, only validated and sliced
}

fn value_raw_stream_errors() {
    // Value: explore and edit unknown documents.
    let mut v = json!({"users":[{"name":"ann","tags":["a","b"]},{"name":"bob"}],"n":3});
    assert_eq!(
        v.pointer("/users/0/tags/1").and_then(Value::as_str),
        Some("b")
    );
    assert_eq!(v["users"][1]["tags"], Value::Null); // indexing a missing key yields Null, no panic
    assert!(v.get("missing").is_none());
    v["users"][1]["tags"] = json!(["z"]);
    *v.pointer_mut("/n").unwrap() = json!(4);
    v.as_object_mut().unwrap().remove("users");
    assert_eq!(v, json!({"n":4}));
    // serde_json's number model: u64/i64/f64; u64 above 2^53 survives only as an integer type.
    assert_eq!(json!(u64::MAX).as_u64(), Some(u64::MAX));
    assert_eq!(serde_json::from_str::<Value>("1.0").unwrap().as_i64(), None);
    // Map ordering: Value uses BTreeMap (sorted keys) unless the `preserve_order` feature is on.
    assert_eq!(json!({"b":1,"a":2}).to_string(), r#"{"a":2,"b":1}"#);

    // RawValue: route messages without parsing the payload.
    let src = r#"{"kind":"order","payload":{"id": 7,  "items":[1,2,3]}}"#;
    let env: Envelope = serde_json::from_str(src).unwrap();
    assert_eq!(env.kind, "order");
    assert_eq!(env.payload.get(), r#"{"id": 7,  "items":[1,2,3]}"#); // whitespace preserved byte for byte
    #[derive(Deserialize)]
    struct Order {
        id: u32,
    }
    assert_eq!(
        serde_json::from_str::<Order>(env.payload.get()).unwrap().id,
        7
    );

    // Streaming: NDJSON / concatenated values, no need to buffer a Vec.
    let nd = "{\"a\":1}\n{\"a\":2}\n{\"a\":3}";
    #[derive(Deserialize, Debug)]
    struct A {
        a: u32,
    }
    let all: Vec<u32> = serde_json::Deserializer::from_str(nd)
        .into_iter::<A>()
        .map(|r| r.unwrap().a)
        .collect();
    assert_eq!(all, [1, 2, 3]);
    // A bad record in the middle: the iterator yields Err and you decide whether to stop (it will keep failing).
    let mut it = serde_json::Deserializer::from_str("{\"a\":1} {\"a\":x}").into_iter::<A>();
    assert!(it.next().unwrap().is_ok());
    assert!(it.next().unwrap().is_err());
    // Line-oriented alternative that survives bad lines: parse each `lines()` item independently.
    let ok_lines = "{\"a\":1}\nnot json\n{\"a\":3}"
        .lines()
        .filter_map(|l| serde_json::from_str::<A>(l).ok())
        .count();
    assert_eq!(ok_lines, 2);

    // Error locations and classification.
    let e = serde_json::from_str::<A>("{\n  \"a\": \"nope\"\n}").unwrap_err();
    // Both are 1-based. For a type error the column is the last byte consumed: here the closing quote of "nope".
    assert_eq!((e.line(), e.column()), (2, 13));
    assert_eq!(e.classify(), serde_json::error::Category::Data);
    assert_eq!(
        serde_json::from_str::<A>("{\"a\":").unwrap_err().classify(),
        serde_json::error::Category::Eof
    );
    assert_eq!(
        serde_json::from_str::<A>("{a:1}").unwrap_err().classify(),
        serde_json::error::Category::Syntax
    );
    // Trailing characters are an error with from_str (use the streaming API to allow them).
    assert!(serde_json::from_str::<A>("{\"a\":1} garbage").is_err());
}

// =================================================================================================================
// 8. Schema evolution strategies
// =================================================================================================================
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "v")]
enum Versioned {
    #[serde(rename = "1")]
    V1 { name: String },
    #[serde(rename = "2")]
    V2 { first: String, last: String },
}
#[derive(Debug, PartialEq)]
struct Person {
    first: String,
    last: String,
}
impl From<Versioned> for Person {
    fn from(v: Versioned) -> Self {
        match v {
            Versioned::V1 { name } => {
                let (f, l) = name.split_once(' ').unwrap_or((&name, ""));
                Person {
                    first: f.into(),
                    last: l.into(),
                }
            }
            Versioned::V2 { first, last } => Person { first, last },
        }
    }
}

fn schema_evolution() {
    let old: Versioned = serde_json::from_str(r#"{"v":"1","name":"Ada Lovelace"}"#).unwrap();
    let new: Versioned =
        serde_json::from_str(r#"{"v":"2","first":"Ada","last":"Lovelace"}"#).unwrap();
    assert_eq!(Person::from(old), Person::from(new));
    // Unknown future version => a clean error, not silent data loss.
    assert!(serde_json::from_str::<Versioned>(r#"{"v":"3"}"#).is_err());
}

// =================================================================================================================
// 9. Production tips and tricks
// =================================================================================================================
fn production_tips() {
    // 1. Be liberal on READ (ignore unknown fields) for service payloads; be strict (deny_unknown_fields) for hand-edited config where typos matter.
    assert!(serde_json::from_str::<Strict>(r#"{"a":1,"typo":2}"#).is_err());
    // 2. Cap input size BEFORE parsing untrusted JSON (serde_json has a recursion limit of 128, not a byte limit).
    let deep = "[".repeat(200) + &"]".repeat(200);
    let e = serde_json::from_str::<Value>(&deep).unwrap_err();
    assert!(e.to_string().contains("recursion limit"), "{e}");
    let limit = 1 << 20;
    let body = vec![b'x'; 10];
    assert!(
        body.len() <= limit,
        "check body.len() against a limit first (axum: DefaultBodyLimit)"
    );
    // 3. Never use f64 for money or u64 ids in JSON consumed by JavaScript: encode as strings.
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Big {
        #[serde(with = "u64_as_string")]
        id: u64,
    }
    mod u64_as_string {
        use super::*;
        pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
            s.collect_str(v)
        }
        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
            String::deserialize(d)?.parse().map_err(de::Error::custom)
        }
    }
    let b = Big { id: u64::MAX };
    assert_eq!(
        serde_json::to_string(&b).unwrap(),
        r#"{"id":"18446744073709551615"}"#
    );
    assert_eq!(
        serde_json::from_str::<Big>(&serde_json::to_string(&b).unwrap()).unwrap(),
        b
    );
    assert_ne!(
        u64::MAX as f64 as u64,
        u64::MAX - 1,
        "JS Number cannot hold 2^64 exactly"
    );
    // 4. Newtype IDs with #[serde(transparent)] keep the type system and the wire format both simple (see section 2).
    // 5. Use `to_writer`/`to_writer_pretty` into a BufWriter for big outputs instead of building a String (clippy: `format_collect` is unrelated; the cost is memory).
    let mut buf = Vec::new();
    serde_json::to_writer(&mut buf, &json!({"k": [1, 2]})).unwrap();
    assert_eq!(buf, br#"{"k":[1,2]}"#);
    // 6. Prefer `from_slice`/`from_str` to `from_reader` for performance: from_reader is several times slower and cannot borrow.
    assert!(serde_json::from_slice::<Value>(b"[1]").is_ok());
    // 7. `#[serde(default)]` on the container gives every field its Default when missing: one line to make a config forward/backward compatible.
    #[derive(Deserialize, Debug, PartialEq, Default)]
    #[serde(default)]
    struct Cfg {
        threads: u32,
        name: String,
    }
    assert_eq!(serde_json::from_str::<Cfg>("{}").unwrap(), Cfg::default());
    // 8. Tag enums on the wire explicitly and add `#[serde(other)]` unit variant for forward compatibility (section 4).
    // 9. Do not derive Serialize for secrets without `#[serde(skip_serializing)]`: logs and APIs leak them. Show it:
    #[derive(Serialize)]
    struct Account {
        user: &'static str,
        #[serde(skip_serializing)]
        password: &'static str,
    }
    assert_eq!(
        serde_json::to_string(&Account {
            user: "u",
            password: "p"
        })
        .unwrap(),
        r#"{"user":"u"}"#
    );
    // 10. Test wire compatibility with golden strings (like these asserts): derive output is a public contract once shipped.
    // 11. serde_json::Value is for exploring, not for domain models: typed structs give validation, speed and refactoring safety.
    // 12. Map keys must be strings in JSON: BTreeMap<u32, _> works (serde stringifies integer keys) but a tuple key fails at runtime.
    let mut m = BTreeMap::new();
    m.insert(1u32, "a");
    assert_eq!(serde_json::to_string(&m).unwrap(), r#"{"1":"a"}"#);
    let mut bad = BTreeMap::new();
    bad.insert((1, 2), "a");
    assert!(serde_json::to_string(&bad).is_err());
    // 13. Floats: NaN/Infinity serialise as null in serde_json (silently!); validate before writing.
    assert_eq!(serde_json::to_string(&f64::NAN).unwrap(), "null");
}

pub fn main() {
    everyday_core();
    container_attributes();
    flatten_demo();
    enum_representations();
    with_modules();
    zero_copy();
    value_raw_stream_errors();
    schema_evolution();
    production_tips();
    println!("serde attributes: all sections passed");
}
