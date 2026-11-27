from y17_parts.common import broken

SOLUTION = r'''#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Value>),
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The most bytes a byte string may hold.
    pub max_bytes: usize,
    /// The most items one list may hold.
    pub max_items: usize,
    /// The most lists that may be open at once.
    pub max_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Malformed,
    TooLong,
    TooMany,
    TooDeep,
}

/// What one step of reading digits found.
struct Digits {
    value: u64,
    /// Where the byte after the digits is.
    end: usize,
}

/// Reads the digits at `buf[at..]`, up to `end_mark`. Every digit is judged as soon as it is seen.
fn digits(
    buf: &[u8],
    at: usize,
    end_mark: u8,
    mut judge: impl FnMut(u64) -> Result<(), Error>,
) -> Result<Option<Digits>, Error> {
    let mut value: u64 = 0;
    let mut i = at;
    loop {
        let Some(&byte) = buf.get(i) else {
            return Ok(None);
        };
        if byte == end_mark && i > at {
            return Ok(Some(Digits { value, end: i + 1 }));
        }
        if !byte.is_ascii_digit() || (i > at && value == 0) {
            return Err(Error::Malformed);
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(byte - b'0')))
            .ok_or(Error::Malformed)?;
        judge(value)?;
        i += 1;
    }
}

fn value_at(
    buf: &[u8],
    at: usize,
    open: usize,
    limits: &Limits,
) -> Result<Option<(Value, usize)>, Error> {
    let Some(&first) = buf.get(at) else {
        return Ok(None);
    };
    match first {
        b'i' => {
            let negative = buf.get(at + 1) == Some(&b'-');
            let start = at + 1 + usize::from(negative);
            let most = i64::MAX as u64 + u64::from(negative);
            let Some(n) = digits(buf, start, b'e', |v| {
                // "-0" can never become valid
                if v > most || (negative && v == 0) {
                    Err(Error::Malformed)
                } else {
                    Ok(())
                }
            })?
            else {
                return Ok(None);
            };
            let value = if negative {
                0i64.wrapping_sub_unsigned(n.value)
            } else {
                n.value as i64
            };
            Ok(Some((Value::Int(value), n.end)))
        }
        b'0'..=b'9' => {
            let Some(len) = digits(buf, at, b':', |v| {
                if v > limits.max_bytes as u64 {
                    Err(Error::TooLong)
                } else {
                    Ok(())
                }
            })?
            else {
                return Ok(None);
            };
            // compare with what is left, so that no sum can overflow
            if (buf.len() - len.end) < len.value as usize {
                return Ok(None);
            }
            let end = len.end + len.value as usize;
            Ok(Some((Value::Bytes(buf[len.end..end].to_vec()), end)))
        }
        b'l' => {
            if open >= limits.max_depth {
                return Err(Error::TooDeep);
            }
            let mut items = Vec::new();
            let mut at = at + 1;
            loop {
                match buf.get(at) {
                    None => return Ok(None),
                    Some(b'e') => return Ok(Some((Value::List(items), at + 1))),
                    Some(b'i' | b'l' | b'0'..=b'9') if items.len() >= limits.max_items => {
                        return Err(Error::TooMany);
                    }
                    Some(_) => {}
                }
                let Some((item, end)) = value_at(buf, at, open + 1, limits)? else {
                    return Ok(None);
                };
                items.push(item);
                at = end;
            }
        }
        _ => Err(Error::Malformed),
    }
}

/// The value at the front of `buf` and the bytes it takes, or `Ok(None)` while more bytes could complete it.
pub fn decode(buf: &[u8], limits: &Limits) -> Result<Option<(Value, usize)>, Error> {
    value_at(buf, 0, 0, limits)
}
'''

STARTER = SOLUTION[:SOLUTION.index("/// What one step")] + r'''/// The value at the front of `buf` and the bytes it takes, or `Ok(None)` while more bytes could complete it.
pub fn decode(buf: &[u8], limits: &Limits) -> Result<Option<(Value, usize)>, Error> {
    todo!()
}
'''

