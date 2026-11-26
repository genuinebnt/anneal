/// Longest size line, without its CRLF.
pub const MAX_LINE: usize = 64;
/// Longest trailer section, with the empty line that ends it.
pub const MAX_TRAILER: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkError {
    Malformed,
    TooLarge,
    TrailerTooLong,
}

/// The line at the front of `rest`: `Ok(None)` if it has not ended, `Err` once it cannot end within `limit` bytes.
fn line(rest: &[u8], limit: usize) -> Result<Option<&[u8]>, ()> {
    match rest.windows(2).position(|w| w == b"\r\n") {
        Some(end) if end > limit => Err(()),
        Some(end) => Ok(Some(&rest[..end])),
        // a final `\r` may be the first half of the line's end
        None if rest.len() - usize::from(rest.ends_with(b"\r")) > limit => Err(()),
        None => Ok(None),
    }
}

fn size_of(line: &[u8]) -> Option<usize> {
    let digits = line.split(|&b| b == b';').next()?;
    if digits.is_empty() || digits.len() > 8 {
        return None;
    }
    digits.iter().try_fold(0usize, |n, &d| Some(n * 16 + (d as char).to_digit(16)? as usize))
}

pub fn decode(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ChunkError> {
    let mut body = Vec::new();
    let mut at = 0;
    loop {
        let Some(header) = line(&buf[at..], MAX_LINE).map_err(|()| ChunkError::Malformed)? else {
            return Ok(None);
        };
        let size = size_of(header).ok_or(ChunkError::Malformed)?;
        at += header.len() + 2;
        if size == 0 {
            return trailers(buf, at).map(|end| end.map(|end| (body, end)));
        }
        // compared this way round so that nothing can overflow
        let rest = &buf[at..];
        let Some(data) = rest.get(..size) else {
            return Ok(None);
        };
        let after = &rest[size..rest.len().min(size + 2)];
        if !b"\r\n".starts_with(after) {
            return Err(ChunkError::Malformed);
        }
        if after.len() < 2 {
            return Ok(None);
        }
        if size > max_body - body.len() {
            return Err(ChunkError::TooLarge);
        }
        body.extend_from_slice(data);
        at += size + 2;
    }
}

/// Skips the trailer lines starting at `start`; returns where the empty line ends.
fn trailers(buf: &[u8], start: usize) -> Result<Option<usize>, ChunkError> {
    let mut at = start;
    loop {
        // what is left of the section's allowance, keeping two bytes for this line's CRLF
        let Some(room) = (MAX_TRAILER - (at - start)).checked_sub(2) else {
            return Err(ChunkError::TrailerTooLong);
        };
        let Some(text) = line(&buf[at..], room).map_err(|()| ChunkError::TrailerTooLong)? else {
            return Ok(None);
        };
        at += text.len() + 2;
        if text.is_empty() {
            return Ok(Some(at));
        }
    }
}
