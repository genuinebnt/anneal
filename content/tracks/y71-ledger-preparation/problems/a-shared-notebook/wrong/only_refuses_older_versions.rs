use std::collections::HashMap;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<T> {
    pub pos: u64,
    pub stream: String,
    pub version: u64,
    pub item: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub expected: u64,
    pub actual: u64,
}

pub struct Journal<T> {
    log: Vec<Record<T>>,
    /// stream -> places in `log`, oldest first. A stream is here only once it has a record.
    streams: HashMap<String, Vec<usize>>,
}

impl<T> Journal<T> {
    pub fn new() -> Self {
        Journal {
            log: Vec::new(),
            streams: HashMap::new(),
        }
    }

    pub fn append(
        &mut self,
        stream: &str,
        expected: u64,
        items: Vec<T>,
    ) -> Result<Range<u64>, Conflict> {
        let actual = self.version(stream);
        if actual > expected {
            return Err(Conflict { expected, actual });
        }
        let first = self.log.len() as u64;
        if !items.is_empty() {
            let places = self.streams.entry(stream.to_string()).or_default();
            for item in items {
                places.push(self.log.len());
                self.log.push(Record {
                    pos: self.log.len() as u64,
                    stream: stream.to_string(),
                    version: places.len() as u64,
                    item,
                });
            }
        }
        Ok(first..self.log.len() as u64)
    }

    pub fn version(&self, stream: &str) -> u64 {
        self.streams.get(stream).map_or(0, |p| p.len() as u64)
    }

    pub fn read_stream<'a>(
        &'a self,
        stream: &str,
        after: u64,
    ) -> impl Iterator<Item = &'a Record<T>> {
        let places = self.streams.get(stream).map_or(&[][..], Vec::as_slice);
        let start = usize::try_from(after).unwrap_or(usize::MAX);
        places
            .get(start..)
            .unwrap_or(&[])
            .iter()
            .map(|&i| &self.log[i])
    }

    pub fn read_all(&self, from: u64) -> &[Record<T>] {
        let start = usize::try_from(from).unwrap_or(usize::MAX);
        self.log.get(start..).unwrap_or(&[])
    }

    pub fn streams(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.streams.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

impl<T> Default for Journal<T> {
    fn default() -> Self {
        Self::new()
    }
}
