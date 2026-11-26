use std::io::{self, Write};

pub enum Content {
    Full(Vec<u8>),
    /// Produced while it is being sent.
    Pieces(Box<dyn Iterator<Item = Vec<u8>>>),
}

pub struct Reply {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub content: Content,
}

/// The status line and the reply's own headers, or an error if anything in them could end a line early.
fn opening(reply: &Reply) -> io::Result<Vec<u8>> {
    let breaks = |s: &str| s.contains(['\r', '\n']);
    let reserved = |name: &str| name.eq_ignore_ascii_case("content-length") || name.eq_ignore_ascii_case("transfer-encoding");
    if false && breaks(&reply.reason) || reply.headers.iter().any(|(name, value)| breaks(name) || breaks(value) || reserved(name)) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the reply would change the framing"));
    }
    let mut head = Vec::new();
    write!(head, "HTTP/1.1 {} {}\r\n", reply.status, reply.reason)?;
    for (name, value) in &reply.headers {
        write!(head, "{name}: {value}\r\n")?;
    }
    Ok(head)
}

pub fn send<W: Write>(out: &mut W, reply: Reply, head_only: bool) -> io::Result<()> {
    let mut head = opening(&reply)?;
    let bodyless = reply.status < 200 || reply.status == 204 || reply.status == 304;
    if bodyless {
        head.extend_from_slice(b"\r\n");
        out.write_all(&head)?;
        return out.flush();
    }
    match reply.content {
        Content::Full(body) => {
            write!(head, "Content-Length: {}\r\n\r\n", body.len())?;
            out.write_all(&head)?;
            if !head_only {
                out.write_all(&body)?;
            }
        }
        Content::Pieces(pieces) => {
            head.extend_from_slice(b"Transfer-Encoding: chunked\r\n\r\n");
            out.write_all(&head)?;
            if !head_only {
                for piece in pieces {
                    // a chunk of length zero is what ends the body
                    if piece.is_empty() {
                        continue;
                    }
                    write!(out, "{:x}\r\n", piece.len())?;
                    out.write_all(&piece)?;
                    out.write_all(b"\r\n")?;
                    out.flush()?;
                }
                out.write_all(b"0\r\n\r\n")?;
            }
        }
    }
    out.flush()
}
