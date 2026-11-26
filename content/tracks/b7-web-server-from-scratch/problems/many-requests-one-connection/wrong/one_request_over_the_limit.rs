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
    More { wants_continue: bool },
    /// A request, and how many bytes of the buffer it took.
    Whole(Request, usize),
    Invalid,
}

/// Reads one request from the front of `buf`. (A small reader: it does not enforce limits.)
pub fn next_request(buf: &[u8]) -> Next {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Next::More { wants_continue: false };
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
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    let value = |name: &str| headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
    let Ok(length) = value("content-length").unwrap_or("0").parse::<usize>() else {
        return Next::Invalid;
    };
    let start = end + 4;
    if buf.len() - start < length {
        return Next::More { wants_continue: value("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) };
    }
    let minor = u8::from(version == "HTTP/1.1");
    let body = buf[start..start + length].to_vec();
    Next::Whole(Request { method: method.to_string(), path: path.to_string(), minor, headers, body }, start + length)
}

/// The bytes of a reply.
pub fn reply(status: u16, body: &str, keep_open: bool) -> Vec<u8> {
    let connection = if keep_open { "keep-alive" } else { "close" };
    format!("HTTP/1.1 {status} .\r\nConnection: {connection}\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

// ------------------------------------------------------------------ yours

/// Whether the connection goes on after this request, as far as the request itself is concerned.
fn asks_to_stay(request: &Request) -> bool {
    let lists = |token: &str| {
        request.headers.iter().filter(|(name, _)| name == "connection").flat_map(|(_, v)| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token))
    };
    !lists("close") && (request.minor == 1 || lists("keep-alive"))
}

fn send<S: Write>(stream: &mut S, bytes: &[u8]) -> io::Result<()> {
    stream.write_all(bytes)?;
    stream.flush()
}

pub fn serve<S: Read + Write>(stream: &mut S, max_requests: usize, handler: impl Fn(&Request) -> String) -> io::Result<usize> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let mut served = 0;
    let mut permitted = false;
    loop {
        // first use what is already here; reading comes only when it is not enough
        match next_request(&buf) {
            Next::Whole(request, used) => {
                buf.drain(..used);
                served += 1;
                permitted = false;
                let keep_open = asks_to_stay(&request) && served <= max_requests;
                send(stream, &reply(200, &handler(&request), keep_open))?;
                if !keep_open {
                    return Ok(served);
                }
                continue;
            }
            Next::Invalid => {
                send(stream, &reply(400, "bad request", false))?;
                return Ok(served);
            }
            Next::More { wants_continue } => {
                if wants_continue && !permitted {
                    permitted = true;
                    send(stream, b"HTTP/1.1 100 Continue\r\n\r\n")?;
                }
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(served),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}
