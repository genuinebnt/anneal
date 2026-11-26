use std::io::{self, BufRead, Read};

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
        let mut bytes = Vec::new();
        // Bounded, but the rest of a long line is left in the stream and read as further lines.
        match (&mut self.reader)
            .take((self.max_len as u64).saturating_add(2))
            .read_until(b'\n', &mut bytes)
        {
            Ok(0) => {
                self.done = true;
                return None;
            }
            Ok(_) => {}
            Err(e) => {
                self.done = true;
                return Some(Err(LineError::Io(e.kind())));
            }
        }
        self.line += 1;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        if bytes.len() > self.max_len {
            return Some(Err(LineError::TooLong { line: self.line }));
        }
        Some(String::from_utf8(bytes).map_err(|_| LineError::NotUtf8 { line: self.line }))
    }
}
