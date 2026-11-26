#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_line: usize,
    pub max_head: usize,
    pub max_headers: usize,
    pub max_body: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    BadRequest,
    LineTooLong,
    HeadTooLarge,
    BodyTooLarge,
}

impl Refusal {
    pub fn status(self) -> u16 {
        match self {
            Refusal::BadRequest => 400,
            Refusal::LineTooLong => 414,
            Refusal::HeadTooLarge => 431,
            Refusal::BodyTooLarge => 413,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub head_len: usize,
    pub body_len: usize,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn check_head(buf: &[u8], limits: &Limits) -> Result<Option<Head>, Refusal> {
    match find(buf, b"\r\n") {
        Some(end) if end > limits.max_line => return Err(Refusal::LineTooLong),
        // a final `\r` may be the first half of the line's end, so it does not count yet
        _ => {}
    }
    let Some(end) = find(buf, b"\r\n\r\n") else {
        return Ok(None);
    };
    let head_len = end + 4;
    if head_len > limits.max_head {
        return Err(Refusal::HeadTooLarge);
    }
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| Refusal::BadRequest)?;
    let mut lines = text.split("\r\n");
    let parts: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let [method, target, version] = parts[..] else {
        return Err(Refusal::BadRequest);
    };
    let known = version == "HTTP/1.1" || version == "HTTP/1.0";
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) || !target.starts_with('/') || !known {
        return Err(Refusal::BadRequest);
    }
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut length: Option<usize> = None;
    for line in lines {
        if headers.len() == limits.max_headers {
            return Err(Refusal::HeadTooLarge);
        }
        if line.starts_with([' ', '\t']) {
            return Err(Refusal::BadRequest);
        }
        let (name, value) = line.split_once(':').ok_or(Refusal::BadRequest)?;
        if name.is_empty() || name.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Err(Refusal::BadRequest);
        }
        let name = name.to_ascii_lowercase();
        let value = value.trim_matches([' ', '\t']);
        if name == "transfer-encoding" {
            return Err(Refusal::BadRequest);
        }
        if name == "content-length" {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Refusal::BadRequest);
            }
            // only digits are left, so the one way to fail is a number too big to hold
            let n = value.parse().unwrap_or(usize::MAX);
            if length.is_some_and(|earlier| earlier != n) {
                return Err(Refusal::BadRequest);
            }
            length = Some(n);
        }
        headers.push((name, value.to_string()));
    }
    if version == "HTTP/1.1" && !headers.iter().any(|(name, _)| name == "host") {
        return Err(Refusal::BadRequest);
    }
    let body_len = length.unwrap_or(0);
    if body_len > limits.max_body {
        return Err(Refusal::BodyTooLarge);
    }
    Ok(Some(Head { method: method.to_string(), target: target.to_string(), headers, head_len, body_len }))
}
