// ============================================================ requests (steps 1 and 2)

/// Size limits a request must stay within.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Bytes in the request line, without its CRLF.
    pub max_request_line: usize,
    /// Bytes in the whole head: request line, headers and the blank line.
    pub max_header_bytes: usize,
    /// Number of header lines.
    pub max_headers: usize,
    /// Bytes in the (decoded) body.
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_request_line: 2048, max_header_bytes: 8192, max_headers: 64, max_body: 64 * 1024 }
    }
}

/// Why a request can never be served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    BadRequest,
    UriTooLong,
    HeaderTooLarge,
    BodyTooLarge,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The target up to the first `?`.
    pub path: String,
    /// What follows the first `?`, or empty.
    pub query: String,
    /// 0 for HTTP/1.0, 1 for HTTP/1.1.
    pub minor: u8,
    /// In arrival order, names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What the bytes received so far amount to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Not a whole request yet. `expect_continue`: the head is complete, it asked for `100-continue`, and the body is not all here.
    Incomplete { expect_continue: bool },
    /// One whole request, and how many bytes of the buffer it used.
    Done(Request, usize),
    /// No further bytes can make this a request the server accepts.
    Bad(HttpError),
}
impl HttpError {
    /// The response status for this error.
    pub fn status(self) -> u16 {
        match self {
            HttpError::BadRequest => 400,
            HttpError::UriTooLong => 414,
            HttpError::HeaderTooLarge => 431,
            HttpError::BodyTooLarge => 413,
        }
    }
}

impl Request {
    /// The first header with this name, compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
/// Reads one request from the front of `buf`.
pub fn parse_request(buf: &[u8], _limits: &Limits) -> Parsed {
    let bad = Parsed::Bad(HttpError::BadRequest);
    let incomplete = Parsed::Incomplete { expect_continue: false };
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return incomplete;
    };
    let head_len = head_end + 4;
    let Ok(head) = std::str::from_utf8(&buf[..head_end]) else {
        return bad;
    };
    let mut lines = head.split("\r\n");
    let parts: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let [method, target, version] = parts[..] else {
        return bad;
    };
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) || !target.starts_with('/') {
        return bad;
    }
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        _ => return bad,
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return bad;
        };
        if name.is_empty() || name.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return bad;
        }
        headers.push((name.to_ascii_lowercase(), value.trim_matches([' ', '\t']).to_string()));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), minor, headers, body: Vec::new() };
    if minor == 1 && req.header("host").is_none() {
        return bad;
    }
    let n = match req.header("content-length") {
        None => 0,
        Some(v) => match v.parse::<usize>() {
            Ok(n) => n,
            Err(_) => return bad,
        },
    };
    let Some(end) = head_len.checked_add(n) else {
        return bad;
    };
    if buf.len() < end {
        return incomplete;
    }
    req.body = buf[head_len..end].to_vec();
    Parsed::Done(req, buf.len())
}
