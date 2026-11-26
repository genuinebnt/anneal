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
}

impl<R: BufRead> BoundedLines<R> {
    pub fn new(reader: R, max_len: usize) -> Self {
        BoundedLines { reader, max_len }
    }
}

impl<R: BufRead> Iterator for BoundedLines<R> {
    type Item = Result<String, LineError>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
