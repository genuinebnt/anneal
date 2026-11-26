//! RECIPE 1103 (crates): one data model, seven formats: JSON, TOML, YAML, bincode, postcard, MessagePack, CBOR, plus CSV
//! LEVEL: intermediate
//! Use in: choosing a wire/disk format; migrating between formats without surprising data loss.   Run: cargo run --release -p cb-crate-tour --example serde_formats_json_toml_yaml_bincode
//! COMMON USES: JSON for HTTP APIs, TOML for hand-edited config (Cargo), YAML for CI/Kubernetes files, postcard for embedded and
//! compact RPC, MessagePack/CBOR for schemaless binary payloads (CBOR is the base of COSE/WebAuthn), CSV for exports.
//! METHODS: serde_json::to_vec, serde_json::from_slice, toml::to_string, toml::from_str, toml::value::Datetime, toml::Table, serde_yaml_ng::to_string, serde_yaml_ng::from_str, bincode::serialize, bincode::deserialize, bincode::DefaultOptions, bincode::Options::with_limit, postcard::to_allocvec, postcard::from_bytes, postcard::take_from_bytes, rmp_serde::to_vec, rmp_serde::to_vec_named, rmp_serde::from_slice, ciborium::into_writer, ciborium::from_reader, csv::Writer::serialize, csv::Reader::deserialize, csv::ReaderBuilder, serde_bytes (#[serde(with = "serde_bytes")]), serde_yaml_ng::Deserializer (multi-document), Value::apply_merge, postcard::{to_slice, to_allocvec_cobs, from_bytes_cobs, take_from_bytes_cobs}, csv::WriterBuilder::has_headers, csv::ReaderBuilder::{has_headers, flexible}, serde_json::Deserializer::from_str
//! DOCS: https://docs.rs/serde_json/1/serde_json/ https://docs.rs/toml/0.8/toml/ https://docs.rs/serde_yaml_ng/0.10/serde_yaml_ng/ https://docs.rs/bincode/1/bincode/ https://docs.rs/postcard/1/postcard/ https://docs.rs/rmp-serde/1/rmp_serde/ https://docs.rs/ciborium/0.2/ciborium/ https://docs.rs/csv/1/csv/
//! CRATES: serde_json = "1", toml = "0.8" (not the newest major line), serde_yaml_ng = "0.10", bincode = "1.3", postcard = "1", rmp-serde = "1", ciborium = "0.2", csv = "1", serde_bytes = "0.11"
//!
//! MAINTENANCE STATUS (checked on crates.io and rustsec.org while writing; check again before you adopt anything):
//!   * `serde_yaml` (dtolnay) is published as 0.9.34+deprecated.
//!   * `serde_yml` has advisory RUSTSEC-2025-0068 (unsound and unmaintained). Do not use it.
//!     https://rustsec.org/advisories/RUSTSEC-2025-0068.html
//!   * `serde_yaml_ng` is CHOSEN here: the advisory above lists it as an alternative and keeps the serde_yaml API. Caveat from the
//!     same advisory: it still builds on the unmaintained `unsafe-libyaml`; `serde_norway` is the other fork it lists.
//!   * `bincode` has advisory RUSTSEC-2025-0141 (unmaintained; the team says 1.3.3 is complete). It is shown because existing
//!     code uses it; for NEW code prefer postcard (or evaluate the advisory's other alternatives).
//!     https://rustsec.org/advisories/RUSTSEC-2025-0141.html
//!
//! CHEAT SHEET
//!   humans read it, other languages consume it      -> JSON
//!   humans EDIT it                                  -> TOML (flat-ish config), YAML only if the ecosystem forces it
//!   compact, both ends are Rust, schema versioned   -> postcard (NOT self-describing: field order IS the schema)
//!   compact but self-describing                     -> MessagePack with to_vec_named, or CBOR
//!   flat rows for spreadsheets                      -> CSV
//!   u64 ids that JavaScript will read               -> encode as strings
//!   Vec<u8> in MessagePack/CBOR                     -> mark it with serde_bytes, or every byte is encoded as a number
//!   packet framing for postcard over a serial line  -> COBS (0x00 never appears inside a frame)
//!
//! Read: https://serde.rs/#data-formats ; https://toml.io/en/v1.0.0 ; https://postcard.jamesmunns.com/ ; https://www.rfc-editor.org/rfc/rfc8949.html
//!
//! TRY NEXT:
//!   - [S] Append a new `#[serde(default)]` field at the END of `Record`. Predict which formats can still read old bytes, then test each.
//!   - [S] Switch `to_vec_named` to `to_vec` in the size table and predict the size change.
//!   - [M] Add a one-byte version prefix to the postcard encoding and write the v1 -> v2 upgrade path as an enum.
//!   - [A] PROJECT: with `bytes_buf_buf_mut_framing`, build a framed protocol whose payloads are postcard and whose handshake negotiates JSON for debugging.
#![allow(dead_code)]
use bincode::Options;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Priority {
    Low,
    High,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Shape {
    Circle(f64),
    Rect { w: u32, h: u32 },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    id: u64,
    name: String,
    tags: Vec<String>,
    score: f64,
    note: Option<String>,
    priority: Priority,
    shape: Shape,
    attrs: BTreeMap<String, i64>,
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
        self.next() % n
    }
}
/// Strings that stress quoting and escaping in every text format.
fn random_string(r: &mut Rng) -> String {
    const PIECES: [&str; 18] = [
        "a", "Z", " ", "\"", "'", "\\", "\n", "\t", ",", ":", "#", "- ", "{", "no", "null", "é",
        "日", "🦀",
    ];
    (0..r.below(6))
        .map(|_| PIECES[r.below(PIECES.len() as u64) as usize])
        .collect()
}
/// `max_id` lets each format get values inside its integer range.
fn random_record(r: &mut Rng, max_id: u64) -> Record {
    Record {
        id: if r.below(10) == 0 {
            max_id
        } else {
            r.below(1 << 40)
        },
        name: random_string(r),
        tags: (0..r.below(4)).map(|_| random_string(r)).collect(),
        score: (r.below(2_000_001) as f64 - 1_000_000.0) / 64.0, // exactly representable, finite
        note: (r.below(2) == 0).then(|| random_string(r)),
        priority: if r.below(2) == 0 {
            Priority::Low
        } else {
            Priority::High
        },
        shape: if r.below(2) == 0 {
            Shape::Circle(r.below(100) as f64 / 4.0)
        } else {
            Shape::Rect {
                w: r.below(1000) as u32,
                h: r.below(1000) as u32,
            }
        },
        attrs: (0..r.below(4))
            .map(|i| (format!("k{i}"), r.below(1000) as i64 - 500))
            .collect(),
    }
}

