/// A parsed HTTP/1.x request. `minor` is 0 or 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub minor: u8,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first header with this name, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_head: usize,    // request line + headers + the blank line
    pub max_headers: usize, // number of header lines
    pub max_body: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BadRequestLine,
    BadVersion,
    BadHeader,
    HeadTooLarge,
    TooManyHeaders,
    BodyTooLarge,
    BadContentLength,
    BadChunk,
    ConflictingLength,
}

/// Incremental HTTP/1.1 request parser: feed bytes as they arrive, take requests as they complete.
pub struct Parser {
    buf: Vec<u8>,
    limits: Limits,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

impl Parser {
    pub fn new(limits: Limits) -> Self {
        Parser { buf: Vec::new(), limits }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Bytes held that do not yet form a complete request.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// The next complete request, `Ok(None)` if more bytes are needed (nothing is consumed), or the reason the stream is bad.
    pub fn next_request(&mut self) -> Result<Option<Request>, ParseError> {
        let Some(head_end) = find(&self.buf, b"\r\n\r\n") else {
            if self.buf.len() > self.limits.max_head {
                return Err(ParseError::HeadTooLarge); // never wait for a terminator that is not coming
            }
            return Ok(None);
        };
        let head_len = head_end + 4;
        if head_len > self.limits.max_head {
            return Err(ParseError::HeadTooLarge);
        }
        let head = std::str::from_utf8(&self.buf[..head_end]).map_err(|_| ParseError::BadHeader)?;
        let mut lines = head.split("\r\n");

        // request line: METHOD SP target SP HTTP/1.x
        let line = lines.next().unwrap_or("");
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() != 3 || parts[0].is_empty() || !parts[0].bytes().all(|b| b.is_ascii_uppercase()) || parts[1].is_empty() || parts[1].bytes().any(|b| b.is_ascii_control()) {
            return Err(ParseError::BadRequestLine);
        }
        let minor = match parts[2] {
            "HTTP/1.1" => 1,
            "HTTP/1.0" => 0,
            v if v.starts_with("HTTP/") => return Err(ParseError::BadVersion),
            _ => return Err(ParseError::BadRequestLine),
        };

        // headers
        let mut headers = Vec::new();
        for l in lines {
            let (name, value) = l.split_once(':').ok_or(ParseError::BadHeader)?;
            if name.is_empty() || !name.bytes().all(is_token_char) {
                return Err(ParseError::BadHeader); // includes a space before the colon and obsolete line folding
            }
            headers.push((name.to_string(), value.trim_matches(|c| c == ' ' || c == '\t').to_string()));
            if headers.len() > self.limits.max_headers {
                return Err(ParseError::TooManyHeaders);
            }
        }

        // body framing: Content-Length xor chunked
        let mut content_length: Option<usize> = None;
        let mut chunked = false;
        for (n, v) in &headers {
            if n.eq_ignore_ascii_case("content-length") {
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ParseError::BadContentLength);
                }
                let len: usize = v.parse().map_err(|_| ParseError::BadContentLength)?;
                match content_length {
                    Some(prev) if prev != len => return Err(ParseError::ConflictingLength),
                    _ => content_length = Some(len),
                }
            } else if n.eq_ignore_ascii_case("transfer-encoding") {
                if !v.eq_ignore_ascii_case("chunked") {
                    return Err(ParseError::BadHeader);
                }
                chunked = true;
            }
        }
        if chunked && content_length.is_some() {
            return Err(ParseError::ConflictingLength); // the request-smuggling ambiguity: refuse it
        }
        let (method, target) = (parts[0].to_string(), parts[1].to_string());
        let (body, consumed) = if chunked {
            match parse_chunked(&self.buf[head_len..], self.limits.max_body)? {
                Some((body, used)) => (body, head_len + used),
                None => return Ok(None),
            }
        } else if let Some(len) = content_length {
            if len > self.limits.max_body {
                return Err(ParseError::BodyTooLarge); // decided from the header, before the body arrives
            }
            if self.buf.len() < head_len + len {
                return Ok(None);
            }
            (self.buf[head_len..head_len + len].to_vec(), head_len + len)
        } else {
            (Vec::new(), head_len)
        };
        self.buf.drain(..consumed);
        Ok(Some(Request { method, target, minor, headers, body }))
    }
}

/// Decodes a chunked body from the start of `buf`: returns the body and how many bytes it used, or `None` if incomplete.
fn parse_chunked(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ParseError> {
    let mut body = Vec::new();
    let mut pos = 0;
    loop {
        let Some(eol) = find(&buf[pos..], b"\r\n") else {
            return if buf.len() - pos > 64 { Err(ParseError::BadChunk) } else { Ok(None) };
        };
        let line = std::str::from_utf8(&buf[pos..pos + eol]).map_err(|_| ParseError::BadChunk)?;
        let size_str = line.split(';').next().unwrap_or(""); // drop chunk extensions
        if size_str.is_empty() || !size_str.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseError::BadChunk);
        }
        let size = usize::from_str_radix(size_str, 16).map_err(|_| ParseError::BadChunk)?;
        pos += eol + 2;
        if size == 0 {
            // trailer lines until an empty line
            loop {
                let Some(eol) = find(&buf[pos..], b"\r\n") else { return Ok(None) };
                let empty = eol == 0;
                pos += eol + 2;
                if empty {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if body.len().checked_add(size).map_or(true, |t| t > max_body) {
            return Err(ParseError::BodyTooLarge); // from the announced size, before the data arrives
        }
        if buf.len() < pos + size + 2 {
            return Ok(None);
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        if &buf[pos + size..pos + size + 2] != b"\r\n" {
            return Err(ParseError::BadChunk);
        }
        pos += size + 2;
    }
}
