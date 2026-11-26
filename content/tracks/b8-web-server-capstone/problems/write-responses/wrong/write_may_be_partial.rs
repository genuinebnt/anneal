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
/// A chunk-size line (size plus any `;extension`), without its CRLF.
const MAX_CHUNK_LINE: usize = 64;

/// Length of a line that has not ended yet; a trailing `\r` may be the first half of its CRLF.
fn open_line_len(tail: &[u8]) -> usize {
    tail.len() - usize::from(tail.last() == Some(&b'\r'))
}

/// Reads one request from the front of `buf`. Pure: call it again whenever more bytes have arrived.
pub fn parse_request(buf: &[u8], limits: &Limits) -> Parsed {
    use HttpError::*;
    let incomplete = Parsed::Incomplete { expect_continue: false };
    match find(buf, b"\r\n") {
        Some(line_end) if line_end > limits.max_request_line => return Parsed::Bad(UriTooLong),
        None if open_line_len(buf) > limits.max_request_line => return Parsed::Bad(UriTooLong),
        _ => {}
    }
    let Some(head_end) = find(buf, b"\r\n\r\n") else {
        return if buf.len() > limits.max_header_bytes { Parsed::Bad(HeaderTooLarge) } else { incomplete };
    };
    let head_len = head_end + 4;
    if head_len > limits.max_header_bytes {
        return Parsed::Bad(HeaderTooLarge);
    }
    let Ok(head) = std::str::from_utf8(&buf[..head_end]) else {
        return Parsed::Bad(BadRequest);
    };
    let mut lines = head.split("\r\n");
    let parts: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let [method, target, version] = parts[..] else {
        return Parsed::Bad(BadRequest);
    };
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) || !target.starts_with('/') {
        return Parsed::Bad(BadRequest);
    }
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        _ => return Parsed::Bad(BadRequest),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if headers.len() >= limits.max_headers {
            return Parsed::Bad(HeaderTooLarge);
        }
        if line.starts_with([' ', '\t']) {
            return Parsed::Bad(BadRequest);
        }
        let Some((name, value)) = line.split_once(':') else {
            return Parsed::Bad(BadRequest);
        };
        if name.is_empty() || name.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Parsed::Bad(BadRequest);
        }
        headers.push((name.to_ascii_lowercase(), value.trim_matches([' ', '\t']).to_string()));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), minor, headers, body: Vec::new() };
    if minor == 1 && req.header("host").is_none() {
        return Parsed::Bad(BadRequest);
    }
    let mut length: Option<usize> = None;
    let mut encodings = 0;
    let mut chunked = false;
    for (name, value) in &req.headers {
        if name == "content-length" {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Parsed::Bad(BadRequest);
            }
            // all digits, so the only way to fail is a number too large to hold
            let n = value.parse::<usize>().unwrap_or(usize::MAX);
            if length.is_some_and(|earlier| earlier != n) {
                return Parsed::Bad(BadRequest);
            }
            length = Some(n);
        } else if name == "transfer-encoding" {
            encodings += 1;
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }
    if encodings > 0 && (encodings > 1 || !chunked || length.is_some() || minor == 0) {
        return Parsed::Bad(BadRequest);
    }
    let expects = req.header("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue"));
    let waiting = Parsed::Incomplete { expect_continue: expects };
    if chunked {
        return match decode_chunks(&buf[head_len..], limits) {
            Err(e) => Parsed::Bad(e),
            Ok(None) => waiting,
            Ok(Some((body, used))) => {
                req.body = body;
                Parsed::Done(req, head_len + used)
            }
        };
    }
    let n = length.unwrap_or(0);
    if n > limits.max_body {
        return Parsed::Bad(BodyTooLarge);
    }
    if buf.len() < head_len + n {
        return waiting;
    }
    req.body = buf[head_len..head_len + n].to_vec();
    Parsed::Done(req, head_len + n)
}

/// A decoded body and the encoded bytes it used.
type Decoded = Option<(Vec<u8>, usize)>;