type Encode = fn(&Record) -> Vec<u8>;
type Decode = fn(&[u8]) -> Record;
struct Format {
    name: &'static str,
    max_id: u64,
    encode: Encode,
    decode: Decode,
}
fn formats() -> Vec<Format> {
    fn text(b: &[u8]) -> &str {
        std::str::from_utf8(b).unwrap()
    }
    vec![
        Format {
            name: "serde_json",
            max_id: u64::MAX,
            encode: |v| serde_json::to_vec(v).unwrap(),
            decode: |b| serde_json::from_slice(b).unwrap(),
        },
        Format {
            name: "toml",
            max_id: i64::MAX as u64,
            encode: |v| toml::to_string(v).unwrap().into_bytes(),
            decode: |b| toml::from_str(text(b)).unwrap(),
        },
        Format {
            name: "serde_yaml_ng",
            max_id: u64::MAX,
            encode: |v| serde_yaml_ng::to_string(v).unwrap().into_bytes(),
            decode: |b| serde_yaml_ng::from_slice(b).unwrap(),
        },
        Format {
            name: "bincode 1.3",
            max_id: u64::MAX,
            encode: |v| bincode::serialize(v).unwrap(),
            decode: |b| bincode::deserialize(b).unwrap(),
        },
        Format {
            name: "postcard",
            max_id: u64::MAX,
            encode: |v| postcard::to_allocvec(v).unwrap(),
            decode: |b| postcard::from_bytes(b).unwrap(),
        },
        Format {
            name: "msgpack named",
            max_id: u64::MAX,
            encode: |v| rmp_serde::to_vec_named(v).unwrap(),
            decode: |b| rmp_serde::from_slice(b).unwrap(),
        },
        Format {
            name: "msgpack compact",
            max_id: u64::MAX,
            encode: |v| rmp_serde::to_vec(v).unwrap(),
            decode: |b| rmp_serde::from_slice(b).unwrap(),
        },
        Format {
            name: "cbor (ciborium)",
            max_id: u64::MAX,
            encode: |v| {
                let mut out = Vec::new();
                ciborium::into_writer(v, &mut out).unwrap();
                out
            },
            decode: |b| ciborium::from_reader(b).unwrap(),
        },
    ]
}

