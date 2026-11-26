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
        Limits {
            max_request_line: 2048,
            max_header_bytes: 8192,
            max_headers: 64,
            max_body: 64 * 1024,
        }
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
        todo!()
    }
}

impl Request {
    /// The first header with this name, compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        todo!()
    }
}
/// Reads one request from the front of `buf`.
pub fn parse_request(buf: &[u8], limits: &Limits) -> Parsed {
    todo!()
}
