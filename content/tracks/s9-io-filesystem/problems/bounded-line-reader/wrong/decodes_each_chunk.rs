use std::io::{self, BufRead};

#[derive(Debug, PartialEq, Eq)]
pub enum LineError {
    TooLong { line: u64 },
    NotUtf8 { line: u64 },
    Io(io::ErrorKind),
}

pub struct BoundedLines<R> {
    reader: R,
    max_len: usize,
    line: u64,
    done: bool,
}

impl<R: BufRead> BoundedLines<R> {
    pub fn new(reader: R, max_len: usize) -> Self {
        BoundedLines {
            reader,
            max_len,
            line: 0,
            done: false,
        }
    }
}

impl<R: BufRead> Iterator for BoundedLines<R> {
    type Item = Result<String, LineError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let keep = self.max_len.saturating_add(1);
        let mut text = String::new();
        let (mut too_long, mut bad, mut seen_any) = (false, false, false);
        let newline = loop {
            let chunk = match self.reader.fill_buf() {
                Ok(chunk) => chunk,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.done = true;
                    return Some(Err(LineError::Io(e.kind())));
                }
            };
            if chunk.is_empty() {
                self.done = true;
                if !seen_any {
                    return None;
                }
                break false;
            }
            seen_any = true;
            let end = chunk.iter().position(|&b| b == b'\n');
            let part = &chunk[..end.unwrap_or(chunk.len())];
            if !too_long {
                if part.len() > keep - text.len() {
                    too_long = true;
                } else {
                    // A character cut in two by the buffer looks invalid here.
                    match std::str::from_utf8(part) {
                        Ok(s) => text.push_str(s),
                        Err(_) => bad = true,
                    }
                }
            }
            let used = end.map_or(chunk.len(), |i| i + 1);
            self.reader.consume(used);
            if end.is_some() {
                break true;
            }
        };
        self.line += 1;
        if newline && text.ends_with('\r') {
            text.pop();
        }
        if too_long || text.len() > self.max_len {
            return Some(Err(LineError::TooLong { line: self.line }));
        }
        Some(if bad {
            Err(LineError::NotUtf8 { line: self.line })
        } else {
            Ok(text)
        })
    }
}