fn round_trips_and_sizes() {
    let sample = Record {
        id: 42,
        name: "widget".into(),
        tags: vec!["a".into(), "b".into()],
        score: 2.5,
        note: None,
        priority: Priority::High,
        shape: Shape::Rect { w: 3, h: 4 },
        attrs: BTreeMap::from([("stock".to_string(), 7)]),
    };
    let mut sizes = BTreeMap::new();
    println!(
        "{:<17} {:>12} {:>22}",
        "format", "sample bytes", "500 random records"
    );
    for f in formats() {
        let mut r = Rng(1103);
        let mut total = 0;
        for i in 0..500 {
            let v = random_record(&mut r, f.max_id);
            let bytes = (f.encode)(&v);
            total += bytes.len();
            assert_eq!((f.decode)(&bytes), v, "{} failed on record {i}", f.name);
        }
        let n = (f.encode)(&sample).len();
        assert_eq!((f.decode)(&(f.encode)(&sample)), sample);
        sizes.insert(f.name, n);
        println!("{:<17} {n:>12} {total:>22}", f.name);
    }
    // Field NAMES dominate small records: the formats that omit them are the smallest.
    assert!(
        sizes["postcard"] < sizes["bincode 1.3"],
        "varints beat bincode 1.x's default fixed-width integers"
    );
    assert!(sizes["msgpack compact"] < sizes["msgpack named"]);
    assert!(sizes["msgpack named"] < sizes["serde_json"]);
}

