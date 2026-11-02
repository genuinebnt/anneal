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
        todo!()
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
    // TODO: the bytes received so far and the limits
}

impl Parser {
    pub fn new(limits: Limits) -> Self {
        todo!()
    }

    pub fn feed(&mut self, data: &[u8]) {
        todo!()
    }

    /// Bytes held that do not yet form a complete request.
    pub fn buffered(&self) -> usize {
        todo!()
    }

    /// The next complete request, `Ok(None)` if more bytes are needed (nothing is consumed), or the reason the stream is bad.
    pub fn next_request(&mut self) -> Result<Option<Request>, ParseError> {
        todo!()
    }
}
