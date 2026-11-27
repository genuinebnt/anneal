//! RECIPE 1205 (compilers): Source maps: from a place in generated code back to the file a person wrote.
//! LEVEL: advanced
//! Use in: error reports, debuggers, build tools.   Run: cargo run --release --example source_maps
//! COMMON USES: minified JavaScript and CSS, TypeScript and Sass output, crash report services.
#![allow(dead_code)]
//!
//! Reads and writes version 3 source maps: the base64 VLQ numbers, the `mappings` string and the lookup. A map from a
//! stranger is hostile input, so every number, index and size is checked. It is stricter than the standard in places
//! (it refuses where a browser may carry on), does not read index maps (`sections`), and never opens or downloads
//! anything a map names.
//! Read: https://tc39.es/ecma426/

use serde::Deserialize;

const MAX_BYTES: usize = 4 << 20;
/// Segments and lines together: each costs memory, and `;` or `A,` is only a byte or two.
const MAX_ENTRIES: usize = 500_000;
/// 32 bits need at most seven 5-bit digits.
const MAX_DIGITS: u32 = 7;
const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq)]
enum MapError {
    TooLarge,
    Json(String),
    Unsupported(&'static str),
    /// A broken part of `mappings`, with the generated line it belongs to (from 0).
    Mappings { line: usize, why: &'static str },
}

// ---- base64 VLQ ----

/// Writes one number: the sign goes in the lowest bit, then 5 bits per character, smallest first.
fn vlq_encode(n: i32, out: &mut String) {
    let magnitude = u64::from(n.unsigned_abs());
    // the format writes the smallest i32 as "minus zero"
    let mut rest = if n == i32::MIN { 1 } else { magnitude << 1 | u64::from(n < 0) };
    loop {
        let low = (rest & 31) as usize;
        rest >>= 5;
        // 32 in a digit says that more digits follow
        out.push(char::from(DIGITS[low | if rest > 0 { 32 } else { 0 }]));
        if rest == 0 {
            return;
        }
    }
}

fn digit(c: u8) -> Option<u64> {
    DIGITS.iter().position(|&d| d == c).map(|p| p as u64)
}

/// Reads one number from the front of `text` and moves `text` past it.
fn vlq_decode(text: &mut &[u8]) -> Result<i32, &'static str> {
    let mut value: u64 = 0;
    for position in 0..MAX_DIGITS {
        let (&c, rest) = text.split_first().ok_or("a number is cut off")?;
        let d = digit(c).ok_or("not a base64 digit")?;
        *text = rest;
        // at most 7 digits, so the shift stays below 35 and a u64 holds the result
        value |= (d & 31) << (5 * position);
        if d & 32 == 0 {
            let value = u32::try_from(value).map_err(|_| "a number needs more than 32 bits")?;
            let magnitude = (value >> 1) as i32;
            return Ok(match (value & 1, magnitude) {
                (0, m) => m,
                (_, 0) => i32::MIN,
                (_, m) => -m,
            });
        }
    }
    Err("a number has too many digits")
}

// ---- mappings ----

#[derive(Debug, Clone, Copy, PartialEq)]
struct Original {
    source: u32,
    /// Both counted from 0.
    line: u32,
    column: u32,
    name: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Segment {
    /// The column in the generated line, from 0, in UTF-16 code units.
    column: u32,
    /// `None`: from here on the generated text comes from no source.
    original: Option<Original>,
}

type Lines = Vec<Vec<Segment>>;

/// Adds a delta to a running total and checks the result.
fn step(total: &mut u32, delta: i32) -> Result<u32, &'static str> {
    *total = total.checked_add_signed(delta).ok_or("a position leaves the range 0 to 2^32-1")?;
    Ok(*total)
}

fn decode_mappings(text: &str, sources: usize, names: usize) -> Result<Lines, MapError> {
    // running totals: each number in the string is the change since the last one of its kind
    let (mut source, mut line, mut column, mut name) = (0u32, 0u32, 0u32, 0u32);
    let mut entries = 0;
    let mut lines = Vec::new();
    for (number, text) in text.split(';').enumerate() {
        let fail = |why| MapError::Mappings { line: number, why };
        let mut segments = Vec::new();
        entries += 1;
        // only the generated column starts again on each line
        let mut generated = 0u32;
        let parts = (!text.is_empty()).then(|| text.split(','));
        for part in parts.into_iter().flatten() {
            entries += 1;
            if entries > MAX_ENTRIES {
                return Err(MapError::TooLarge);
            }
            let mut bytes = part.as_bytes();
            let mut fields = [0i32; 5];
            let mut count = 0;
            while !bytes.is_empty() {
                if count == fields.len() {
                    return Err(fail("a segment has more than five numbers"));
                }
                fields[count] = vlq_decode(&mut bytes).map_err(fail)?;
                count += 1;
            }
            if !matches!(count, 1 | 4 | 5) {
                return Err(fail("a segment needs one, four or five numbers"));
            }
            let at = step(&mut generated, fields[0]).map_err(fail)?;
            let mut original = None;
            if count >= 4 {
                let mut found = Original {
                    source: step(&mut source, fields[1]).map_err(fail)?,
                    line: step(&mut line, fields[2]).map_err(fail)?,
                    column: step(&mut column, fields[3]).map_err(fail)?,
                    name: None,
                };
                if found.source as usize >= sources {
                    return Err(fail("there is no source with this index"));
                }
                if count == 5 {
                    let index = step(&mut name, fields[4]).map_err(fail)?;
                    if index as usize >= names {
                        return Err(fail("there is no name with this index"));
                    }
                    found.name = Some(index);
                }
                original = Some(found);
            }
            segments.push(Segment { column: at, original });
        }
        if entries > MAX_ENTRIES {
            return Err(MapError::TooLarge);
        }
        // a column delta may be negative, so sort before searching
        segments.sort_by_key(|s| s.column);
        lines.push(segments);
    }
    Ok(lines)
}

/// The reverse of `decode_mappings`. Every position must fit in an `i32`.
fn encode_mappings(lines: &[Vec<Segment>]) -> String {
    let mut out = String::new();
    let mut previous = [0i64; 4];
    for (number, line) in lines.iter().enumerate() {
        if number > 0 {
            out.push(';');
        }
        let mut generated = 0;
        for (i, segment) in line.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let delta = |now: u32, before: &mut i64, out: &mut String| {
                let change = i64::from(now) - *before;
                *before = i64::from(now);
                vlq_encode(i32::try_from(change).expect("positions fit in an i32"), out);
            };
            delta(segment.column, &mut generated, &mut out);
            let Some(o) = segment.original else { continue };
            for (slot, value) in [o.source, o.line, o.column].into_iter().enumerate() {
                delta(value, &mut previous[slot], &mut out);
            }
            if let Some(name) = o.name {
                delta(name, &mut previous[3], &mut out);
            }
        }
    }
    out
}