HELPERS = r'''
#[allow(dead_code)]
fn lim() -> Limits {
    Limits { max_bytes: 10, max_items: 3, max_depth: 3 }
}
#[allow(dead_code)]
fn read(input: &str) -> Result<Option<(Value, usize)>, Error> {
    decode(input.as_bytes(), &lim())
}
#[allow(dead_code)]
fn whole(input: &str) -> Result<Option<Value>, Error> {
    read(input).map(|r| r.map(|(v, used)| {
        assert_eq!(used, input.len(), "all of {input:?} belongs to the value");
        v
    }))
}
#[allow(dead_code)]
fn text(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}
/// `bad` must be refused with `error` exactly when its last byte arrives, and not a byte earlier.
#[allow(dead_code)]
fn refused_at_the_last_byte(bad: &str, error: Error) {
    for cut in 0..bad.len() {
        check!(format!("the first {cut} bytes of {bad:?} could still be valid"), decode(&bad.as_bytes()[..cut], &lim()), Ok(None));
    }
    check!(format!("{bad:?}"), read(bad), Err(error));
}
'''

VISIBLE = [
    HELPERS,
    r'''
#[test]
fn the_three_kinds() {
    check!("an integer", whole("i-42e"), Ok(Some(Value::Int(-42))));
    check!("bytes", whole("5:he:ll"), Ok(Some(text("he:ll"))));
    check!("no bytes", whole("0:"), Ok(Some(text(""))));
    check!("a list", whole("li1el2:abee"), Ok(Some(Value::List(vec![Value::Int(1), Value::List(vec![text("ab")])]))));
    check!("an empty list", whole("le"), Ok(Some(Value::List(vec![]))));
}

#[test]
fn only_the_first_value_is_taken() {
    check!("two values", read("i7ei8e"), Ok(Some((Value::Int(7), 3))));
}

#[test]
fn an_unfinished_value_needs_more() {
    let input = "li-5e3:abcl0:ee";
    for cut in 0..input.len() {
        check!(format!("{:?}", &input[..cut]), read(&input[..cut]), Ok(None));
    }
}

#[test]
fn malformed_input() {
    refused_at_the_last_byte("x", Error::Malformed);
    refused_at_the_last_byte("ie", Error::Malformed);
    refused_at_the_last_byte("i03", Error::Malformed);
    refused_at_the_last_byte("i-0", Error::Malformed);
}

#[test]
fn a_limit_is_reported_before_the_input_is_complete() {
    refused_at_the_last_byte("11", Error::TooLong);
    refused_at_the_last_byte("llll", Error::TooDeep);
    refused_at_the_last_byte("li1ei2ei3ei", Error::TooMany);
}
''',
]

