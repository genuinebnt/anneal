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