/// Decodes a chunked body from the front of `buf`. `Ok(None)`: more bytes are needed.
fn decode_chunks(buf: &[u8], limits: &Limits) -> Result<Decoded, HttpError> {
    let mut body = Vec::new();
    let mut pos = 0;
    loop {
        let rest = &buf[pos..];
        let Some(end) = find(rest, b"\r\n") else {
            return if open_line_len(rest) > MAX_CHUNK_LINE { Err(HttpError::BadRequest) } else { Ok(None) };
        };
        if end > MAX_CHUNK_LINE {
            return Err(HttpError::BadRequest);
        }
        let digits = rest[..end].split(|&b| b == b';').next().unwrap_or_default();
        if digits.is_empty() || digits.len() > 8 || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(HttpError::BadRequest);
        }
        let size = digits.iter().fold(0usize, |n, &d| n * 16 + (d as char).to_digit(16).unwrap_or(0) as usize);
        pos += end + 2;
        if size == 0 {
            let trailers = pos;
            loop {
                let rest = &buf[pos..];
                let Some(end) = find(rest, b"\r\n") else {
                    return if buf.len() - trailers > limits.max_header_bytes { Err(HttpError::HeaderTooLarge) } else { Ok(None) };
                };
                pos += end + 2;
                if pos - trailers > limits.max_header_bytes {
                    return Err(HttpError::HeaderTooLarge);
                }
                if end == 0 {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if size > limits.max_body - body.len() {
            return Err(HttpError::BodyTooLarge);
        }
        let have = &buf[pos..];
        if have.len() < size + 2 {
            // the CR may already be here and wrong
            return if have.len() > size && have[size] != b'\r' { Err(HttpError::BadRequest) } else { Ok(None) };
        }
        if &have[size..size + 2] != b"\r\n" {
            return Err(HttpError::BadRequest);
        }
        body.extend_from_slice(&have[..size]);
        pos += size + 2;
    }
}
// ============================================================ responses (step 3)

use std::io::{self, Write};

pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    /// Produced piece by piece while it is being sent.
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

/// The reason phrase for the status codes this server sends.
pub fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        204 => "No Content",
        206 => "Partial Content",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Content Too Large",
        414 => "URI Too Long",
        416 => "Range Not Satisfiable",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}
impl Response {
    /// A response with no headers and an empty body.
    pub fn new(status: u16) -> Response {
        Response { status, headers: Vec::new(), body: Body::Empty }
    }

    /// A plain-text response.
    pub fn text(status: u16, text: impl Into<String>) -> Response {
        Response::new(status).header("Content-Type", "text/plain; charset=utf-8").bytes(text.into().into_bytes())
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn bytes(mut self, body: Vec<u8>) -> Response {
        self.body = Body::Bytes(body);
        self
    }

    pub fn stream(mut self, pieces: impl Iterator<Item = Vec<u8>> + Send + 'static) -> Response {
        self.body = Body::Stream(Box::new(pieces));
        self
    }

    /// The first header with this name, compared without regard to case.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Sends `response`. Returns the number of bytes written.
pub fn write_response<W: Write>(out: &mut W, response: Response, head_only: bool, keep_alive: bool) -> io::Result<u64> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status)).into_bytes();
    for (name, value) in &response.headers {
        write!(head, "{name}: {value}\r\n")?;
    }
    write!(head, "Connection: {}\r\n", if keep_alive { "keep-alive" } else { "close" })?;
    let bodyless = response.status / 100 == 1 || response.status == 204 || response.status == 304;
    let bytes = match response.body {
        Body::Stream(pieces) if !bodyless => {
            head.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
            out.write_all(&head)?;
            let mut total = head.len() as u64;
            if !head_only {
                for piece in pieces {
                    if piece.is_empty() {
                        continue; // an empty chunk would end the body
                    }
                    let mut frame = format!("{:x}\r\n", piece.len()).into_bytes();
                    frame.extend_from_slice(&piece);
                    frame.extend_from_slice(b"\r\n");
                    out.write_all(&frame)?;
                    out.flush()?;
                    total += frame.len() as u64;
                }
                out.write_all(b"0\r\n\r\n")?;
                total += 5;
            }
            out.flush()?;
            return Ok(total);
        }
        Body::Bytes(bytes) => bytes,
        _ => Vec::new(),
    };
    if !bodyless {
        write!(head, "Content-Length: {}\r\n", bytes.len())?;
    }
    head.extend_from_slice(b"\r\n");
    if !head_only && !bodyless {
        head.extend_from_slice(&bytes);
    }
    let n = out.write(&head)?;
    out.flush()?;
    Ok(n as u64)
}

/// The response for a request that could not be accepted.
pub fn error_response(error: HttpError) -> Response {
    let status = error.status();
    Response::text(status, format!("{status} {}\n", reason(status)))
}
