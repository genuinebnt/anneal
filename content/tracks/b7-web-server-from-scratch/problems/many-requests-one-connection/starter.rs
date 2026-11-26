use std::io::{self, Read, Write};

// ------------------------------------------------------------------ given: use as it is

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    /// 0 for HTTP/1.0, 1 for HTTP/1.1.
    pub minor: u8,
    /// Names in lower case, in order.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// No whole request yet. `wants_continue`: the head is complete, it asked for `100-continue`, and the body is not all here.
    More {
        wants_continue: bool,
    },
    /// A request, and how many bytes of the buffer it took.
    Whole(Request, usize),
    Invalid,
}

/// The characters a header name may hold: an HTTP "token". Anything else could be read as a different
/// name by a proxy in front of the server, which is how requests are smuggled past it.
fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Reads one request from the front of `buf`. (A small reader: it does not enforce limits.)
pub fn next_request(buf: &[u8]) -> Next {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Next::More {
            wants_continue: false,
        };
    };
    let Ok(head) = std::str::from_utf8(&buf[..end]) else {
        return Next::Invalid;
    };
    let mut lines = head.split("\r\n");
    let parts: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let [method, path, version @ ("HTTP/1.1" | "HTTP/1.0")] = parts[..] else {
        return Next::Invalid;
    };
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Next::Invalid;
        };
        if name.is_empty() || !name.bytes().all(is_token_byte) {
            return Next::Invalid;
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    let value = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let Ok(length) = value("content-length").unwrap_or("0").parse::<usize>() else {
        return Next::Invalid;
    };
    let start = end + 4;
    if buf.len() - start < length {
        return Next::More {
            wants_continue: value("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")),
        };
    }
    let minor = u8::from(version == "HTTP/1.1");
    let body = buf[start..start + length].to_vec();
    Next::Whole(
        Request {
            method: method.to_string(),
            path: path.to_string(),
            minor,
            headers,
            body,
        },
        start + length,
    )
}

/// The bytes of a reply.
pub fn reply(status: u16, body: &str, keep_open: bool) -> Vec<u8> {
    let connection = if keep_open { "keep-alive" } else { "close" };
    format!(
        "HTTP/1.1 {status} .\r\nConnection: {connection}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

// ------------------------------------------------------------------ yours

pub fn serve<S: Read + Write>(
    stream: &mut S,
    max_requests: usize,
    handler: impl Fn(&Request) -> String,
) -> io::Result<usize> {
    todo!()
}