// ---- the map ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Json {
    version: u32,
    sources: Vec<Option<String>>,
    #[serde(default)]
    names: Vec<String>,
    mappings: String,
    source_root: Option<String>,
    #[serde(default)]
    sources_content: Option<Vec<Option<String>>>,
}

#[derive(Debug)]
struct SourceMap {
    /// Text chosen by whoever made the map. A label to show: not a path to open or an address to fetch.
    sources: Vec<Option<String>>,
    contents: Vec<Option<String>>,
    names: Vec<String>,
    lines: Lines,
}

#[derive(Debug, PartialEq)]
struct Found<'a> {
    source: Option<&'a str>,
    line: u32,
    column: u32,
    name: Option<&'a str>,
}

impl SourceMap {
    fn parse(text: &str) -> Result<Self, MapError> {
        if text.len() > MAX_BYTES {
            return Err(MapError::TooLarge);
        }
        // some servers put a line starting `)]}'` in front, to stop the file running as a script
        let text = match text.strip_prefix(")]}'") {
            Some(rest) => rest.split_once('\n').map_or("", |(_, json)| json),
            None => text,
        };
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| MapError::Json(e.to_string()))?;
        if value.get("sections").is_some() {
            return Err(MapError::Unsupported("an index map, with `sections`"));
        }
        let json = Json::deserialize(value).map_err(|e| MapError::Json(e.to_string()))?;
        if json.version != 3 {
            return Err(MapError::Unsupported("a version other than 3"));
        }
        let lines = decode_mappings(&json.mappings, json.sources.len(), json.names.len())?;
        // `sourceRoot` goes in front of every source; an empty one is treated as none
        let root = json.source_root.filter(|r| !r.is_empty()).map(|r| format!("{}/", r.strip_suffix('/').unwrap_or(&r)));
        let sources: Vec<_> = match &root {
            // a long root times many short sources would multiply into gigabytes
            Some(root) if root.len().saturating_mul(json.sources.len()) > MAX_BYTES => return Err(MapError::TooLarge),
            Some(root) => json.sources.into_iter().map(|s| s.map(|s| format!("{root}{s}"))).collect(),
            None => json.sources,
        };
        let contents = json.sources_content.unwrap_or_default();
        Ok(SourceMap { sources, contents, names: json.names, lines })
    }

    /// The original place of a generated position. Both numbers count from 0, the column in UTF-16 units.
    fn lookup(&self, line: u32, column: u32) -> Option<Found<'_>> {
        let segments = self.lines.get(line as usize)?;
        // the last segment that starts at or before the column, on this line only
        let after = segments.partition_point(|s| s.column <= column);
        let o = segments.get(after.checked_sub(1)?)?.original?;
        Some(Found {
            source: self.sources[o.source as usize].as_deref(),
            line: o.line,
            column: o.column,
            name: o.name.map(|n| self.names[n as usize].as_str()),
        })
    }
}

