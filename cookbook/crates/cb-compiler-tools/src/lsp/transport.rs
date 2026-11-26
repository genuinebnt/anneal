//! LSP base protocol framing: `Content-Length: N\r\n\r\n` followed by N bytes of JSON.

use serde_json::Value;
use std::io::{self, BufRead, Write};

/// Refuse absurd frames instead of allocating them (a corrupt header must not OOM us).
const MAX_BODY: usize = 256 * 1024 * 1024;

/// Read one frame body. `Ok(None)` is a clean end of input.
pub fn read_message<R: BufRead>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return if saw_header {
                Err(io::ErrorKind::UnexpectedEof.into())
            } else {
                Ok(None)
            };
        }
        saw_header = true;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        // Header names are case-insensitive; unknown headers (Content-Type) are ignored.
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            len =
                Some(value.trim().parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "bad Content-Length")
                })?);
        }
    }
    let len =
        len.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    if len > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

pub fn write_message<W: Write>(w: &mut W, v: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(v).map_err(io::Error::other)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// Frame bytes for tests and golden transcripts.
pub fn frame(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = write_message(&mut out, v);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_two_frames() {
        let mut buf = frame(&json!({"a": 1}));
        buf.extend(frame(&json!({"b": "é"})));
        let mut r = io::Cursor::new(buf);
        let a: Value = serde_json::from_slice(&read_message(&mut r).unwrap().unwrap()).unwrap();
        let b: Value = serde_json::from_slice(&read_message(&mut r).unwrap().unwrap()).unwrap();
        assert_eq!((a, b), (json!({"a": 1}), json!({"b": "é"})));
        assert!(read_message(&mut r).unwrap().is_none());
    }

    #[test]
    fn extra_headers_and_case() {
        let mut r = io::Cursor::new(b"content-length: 2\r\nContent-Type: x\r\n\r\n{}".to_vec());
        assert_eq!(read_message(&mut r).unwrap().unwrap(), b"{}");
    }

    #[test]
    fn missing_length_is_an_error() {
        let mut r = io::Cursor::new(b"X: 1\r\n\r\n{}".to_vec());
        assert!(read_message(&mut r).is_err());
    }
}