HIDDEN = [
    HELPERS,
    r'''
#[test]
fn every_way_to_be_malformed() {
    for bad in [
        "e", " ", "-", ":", "ix", "i-e", "i--", "i+", "i1-", "i1x", "i ", "i00", "01", "00", "1x", "1-", "li1ex", "l:",
        "i9223372036854775808", "i-9223372036854775809", "i9999999999999999999",
    ] {
        refused_at_the_last_byte(bad, Error::Malformed);
    }
}

#[test]
fn the_edges_of_i64() {
    check!("max", whole("i9223372036854775807e"), Ok(Some(Value::Int(i64::MAX))));
    check!("min", whole("i-9223372036854775808e"), Ok(Some(Value::Int(i64::MIN))));
    check!("zero", whole("i0e"), Ok(Some(Value::Int(0))));
    check!("ten", whole("i10e"), Ok(Some(Value::Int(10))));
}

#[test]
fn limits_are_exact() {
    check!("10 bytes", whole("10:abcdefghij"), Ok(Some(text("abcdefghij"))));
    refused_at_the_last_byte("11", Error::TooLong);
    refused_at_the_last_byte("20", Error::TooLong);
    refused_at_the_last_byte("l10:abcdefghij3:abc99", Error::TooLong);
    check!("3 items", whole("li1ei2ei3ee").map(|v| v.is_some()), Ok(true));
    refused_at_the_last_byte("li1ei2ei3el", Error::TooMany);
    refused_at_the_last_byte("li1ei2ei3e0", Error::TooMany);
    check!("three lists open", whole("llleee").map(|v| v.is_some()), Ok(true));
    refused_at_the_last_byte("llll", Error::TooDeep);
    check!("depth is about open lists, not about their number", whole("llelelee").map(|v| v.is_some()), Ok(true));
    check!("the item limit is per list", whole("lli1ei2ei3eeli1ei2ei3eee").map(|v| v.is_some()), Ok(true));
}

#[test]
fn a_bad_byte_after_a_full_list_is_malformed() {
    refused_at_the_last_byte("li1ei2ei3ex", Error::Malformed);
}

#[test]
fn a_limit_of_zero() {
    let none = Limits { max_bytes: 0, max_items: 0, max_depth: 0 };
    check!("no lists", decode(b"l", &none), Err(Error::TooDeep));
    check!("only empty bytes", decode(b"1", &none), Err(Error::TooLong));
    check!("empty bytes", decode(b"0:", &none), Ok(Some((text(""), 2))));
    check!("an integer is not limited", decode(b"i5e", &none), Ok(Some((Value::Int(5), 3))));
    let empty_lists = Limits { max_depth: 1, ..none };
    check!("an empty list", decode(b"le", &empty_lists), Ok(Some((Value::List(vec![]), 2))));
    check!("no items", decode(b"li", &empty_lists), Err(Error::TooMany));
}

#[test]
fn huge_declared_sizes_are_harmless() {
    let huge = Limits { max_bytes: usize::MAX, max_items: usize::MAX, max_depth: 3 };
    check!("a vast string", decode(b"18446744073709551615:abc", &huge), Ok(None));
    check!("a vast string in a list", decode(b"l9223372036854775807:", &huge), Ok(None));
    check!("past u64", decode(b"18446744073709551616", &huge), Err(Error::Malformed));
}

#[test]
fn very_deep_input_does_not_overflow_the_stack() {
    let deep = vec![b'l'; 1_000_000];
    check!("a million", decode(&deep, &lim()), Err(Error::TooDeep));
}

#[allow(dead_code)]
fn random_value(rng: &mut anneal_prelude::Rng, depth: usize) -> Value {
    match rng.below(if depth < 3 { 3 } else { 2 }) {
        0 => Value::Int(*rng.pick(&[i64::MIN, -100, -1, 0, 1, 10, i64::MAX])),
        1 => {
            let len = rng.below(11);
            Value::Bytes(rng.vec::<i64>(len, 0, 255).into_iter().map(|b| b as u8).collect())
        }
        _ => {
            let len = rng.below(4);
            Value::List((0..len).map(|_| random_value(rng, depth + 1)).collect())
        }
    }
}
#[allow(dead_code)]
fn write(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Int(n) => out.extend(format!("i{n}e").into_bytes()),
        Value::Bytes(b) => {
            out.extend(format!("{}:", b.len()).into_bytes());
            out.extend(b);
        }
        Value::List(items) => {
            out.push(b'l');
            items.iter().for_each(|i| write(i, out));
            out.push(b'e');
        }
    }
}

#[test]
fn random_values_come_back_and_every_prefix_needs_more() {
    let mut rng = anneal_prelude::Rng::new(0x3101);
    for _ in 0..500 {
        let value = random_value(&mut rng, 0);
        let mut bytes = Vec::new();
        write(&value, &mut bytes);
        let len = bytes.len();
        for cut in 0..len {
            check!(format!("the first {cut} bytes of {value:?}"), decode(&bytes[..cut], &lim()), Ok(None));
        }
        bytes.extend(b"i1e");
        check!(format!("{value:?}"), decode(&bytes, &lim()), Ok(Some((value.clone(), len))));
    }
}

#[test]
fn random_damage_never_panics_and_an_error_stays_an_error() {
    let mut rng = anneal_prelude::Rng::new(0x3102);
    for _ in 0..3000 {
        let len = rng.below(24);
        let bytes: Vec<u8> = (0..len).map(|_| *rng.pick(b"li:e-0123456789x")).collect();
        let mut failed = None;
        for cut in 0..=len {
            let result = decode(&bytes[..cut], &lim());
            if let Some(first) = failed {
                check!(format!("{:?} after it was refused", String::from_utf8_lossy(&bytes[..cut])), result, Err(first));
            } else if let Err(e) = result {
                failed = Some(e);
            }
        }
    }
}
''',
]

