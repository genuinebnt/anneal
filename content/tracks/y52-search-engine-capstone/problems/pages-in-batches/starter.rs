use std::collections::HashMap;

// ---------- words and packed positions ----------

/// Where one word is: a page, and the places of the word on it, rising.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    pub page: u32,
    pub positions: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    Truncated,
    Overflow,
    NotAscending,
    NoPositions,
    TrailingBytes,
}

/// The words of a text, in order, lowercased.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// LEB128: 7 bits per byte, lowest first; the high bit says that more follow.
fn put_varint(mut n: u32, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n & 0x7f) as u8 | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn get_varint(bytes: &[u8], at: &mut usize) -> Result<u32, CodecError> {
    let mut value = 0u32;
    for shift in [0, 7, 14, 21, 28] {
        let byte = *bytes.get(*at).ok_or(CodecError::Truncated)?;
        *at += 1;
        if shift == 28 && byte > 0x0f {
            return Err(CodecError::Overflow);
        }
        value |= u32::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            break;
        }
    }
    Ok(value)
}

/// Reads a gap and adds it to `previous`. Only the first value of a list may have a gap of 0.
fn next_value(bytes: &[u8], at: &mut usize, previous: u32, first: bool) -> Result<u32, CodecError> {
    let gap = get_varint(bytes, at)?;
    if gap == 0 && !first {
        return Err(CodecError::NotAscending);
    }
    previous.checked_add(gap).ok_or(CodecError::Overflow)
}

/// Packs a list whose pages rise, each with at least one position, positions rising.
pub fn encode(list: &[Posting]) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(list.len() as u32, &mut out);
    let mut page = 0;
    for posting in list {
        put_varint(posting.page - page, &mut out);
        page = posting.page;
        put_varint(posting.positions.len() as u32, &mut out);
        let mut previous = 0;
        for &position in &posting.positions {
            put_varint(position - previous, &mut out);
            previous = position;
        }
    }
    out
}

/// Reads bytes that anyone may have written.
pub fn decode(bytes: &[u8]) -> Result<Vec<Posting>, CodecError> {
    let mut at = 0;
    let count = get_varint(bytes, &mut at)?;
    // `count` is only a claim, so nothing is reserved for it
    let mut list = Vec::new();
    let mut page = 0;
    for i in 0..count {
        page = next_value(bytes, &mut at, page, i == 0)?;
        let places = get_varint(bytes, &mut at)?;
        if places == 0 {
            return Err(CodecError::NoPositions);
        }
        let mut positions = Vec::new();
        let mut position = 0;
        for j in 0..places {
            position = next_value(bytes, &mut at, position, j == 0)?;
            positions.push(position);
        }
        list.push(Posting { page, positions });
    }
    if at != bytes.len() {
        return Err(CodecError::TrailingBytes);
    }
    Ok(list)
}

// ---------- your work: batches ----------

pub struct Index {
    // your fields
}

impl Index {
    pub fn new() -> Index {
        todo!()
    }

    /// Adds these pages as one new batch. An empty batch changes nothing.
    pub fn add_batch(&mut self, pages: &[&str]) {
        todo!()
    }

    pub fn batches(&self) -> usize {
        todo!()
    }

    /// How many pages there are.
    pub fn len(&self) -> u32 {
        todo!()
    }

    /// How many words a page has.
    pub fn page_len(&self, page: u32) -> Option<u32> {
        todo!()
    }

    /// Every page that has the word, with the places, in page order.
    pub fn postings(&self, word: &str) -> Vec<Posting> {
        todo!()
    }

    /// The pages where these words stand one straight after another.
    pub fn phrase(&self, words: &[&str]) -> Vec<u32> {
        todo!()
    }

    /// The total length of the packed lists: one for each word of each batch.
    pub fn packed_bytes(&self) -> usize {
        todo!()
    }
}
