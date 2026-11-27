#[derive(Debug, Clone, PartialEq)]
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