STATEMENT = r'''
Programs often send **nested** data over a connection: a list of strings and numbers, with lists inside. The bytes arrive in pieces, from a sender you do not trust.

The format has three kinds of value:

| Bytes | Value |
|---|---|
| `i-42e` | an integer: `i`, an optional `-`, digits, `e` |
| `5:hello` | bytes: a length, `:`, then that many bytes of any kind |
| `l` items `e` | a list of values |

Digits are ASCII. A number has no leading zeros (`0` itself is fine), and there is no `-0`. An integer must fit in an `i64`. A length must fit in a `u64`.

**`decode(buf, limits)`** looks at the value at the front of `buf`.

- `Ok(Some((value, used)))`: a whole value takes the first `used` bytes. Later bytes are left alone.
- `Ok(None)`: more bytes could still complete a valid value.
- `Err(..)`: **no bytes added later could make it valid** within the limits. Report this as soon as it becomes true, at the exact byte that makes it so.

| Error | When |
|---|---|
| `TooLong` | a byte string's length is over `max_bytes`, as soon as its digits say so |
| `TooDeep` | an `l` would make more than `max_depth` lists open at once |
| `TooMany` | a list that holds `max_items` items sees the first byte of another (`i`, `l` or a digit) |
| `Malformed` | anything else that cannot be valid |

A length that goes over `max_bytes` is `TooLong` even before its `:`. A length too large for `u64` is `Malformed`, which can happen only when `max_bytes` is that large.

The input may declare enormous sizes, with limits as high as `usize::MAX`. Do not allocate for what is only declared, and do not overflow. Deeply nested input must not crash the program.
'''

PROBLEM = dict(
    slug="a-value-inside-a-value", title="A value inside a value, arriving in pieces", level="medium", stage="messages",
    tags=["recursive descent", "incremental parsing", "untrusted input", "limits"],
    statement=STATEMENT, starter=STARTER, solution=SOLUTION,
    visible=VISIBLE, hidden=HIDDEN,
    teaches=[
        "A recursive parser for a stream passes \"need more\" up through every level.",
        "\"Never\" can be known long before the end; refuse at the first byte that proves it.",
        "Recursion on untrusted input needs a depth limit checked before going deeper.",
    ],
    follow_up="1) This format is a subset of bencode, which BitTorrent uses. It adds dictionaries with sorted keys: what must the decoder check? 2) Each call parses from the start. Design a decoder that keeps its place between calls. What does it have to store for open lists? 3) Return values that borrow from the buffer. What changes for the caller?",
    hints=[
        ("approach", "Write one function that reads a value starting at a position, with the number of lists already open. It returns the same three outcomes as `decode`, and lists call it for each item."),
        ("rust", "`let Some(x) = inner()? else { return Ok(None) };` passes both other outcomes upwards. A helper that reads digits can take a closure that judges the number after each digit."),
        ("edge", "Is `i-0` already hopeless? Is `1` when `max_bytes` is 0? Compare a declared length with the bytes you have left, not by adding it to a position."),
    ],
    notes=("One function parses a value at a position and returns `None` as soon as bytes run out. Numbers are judged digit by digit, so a limit or an impossible number is reported at once. The depth check comes before the recursive call, which also bounds the stack.",
           "O(bytes) per call", "O(depth) stack plus the value"),
    wrong={
        "limit_checked_after_the_colon": broken(SOLUTION, (
            "                if v > limits.max_bytes as u64 {\n                    Err(Error::TooLong)\n                } else {\n                    Ok(())\n                }\n            })?\n            else {\n                return Ok(None);\n            };\n",
            "                let _ = v;\n                Ok(())\n            })?\n            else {\n                return Ok(None);\n            };\n            if len.value > limits.max_bytes as u64 {\n                return Err(Error::TooLong);\n            }\n")),
        "leading_zeros_allowed": broken(SOLUTION, ("        if !byte.is_ascii_digit() || (i > at && value == 0) {", "        if !byte.is_ascii_digit() {")),
        "minus_zero_waits_for_the_end": broken(SOLUTION, (
            "                if v > most || (negative && v == 0) {", "                if v > most {"), (
            "            let value = if negative {", "            if negative && n.value == 0 {\n                return Err(Error::Malformed);\n            }\n            let value = if negative {")),
        "too_many_found_after_the_item": broken(SOLUTION, (
            "                    Some(b'i' | b'l' | b'0'..=b'9') if items.len() >= limits.max_items => {\n                        return Err(Error::TooMany);\n                    }\n", ""),
            ("                items.push(item);\n", "                items.push(item);\n                if items.len() > limits.max_items {\n                    return Err(Error::TooMany);\n                }\n")),
        "no_depth_limit_on_the_way_down": broken(SOLUTION, (
            "            if open >= limits.max_depth {\n                return Err(Error::TooDeep);\n            }\n", "")),
        "end_computed_by_adding": broken(SOLUTION, (
            "            if (buf.len() - len.end) < len.value as usize {\n                return Ok(None);\n            }",
            "            if buf.len() < len.end + len.value as usize {\n                return Ok(None);\n            }")),
    },
)
