#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub verb: String,
    pub name: String,
    /// In order, names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    NeedMore,
    Whole(Message, usize),
    Invalid,
}

/// The stream cannot be read any further. `before` holds the requests that came before the bad one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub before: Vec<Message>,
}

const BLANK_LINE: &[u8] = b"\r\n\r\n";

/// The parsed head and the body length it announces.
fn parse_head(head: &str) -> Option<(Message, usize)> {
    let mut lines = head.split("\r\n");
    let parts: Vec<&str> = lines.next()?.split(' ').collect();
    let [verb, name, "MSG/1"] = parts[..] else {
        return None;
    };
    if verb.is_empty() || name.is_empty() {
        return None;
    }
    let mut headers = Vec::new();
    let mut length = None;
    for line in lines {
        let (key, value) = line.split_once(':')?;
        if key.is_empty() {
            return None;
        }
        let key = key.to_ascii_lowercase();
        let value = value.trim_matches([' ', '\t']);
        if key == "content-length" {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            length = Some(value.parse::<usize>().ok()?);
        }
        headers.push((key, value.to_string()));
    }
    let message = Message {
        verb: verb.to_string(),
        name: name.to_string(),
        headers,
        body: Vec::new(),
    };
    Some((message, length.unwrap_or(0)))
}

pub fn read_one(buf: &[u8]) -> Read {
    let Some(head_end) = buf.windows(BLANK_LINE.len()).position(|w| w == BLANK_LINE) else {
        return Read::NeedMore;
    };
    let body_start = head_end + BLANK_LINE.len();
    let head = std::str::from_utf8(&buf[..head_end]).ok();
    let Some((mut message, length)) = head.and_then(parse_head) else {
        return Read::Invalid;
    };
    // `get` answers None while the body is still arriving
    let end = body_start.saturating_add(length);
    let Some(body) = buf.get(body_start..end) else {
        return Read::NeedMore;
    };
    message.body = body.to_vec();
    Read::Whole(message, end)
}

pub fn take_all(buf: &mut Vec<u8>) -> Result<Vec<Message>, Invalid> {
    let mut messages = Vec::new();
    let mut used = 0;
    let outcome = loop {
        match read_one(&buf[used..]) {
            Read::Whole(message, n) => {
                messages.push(message);
                used += n;
            }
            Read::NeedMore => break Ok(()),
            Read::Invalid => break Err(()),
        }
    };
    // removed once, so many small requests do not move the rest of the buffer again and again
    buf.drain(..used);
    match outcome {
        Ok(()) => Ok(messages),
        Err(()) => Err(Invalid { before: messages }),
    }
}