fn json_gotchas() {
    // u64 is exact in serde_json, but a consumer that parses numbers as f64 (JavaScript) loses precision above 2^53.
    let big = (1u64 << 53) + 1;
    assert_eq!(serde_json::from_str::<u64>(&big.to_string()).unwrap(), big);
    assert_ne!(
        serde_json::from_str::<f64>(&big.to_string()).unwrap() as u64,
        big
    );
    // NaN and infinity are written as null with NO error, and null does not read back as f64.
    assert_eq!(
        serde_json::to_string(&[f64::NAN, f64::INFINITY]).unwrap(),
        "[null,null]"
    );
    assert!(serde_json::from_str::<f64>("null").is_err());
    // Object keys are strings: integer keys are stringified and parsed back; composite keys are an error.
    let m = BTreeMap::from([(7u32, true)]);
    assert_eq!(serde_json::to_string(&m).unwrap(), r#"{"7":true}"#);
    assert_eq!(
        serde_json::from_str::<BTreeMap<u32, bool>>(r#"{"7":true}"#).unwrap(),
        m
    );
    let e = serde_json::to_string(&BTreeMap::from([((1, 2), 3)])).unwrap_err();
    assert!(e.to_string().contains("key must be a string"), "{e}");
    // The usual fix: serialise as a list of pairs.
    assert_eq!(
        serde_json::to_string(&[((1, 2), 3)]).unwrap(),
        "[[[1,2],3]]"
    );
}

fn toml_gotchas() {
    // No null. A `None` struct field is simply omitted ...
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct C {
        a: Option<u32>,
        b: u32,
    }
    assert_eq!(toml::to_string(&C { a: None, b: 1 }).unwrap(), "b = 1\n");
    assert_eq!(toml::from_str::<C>("b = 1").unwrap(), C { a: None, b: 1 });
    // ... but a None where a VALUE is required has no representation at all.
    #[derive(Serialize)]
    struct L {
        xs: Vec<Option<u32>>,
    }
    assert!(
        toml::to_string(&L {
            xs: vec![Some(1), None]
        })
        .is_err()
    );
    // The top level must be a table.
    assert!(toml::to_string(&5u32).is_err());
    // Integers are i64 in toml 0.8: large u64 values are rejected, not wrapped.
    assert!(
        toml::to_string(&C {
            a: None,
            b: u32::MAX
        })
        .is_ok()
    );
    #[derive(Serialize)]
    struct Big {
        v: u64,
    }
    assert!(toml::to_string(&Big { v: u64::MAX }).is_err());
    // Date-times are a native TOML type. serde sees toml's own Datetime; a plain String field does not accept one.
    #[derive(Deserialize, Serialize)]
    struct D {
        at: toml::value::Datetime,
    }
    let d: D = toml::from_str("at = 2024-03-05T10:20:30Z").unwrap();
    assert_eq!(
        d.at.date.map(|d| (d.year, d.month, d.day)),
        Some((2024, 3, 5))
    );
    assert_eq!(
        toml::to_string(&d).unwrap().trim(),
        "at = 2024-03-05T10:20:30Z"
    );
    #[derive(Deserialize)]
    struct S {
        at: String,
    }
    assert!(toml::from_str::<S>("at = 2024-03-05T10:20:30Z").is_err());
    // Dynamic access and error messages that point at the line.
    let t: toml::Table =
        toml::from_str("[server]\nport = 80\n[[server.route]]\npath = '/a'\n").unwrap();
    assert_eq!(t["server"]["port"].as_integer(), Some(80));
    assert_eq!(t["server"]["route"][0]["path"].as_str(), Some("/a"));
    let e = toml::from_str::<C>("b = 1\nb = 2").unwrap_err();
    assert!(e.to_string().contains("line 2"), "{e}");
}

fn yaml_gotchas() {
    // Enum variants with data are written as YAML TAGS, not as the one-key maps you know from JSON.
    let y = serde_yaml_ng::to_string(&Shape::Circle(1.5)).unwrap();
    assert_eq!(y, "!Circle 1.5\n");
    assert_eq!(
        serde_yaml_ng::from_str::<Shape>(&y).unwrap(),
        Shape::Circle(1.5)
    );
    // "The Norway problem": this parser keeps `no`, `yes`, `on` as strings (only true/false are booleans) ...
    for word in ["no", "yes", "on"] {
        assert_eq!(
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(word)
                .unwrap()
                .as_str(),
            Some(word)
        );
    }
    // ... but other YAML 1.1 parsers reading YOUR file may not, so quote such strings in shared files.
    // Unquoted scalars are typed by their looks: a "version" can silently become a float.
    let v: serde_yaml_ng::Value = serde_yaml_ng::from_str("version: 1.10").unwrap();
    assert_eq!(v["version"].as_f64(), Some(1.1));
    let v: serde_yaml_ng::Value = serde_yaml_ng::from_str("version: \"1.10\"").unwrap();
    assert_eq!(v["version"].as_str(), Some("1.10"));
    // Non-string keys are fine, unlike JSON.
    let m: BTreeMap<(u8, u8), u8> = BTreeMap::from([((1, 2), 3)]);
    assert_eq!(
        serde_yaml_ng::from_str::<BTreeMap<(u8, u8), u8>>(&serde_yaml_ng::to_string(&m).unwrap())
            .unwrap(),
        m
    );
}

fn non_self_describing_gotchas() {
    // postcard and bincode write values only. The reader's TYPE is the schema.
    assert_eq!(
        postcard::to_allocvec(&(1u32, "hi", true)).unwrap(),
        [1, 2, b'h', b'i', 1]
    );
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct V1 {
        a: u32,
        b: u32,
    }
    #[derive(Deserialize, Debug, PartialEq)]
    struct Swapped {
        b: u32,
        a: u32,
    }
    let bytes = postcard::to_allocvec(&V1 { a: 1, b: 2 }).unwrap();
    // Reordering fields silently swaps data: no error at all.
    assert_eq!(
        postcard::from_bytes::<Swapped>(&bytes).unwrap(),
        Swapped { b: 1, a: 2 }
    );
    // PITFALL: skip_serializing_if removes bytes the reader still expects.
    #[derive(Serialize, Deserialize, Debug)]
    struct Skippy {
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<u32>,
        b: u32,
    }
    let bytes = postcard::to_allocvec(&Skippy { a: None, b: 7 }).unwrap();
    assert!(postcard::from_bytes::<Skippy>(&bytes).is_err());
    // Anything that needs deserialize_any (untagged and internally tagged enums, serde_json::Value) cannot be READ.
    #[derive(Serialize, Deserialize, Debug)]
    #[serde(tag = "type")]
    enum Tagged {
        A { x: u32 },
    }
    let bytes = postcard::to_allocvec(&Tagged::A { x: 1 }).unwrap(); // writing succeeds, which is the trap
    assert!(postcard::from_bytes::<Tagged>(&bytes).is_err());
    let bytes = bincode::serialize(&Tagged::A { x: 1 }).unwrap();
    assert!(bincode::deserialize::<Tagged>(&bytes).is_err());
    // Framing: take_from_bytes returns the unread rest, so concatenated messages can be peeled off.
    let mut stream = postcard::to_allocvec(&V1 { a: 1, b: 2 }).unwrap();
    stream.extend(postcard::to_allocvec(&V1 { a: 300, b: 4 }).unwrap());
    let (first, rest) = postcard::take_from_bytes::<V1>(&stream).unwrap();
    let (second, rest) = postcard::take_from_bytes::<V1>(rest).unwrap();
    assert_eq!((first.a, second.a, rest.len()), (1, 300, 0));
    // bincode 1.x: the free functions and `DefaultOptions` are DIFFERENT encodings.
    assert_eq!(bincode::serialize(&1u64).unwrap().len(), 8); // fixint
    assert_eq!(
        bincode::DefaultOptions::new()
            .serialize(&1u64)
            .unwrap()
            .len(),
        1
    ); // varint
    // The free functions ignore trailing bytes; DefaultOptions rejects them.
    assert_eq!(bincode::deserialize::<u8>(&[1, 99]).unwrap(), 1);
    assert!(
        bincode::DefaultOptions::new()
            .deserialize::<u8>(&[1, 99])
            .is_err()
    );
    // Untrusted input: set a size limit.
    let huge_len = u64::MAX.to_le_bytes();
    let limited = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(1024);
    assert!(limited.deserialize::<Vec<u8>>(&huge_len).is_err());
}

fn self_describing_binary() {
    // MessagePack "compact" writes structs as ARRAYS (positional); "named" writes maps with field names.
    #[derive(Serialize)]
    struct P {
        x: u8,
    }
    assert_eq!(rmp_serde::to_vec(&P { x: 5 }).unwrap(), [0x91, 0x05]); // fixarray(1)
    assert_eq!(
        rmp_serde::to_vec_named(&P { x: 5 }).unwrap(),
        [0x81, 0xa1, b'x', 0x05]
    ); // fixmap(1)
    // CBOR (RFC 8949): map(1), text(1) "x", unsigned 5. It also has real byte strings, floats incl. NaN, and non-string keys.
    let mut out = Vec::new();
    ciborium::into_writer(&P { x: 5 }, &mut out).unwrap();
    assert_eq!(out, [0xa1, 0x61, b'x', 0x05]);
    let mut nan = Vec::new();
    ciborium::into_writer(&f64::NAN, &mut nan).unwrap();
    assert!(
        ciborium::from_reader::<f64, _>(nan.as_slice())
            .unwrap()
            .is_nan(),
        "unlike JSON, NaN survives"
    );
    // Self-describing means you can decode without the type, here into serde_json::Value.
    let v: serde_json::Value = ciborium::from_reader(out.as_slice()).unwrap();
    assert_eq!(v, serde_json::json!({"x": 5}));
}

fn csv_section() {
    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct Row {
        id: u32,
        name: String,
        note: Option<String>,
        priority: Priority,
    }
    let rows = vec![
        Row {
            id: 1,
            name: "plain".into(),
            note: None,
            priority: Priority::Low,
        },
        Row {
            id: 2,
            name: "has,comma \"quote\"\nnewline".into(),
            note: Some("n".into()),
            priority: Priority::High,
        },
    ];
    let mut w = csv::Writer::from_writer(Vec::new());
    for row in &rows {
        w.serialize(row).unwrap();
    }
    let text = String::from_utf8(w.into_inner().unwrap()).unwrap();
    assert!(
        text.starts_with("id,name,note,priority\n1,plain,,Low\n"),
        "{text}"
    );
    let back: Vec<Row> = csv::Reader::from_reader(text.as_bytes())
        .deserialize()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(back, rows);
    // GOTCHA: None and Some("") are both an empty field.
    let mut w = csv::Writer::from_writer(Vec::new());
    w.serialize(Row {
        note: Some(String::new()),
        ..rows[0].clone()
    })
    .unwrap();
    let text = w.into_inner().unwrap();
    let back: Row = csv::Reader::from_reader(text.as_slice())
        .deserialize()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(back.note, None);
    // Nested containers do not fit in a flat row once headers are written.
    #[derive(Serialize)]
    struct Nested {
        inner: Row,
    }
    assert!(
        csv::Writer::from_writer(Vec::new())
            .serialize(Nested {
                inner: rows[0].clone()
            })
            .is_err()
    );
    // Dialects and error positions.
    let mut rd = csv::ReaderBuilder::new()
        .delimiter(b';')
        .trim(csv::Trim::All)
        .from_reader("id; name; note; priority\n 3 ; x ;; Low\nzz;y;;Low\n".as_bytes());
    let mut it = rd.deserialize::<Row>();
    assert_eq!(it.next().unwrap().unwrap().id, 3);
    let e = it.next().unwrap().unwrap_err();
    assert_eq!(e.position().map(|p| p.line()), Some(3));
}

/// u64 as a JSON string on the wire, a number in Rust. Accepts only strings, so a producer cannot silently lose precision.
mod u64_as_string {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

fn json_hostile_input() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Id {
        #[serde(with = "u64_as_string")]
        id: u64,
    }
    let text = serde_json::to_string(&Id { id: u64::MAX }).unwrap();
    assert_eq!(text, r#"{"id":"18446744073709551615"}"#);
    assert_eq!(
        serde_json::from_str::<Id>(&text).unwrap(),
        Id { id: u64::MAX }
    );
    assert!(serde_json::from_str::<Id>(r#"{"id":5}"#).is_err());

    // PITFALL: duplicate keys. A derived struct rejects them; Value and maps keep the LAST one without a word. If one
    // service validates with a map and another reads a different parser that keeps the first, they disagree on the data.
    let dup = r#"{"id":"1","id":"2"}"#;
    let e = serde_json::from_str::<Id>(dup).unwrap_err();
    assert!(e.to_string().contains("duplicate field `id`"), "{e}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(dup).unwrap()["id"],
        "2"
    );
    assert_eq!(
        serde_json::from_str::<BTreeMap<String, String>>(dup).unwrap()["id"],
        "2"
    );

    // Deeply nested input cannot overflow the stack: there is a recursion limit (127 levels accepted, 128 rejected).
    let nested = |n: usize| "[".repeat(n) + &"]".repeat(n);
    assert!(serde_json::from_str::<serde_json::Value>(&nested(127)).is_ok());
    let e = serde_json::from_str::<serde_json::Value>(&nested(128)).unwrap_err();
    assert!(e.to_string().contains("recursion limit exceeded"), "{e}");
    assert!(serde_json::from_str::<serde_json::Value>(&nested(100_000)).is_err());
    // classify() separates "wait for more input" from "this is broken".
    assert!(
        serde_json::from_str::<Id>(r#"{"id":"1"#)
            .unwrap_err()
            .is_eof()
    );
    assert!(
        serde_json::from_str::<Id>(r#"{id}"#)
            .unwrap_err()
            .is_syntax()
    );
    assert!(
        serde_json::from_str::<Id>(r#"{"id":[]}"#)
            .unwrap_err()
            .is_data()
    );
    // from_str demands that nothing but whitespace follows; a stream deserializer reads value after value.
    assert!(serde_json::from_str::<u8>("1 2").is_err());
    let many: Vec<u8> = serde_json::Deserializer::from_str("1 2\n3")
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(many, [1, 2, 3]);
}

fn binary_blobs() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Plain {
        data: Vec<u8>,
    }
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Marked {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }
    let data = vec![0xFFu8; 1000];
    let (plain, marked) = (Plain { data: data.clone() }, Marked { data });
    let cbor = |v: &dyn Fn(&mut Vec<u8>)| {
        let mut out = vec![];
        v(&mut out);
        out
    };
    // serde treats Vec<u8> as "a sequence of numbers". A byte above 127 then needs TWO bytes in MessagePack and CBOR.
    let a = rmp_serde::to_vec_named(&plain).unwrap();
    let b = rmp_serde::to_vec_named(&marked).unwrap();
    assert!(
        a.len() > 2000 && b.len() < 1020,
        "msgpack {} vs {}",
        a.len(),
        b.len()
    );
    assert_eq!(rmp_serde::from_slice::<Marked>(&b).unwrap(), marked);
    let a = cbor(&|o| ciborium::into_writer(&plain, o).unwrap());
    let b = cbor(&|o| ciborium::into_writer(&marked, o).unwrap());
    assert!(
        a.len() > 2000 && b.len() < 1020,
        "cbor {} vs {}",
        a.len(),
        b.len()
    );
    assert_eq!(
        ciborium::from_reader::<Marked, _>(b.as_slice()).unwrap(),
        marked
    );
    // postcard writes u8 as one raw byte either way, so the encodings are identical.
    assert_eq!(
        postcard::to_allocvec(&plain).unwrap(),
        postcard::to_allocvec(&marked).unwrap()
    );
    // JSON has no byte-string type: serde_bytes does not help; use base64 text there.
    assert_eq!(
        serde_json::to_vec(&plain).unwrap(),
        serde_json::to_vec(&marked).unwrap()
    );
    // With serde_bytes a &[u8] field can borrow from the input: zero copy.
    #[derive(Deserialize)]
    struct Borrowed<'a> {
        #[serde(with = "serde_bytes")]
        data: &'a [u8],
    }
    let wire = rmp_serde::to_vec_named(&marked).unwrap();
    let view: Borrowed = rmp_serde::from_slice(&wire).unwrap();
    assert!(wire.as_ptr_range().contains(&view.data.as_ptr()) && view.data.len() == 1000);
}

fn yaml_documents_and_anchors() {
    // One file, several documents (Kubernetes manifests). from_str reads exactly one.
    let multi = "kind: A\n---\nkind: B\n";
    assert!(serde_yaml_ng::from_str::<BTreeMap<String, String>>(multi).is_err());
    let kinds: Vec<String> = serde_yaml_ng::Deserializer::from_str(multi)
        .map(|doc| {
            BTreeMap::<String, String>::deserialize(doc)
                .unwrap()
                .remove("kind")
                .unwrap()
        })
        .collect();
    assert_eq!(kinds, ["A", "B"]);
    // Anchors (&) and aliases (*) are expanded. Merge keys (<<) are NOT applied until you ask.
    let text = "base: &b\n  retries: 3\n  timeout: 5\njob:\n  <<: *b\n  timeout: 9\n";
    let mut v: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).unwrap();
    assert!(v["job"].get("retries").is_none() && v["job"].get("<<").is_some());
    v.apply_merge().unwrap();
    assert_eq!(
        (v["job"]["retries"].as_u64(), v["job"]["timeout"].as_u64()),
        (Some(3), Some(9)),
        "own keys win"
    );
    assert!(v["job"].get("<<").is_none());
}

fn postcard_framing() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Msg {
        seq: u32,
        body: Vec<u8>,
    }
    let msg = Msg {
        seq: 0,
        body: vec![0, 1, 0],
    };
    // No allocator (embedded): serialise into a caller-provided buffer.
    let mut buf = [0u8; 16];
    let used = postcard::to_slice(&msg, &mut buf).unwrap().len();
    assert_eq!(postcard::from_bytes::<Msg>(&buf[..used]).unwrap(), msg);
    assert_eq!(
        postcard::to_slice(&msg, &mut [0u8; 2]),
        Err(postcard::Error::SerializeBufferFull)
    );
    // COBS removes every zero from the payload and appends one as the delimiter, so a receiver that joins mid-stream can
    // resynchronise at the next 0x00.
    let mut stream = postcard::to_allocvec_cobs(&msg).unwrap();
    assert_eq!(stream.iter().filter(|&&b| b == 0).count(), 1);
    assert_eq!(stream.last(), Some(&0));
    stream.extend(
        postcard::to_allocvec_cobs(&Msg {
            seq: 1,
            body: vec![],
        })
        .unwrap(),
    );
    let mut garbled = stream[3..].to_vec(); // lost the start of the first frame
    let resync = garbled.iter().position(|&b| b == 0).unwrap() + 1;
    assert_eq!(
        postcard::from_bytes_cobs::<Msg>(&mut garbled[resync..])
            .unwrap()
            .seq,
        1
    );
    // Decoding happens in place, which is why it needs &mut.
    let (first, rest) = postcard::take_from_bytes_cobs::<Msg>(&mut stream).unwrap();
    assert_eq!(first, msg);
    assert_eq!(postcard::from_bytes_cobs::<Msg>(rest).unwrap().seq, 1);
}

fn csv_headers_and_ragged_rows() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Row {
        id: u32,
        name: String,
    }
    // With headers, fields are matched BY NAME: column order and extra columns do not matter.
    let swapped = "name,extra,id\nann,z,1\n";
    let rows: Vec<Row> = csv::Reader::from_reader(swapped.as_bytes())
        .deserialize()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        [Row {
            id: 1,
            name: "ann".into()
        }]
    );
    // PITFALL: a file WITHOUT a header row, read with the default settings, silently loses its first record.
    let headerless = "1,ann\n2,bob\n";
    let mut rd = csv::Reader::from_reader(headerless.as_bytes());
    assert_eq!(rd.records().count(), 1);
    let rows: Vec<Row> = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(headerless.as_bytes())
        .deserialize()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    let mut w = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    w.serialize(&rows[0]).unwrap();
    assert_eq!(w.into_inner().unwrap(), b"1,ann\n");
    // Rows of different lengths are an error unless the reader is flexible.
    let ragged = "a,b\n1,2\n3\n";
    let results: Vec<bool> = csv::Reader::from_reader(ragged.as_bytes())
        .records()
        .map(|r| r.is_ok())
        .collect();
    assert_eq!(results, [true, false]);
    let lens: Vec<usize> = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(ragged.as_bytes())
        .records()
        .map(|r| r.unwrap().len())
        .collect();
    assert_eq!(lens, [2, 1]);
    // SECURITY: CSV quoting is not spreadsheet safety. A cell starting with '=' is written unchanged, and a spreadsheet
    // may run it as a formula. Neutralise such cells yourself when exporting user data.
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(["=1+1"]).unwrap();
    assert_eq!(w.into_inner().unwrap(), b"=1+1\n");
}

pub fn main() {
    round_trips_and_sizes();
    json_gotchas();
    toml_gotchas();
    yaml_gotchas();
    non_self_describing_gotchas();
    self_describing_binary();
    csv_section();
    json_hostile_input();
    binary_blobs();
    yaml_documents_and_anchors();
    postcard_framing();
    csv_headers_and_ragged_rows();
    println!("serde formats: all sections passed");
}
