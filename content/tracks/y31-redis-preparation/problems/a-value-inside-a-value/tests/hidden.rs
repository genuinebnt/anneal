use solution::*;


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