/// The byte where UTF-16 column `column` of `line` starts. `None` past the end or inside a character.
fn byte_of_column(line: &str, column: u32) -> Option<usize> {
    let mut units = 0;
    for (byte, c) in line.char_indices() {
        if units >= column {
            return (units == column).then_some(byte);
        }
        units += c.len_utf16() as u32;
    }
    (units == column).then_some(line.len())
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

// ---- a toy build step that records where its output came from ----

/// Joins each file into one output line: drops indentation, blank lines and `//` lines.
/// It shows the bookkeeping. It is not a safe minifier for real JavaScript.
fn bundle(files: &[(&str, &str)]) -> (String, String) {
    let mut output = String::new();
    let mut lines: Lines = Vec::new();
    for (source, (_, text)) in files.iter().enumerate() {
        let mut generated = String::new();
        let mut segments = Vec::new();
        for (line, full) in text.lines().enumerate() {
            let piece = full.trim_start();
            if piece.is_empty() || piece.starts_with("//") {
                continue;
            }
            if !generated.is_empty() {
                generated.push(' ');
            }
            let indent = &full[..full.len() - piece.len()];
            segments.push(Segment {
                column: utf16_len(&generated),
                original: Some(Original { source: source as u32, line: line as u32, column: utf16_len(indent), name: None }),
            });
            generated.push_str(piece);
        }
        output.push_str(&generated);
        output.push('\n');
        lines.push(segments);
    }
    let names: Vec<_> = files.iter().map(|f| f.0).collect();
    let map = serde_json::json!({ "version": 3, "sources": names, "names": [], "mappings": encode_mappings(&lines) });
    (output, map.to_string())
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// A second decoder written the slow, obvious way, with numbers too wide to overflow here.
fn naive_numbers(text: &str) -> Option<Vec<i64>> {
    let mut out = Vec::new();
    let mut digits: Vec<u64> = Vec::new();
    for c in text.bytes() {
        let d = DIGITS.iter().position(|&x| x == c)? as u64;
        digits.push(d % 32);
        if digits.len() > MAX_DIGITS as usize {
            return None;
        }
        if d < 32 {
            let value = digits.iter().rev().fold(0u128, |v, &d| v * 32 + u128::from(d));
            if value > u128::from(u32::MAX) {
                return None;
            }
            let half = (value / 2) as i64;
            out.push(match (value % 2, half) {
                (0, h) => h,
                (_, 0) => -(1i64 << 31),
                (_, h) => -h,
            });
            digits.clear();
        }
    }
    digits.is_empty().then_some(out)
}

fn all_numbers(text: &str) -> Option<Vec<i64>> {
    let mut bytes = text.as_bytes();
    let mut out = Vec::new();
    while !bytes.is_empty() {
        out.push(i64::from(vlq_decode(&mut bytes).ok()?));
    }
    Some(out)
}

fn number_cases() {
    // the first two are the examples in the standard
    for (text, n) in [("iB", 17), ("V", -10), ("A", 0), ("C", 1), ("D", -1), ("e", 15), ("gB", 16), ("B", i32::MIN), ("+/////D", i32::MAX), ("//////D", -i32::MAX), ("gA", 0)] {
        assert_eq!(all_numbers(text), Some(vec![i64::from(n)]), "{text}");
    }
    for n in [0, 1, -1, 15, 16, -16, 511, 512, i32::MAX, i32::MIN, -i32::MAX] {
        let mut text = String::new();
        vlq_encode(n, &mut text);
        assert_eq!(all_numbers(&text), Some(vec![i64::from(n)]), "{text}");
    }
    // 2^32, eight digits, a very long run, a number with no last digit, other characters
    for bad in ["ggggggE", "gggggggA", &"g".repeat(10_000), "g", "AAg", "=", "A A", "é"] {
        assert_eq!(all_numbers(bad), None, "{bad:.12}");
    }
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    for _ in 0..100_000 {
        let n = rng.below(u64::MAX) as u32 as i32 >> rng.below(32);
        let mut text = String::new();
        vlq_encode(n, &mut text);
        assert_eq!(all_numbers(&text), Some(vec![i64::from(n)]));
        let junk: String = (0..rng.below(12)).map(|_| char::from(b"ABDEghz+/9!,"[rng.below(12) as usize])).collect();
        assert_eq!(all_numbers(&junk), naive_numbers(&junk), "{junk}");
    }
}

fn mapping_cases() {
    let one = |m: &str| decode_mappings(m, 2, 1);
    let why = |m: &str| match one(m) {
        Err(MapError::Mappings { why, .. }) => why,
        other => panic!("{m}: {other:?}"),
    };
    assert_eq!(why("AA"), "a segment needs one, four or five numbers");
    assert_eq!(why("AAA"), "a segment needs one, four or five numbers");
    assert_eq!(why("AAAAAA"), "a segment has more than five numbers");
    assert_eq!(why("D"), "a position leaves the range 0 to 2^32-1");
    assert_eq!(why("AADA"), "a position leaves the range 0 to 2^32-1");
    assert_eq!(why("AEAA"), "there is no source with this index");
    assert_eq!(why("AAAAC"), "there is no name with this index");
    assert_eq!(why("AAAA,"), "a segment needs one, four or five numbers");
    assert_eq!(why("AAAg"), "a number is cut off");
    assert_eq!(one("AAAA;;AACA").unwrap().iter().map(Vec::len).collect::<Vec<_>>(), [1, 0, 1]);
    // the largest step, three times over, passes 2^32: refused, not wrapped to a small number
    assert_eq!(why("+/////D,+/////D,+/////D"), "a position leaves the range 0 to 2^32-1");
    assert_eq!(one(&";".repeat(MAX_ENTRIES)), Err(MapError::TooLarge));
    assert_eq!(one(&"A,".repeat(MAX_ENTRIES)), Err(MapError::TooLarge));
    assert!(one(&";".repeat(MAX_ENTRIES - 1)).is_ok());
    // columns 4 then 1: allowed, and sorted before the search
    let columns: Vec<_> = one("IAAA,HAAC").unwrap()[0].iter().map(|s| s.column).collect();
    assert_eq!(columns, [1, 4]);
}

fn map_cases() {
    let map = SourceMap::parse(
        ")]}'\n{\"version\":3,\"sourceRoot\":\"web\",\"sources\":[\"a.ts\",null],\"names\":[\"total\"],\
         \"mappings\":\"EAAAA,K;;ACCC\",\"x_extra\":1}",
    )
    .unwrap();
    assert_eq!(map.lookup(0, 1), None, "before the first segment");
    assert_eq!(map.lookup(0, 2), Some(Found { source: Some("web/a.ts"), line: 0, column: 0, name: Some("total") }));
    assert_eq!(map.lookup(0, 6), map.lookup(0, 2));
    assert_eq!(map.lookup(0, 7), None, "a one-number segment ends the mapped text");
    assert_eq!(map.lookup(1, 0), None, "an earlier line never covers this one");
    assert_eq!(map.lookup(2, 99).unwrap().source, None);
    assert_eq!(map.lookup(3, 0), None);
    assert_eq!(map.lookup(u32::MAX, u32::MAX), None);

    let with = |fields: &str| SourceMap::parse(&format!("{{{fields}}}")).map(|_| ());
    assert!(matches!(with(r#""version":3,"sections":[]"#), Err(MapError::Unsupported(_))));
    assert!(matches!(with(r#""version":2,"sources":[],"mappings":"""#), Err(MapError::Unsupported(_))));
    for broken in [r#""version":3,"mappings":"""#, r#""version":3,"sources":[1],"mappings":"""#, r#""version":-3,"sources":[],"mappings":"""#] {
        assert!(matches!(with(broken), Err(MapError::Json(_))), "{broken}");
    }
    assert!(matches!(SourceMap::parse(&"[".repeat(100_000)), Err(MapError::Json(_))));
    assert!(matches!(SourceMap::parse(")]}'"), Err(MapError::Json(_))));
    assert!(matches!(SourceMap::parse(&" ".repeat(MAX_BYTES + 1)), Err(MapError::TooLarge)));
    let many = format!(r#""version":3,"mappings":"","sourceRoot":"{}","sources":[{}null]"#, "r".repeat(100_000), "null,".repeat(100));
    assert_eq!(with(&many), Err(MapError::TooLarge));
}

/// Random tables: write them, read them back, and compare each lookup with a plain scan.
fn random_maps() {
    let mut rng = Rng(0xfeed_face_cafe_beef);
    for _ in 0..3_000 {
        let lines: Lines = (0..rng.below(5))
            .map(|_| {
                let mut line: Vec<Segment> = (0..rng.below(6))
                    .map(|_| {
                        let original = (rng.below(4) > 0).then(|| Original {
                            source: rng.below(3) as u32,
                            line: rng.below(2_000) as u32,
                            column: rng.below(100) as u32,
                            name: (rng.below(2) > 0).then(|| rng.below(4) as u32),
                        });
                        Segment { column: rng.below(40) as u32, original }
                    })
                    .collect();
                line.sort_by_key(|s| s.column);
                line
            })
            .collect();
        let text = encode_mappings(&lines);
        // splitting an empty string still gives one line
        let want = if lines.is_empty() { vec![Vec::new()] } else { lines.clone() };
        assert_eq!(decode_mappings(&text, 3, 4), Ok(want), "{text}");
        let map = SourceMap { sources: vec![None; 3], contents: Vec::new(), names: vec!["n".into(); 4], lines };
        for _ in 0..20 {
            let (line, column) = (rng.below(6) as u32, rng.below(45) as u32);
            let mut slow = None;
            for s in map.lines.get(line as usize).into_iter().flatten() {
                if s.column <= column {
                    slow = Some(s.original);
                }
            }
            let found = map.lookup(line, column).map(|f| (f.line, f.column));
            assert_eq!(found, slow.flatten().map(|o| (o.line, o.column)));
        }
        // cut short and damaged strings: an answer or an error, never a panic
        for cut in 0..=text.len() {
            let _ = decode_mappings(&text[..cut], 3, 4);
        }
    }
    for _ in 0..60_000 {
        let junk: String = (0..rng.below(30)).map(|_| ['A', 'C', 'D', 'E', 'g', '/', '+', '9', ',', ';', '!', 'é'][rng.below(12) as usize]).collect();
        let _ = decode_mappings(&junk, 2, 2);
    }
}

const CART: &str = "// prices are in cents\nfunction total(items) {\n    const label = \"🛒\"; return sum(items);\n}\n";
const MAIN: &str = "const cart = load();\n\n  show(total(cart));\n";

/// Every recorded piece must lead back to the same text.
fn bundle_leads_back() {
    let files = [("cart.js", CART), ("main.js", MAIN)];
    let (output, json) = bundle(&files);
    let map = SourceMap::parse(&json).unwrap();
    let mut checked = 0;
    for (number, generated) in output.lines().enumerate() {
        for segment in &map.lines[number] {
            let found = map.lookup(number as u32, segment.column).unwrap();
            let text = files.iter().find(|f| Some(f.0) == found.source).unwrap().1;
            let original = text.lines().nth(found.line as usize).unwrap();
            let from = &original[byte_of_column(original, found.column).unwrap()..];
            assert!(generated[byte_of_column(generated, segment.column).unwrap()..].starts_with(from));
            checked += 1;
        }
    }
    assert_eq!(checked, 5);
    assert_eq!(byte_of_column("a🔥b", 3), Some(5));
    assert_eq!(byte_of_column("a🔥b", 2), None);
    assert_eq!(byte_of_column("ab", 2), Some(2));
    assert_eq!(byte_of_column("ab", 3), None);
}

fn main() {
    number_cases();
    mapping_cases();
    map_cases();
    random_maps();
    bundle_leads_back();

    let (output, json) = bundle(&[("cart.js", CART), ("main.js", MAIN)]);
    print!("{output}");
    println!("{json}");
    let map = SourceMap::parse(&json).expect("a map made above");
    let first = output.lines().next().expect("one line per file");
    // an error report names a place in the output; here, where `sum(` starts
    let column = utf16_len(&first[..first.find("sum(").expect("in the bundle")]);
    let found = map.lookup(0, column).expect("mapped");
    println!("generated 1:{} is in {} at line {}", column + 1, found.source.unwrap_or("?"), found.line + 1);
    println!("all checks passed");
}
