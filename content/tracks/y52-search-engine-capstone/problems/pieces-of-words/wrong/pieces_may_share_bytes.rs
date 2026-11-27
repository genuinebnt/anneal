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

// ---------- batches ----------

/// One batch of pages. Nothing in it changes after it is built.
struct Segment {
    /// the number of its first page
    first: u32,
    /// how many words each page has
    page_len: Vec<u32>,
    /// word -> its packed postings in this batch
    words: HashMap<String, Vec<u8>>,
    /// each page, folded, to check what the windows only suggest
    texts: Vec<String>,
    /// three bytes -> the pages whose folded text has them, rising
    windows: HashMap<[u8; 3], Vec<u32>>,
}

impl Segment {
    fn build(first: u32, pages: &[&str]) -> Segment {
        let mut page_len = Vec::with_capacity(pages.len());
        let mut lists: HashMap<String, Vec<Posting>> = HashMap::new();
        let mut texts = Vec::with_capacity(pages.len());
        let mut windows: HashMap<[u8; 3], Vec<u32>> = HashMap::new();
        for (page, text) in (first..).zip(pages) {
            let folded = fold(text);
            for window in windows3(&folded) {
                let list = windows.entry(window).or_default();
                if list.last() != Some(&page) {
                    list.push(page);
                }
            }
            texts.push(folded);
            let words = tokenize(text);
            page_len.push(words.len() as u32);
            for (position, word) in (0u32..).zip(words) {
                let list = lists.entry(word).or_default();
                match list.last_mut() {
                    Some(last) if last.page == page => last.positions.push(position),
                    _ => list.push(Posting {
                        page,
                        positions: vec![position],
                    }),
                }
            }
        }
        let words = lists.into_iter().map(|(w, l)| (w, encode(&l))).collect();
        Segment {
            first,
            page_len,
            words,
            texts,
            windows,
        }
    }

    fn postings(&self, word: &str) -> Vec<Posting> {
        self.words
            .get(word)
            .map(|bytes| decode(bytes).expect("a batch holds only what `encode` wrote"))
            .unwrap_or_default()
    }
}

pub struct Index {
    segments: Vec<Segment>,
}

impl Index {
    pub fn new() -> Index {
        Index {
            segments: Vec::new(),
        }
    }

    /// Adds these pages as one new batch. An empty batch changes nothing.
    pub fn add_batch(&mut self, pages: &[&str]) {
        if !pages.is_empty() {
            let segment = Segment::build(self.len(), pages);
            self.segments.push(segment);
        }
    }

    pub fn batches(&self) -> usize {
        self.segments.len()
    }

    /// How many pages there are.
    pub fn len(&self) -> u32 {
        self.segments
            .last()
            .map_or(0, |s| s.first + s.page_len.len() as u32)
    }

    /// How many words a page has.
    pub fn page_len(&self, page: u32) -> Option<u32> {
        let after = self.segments.partition_point(|s| s.first <= page);
        let segment = &self.segments[after.checked_sub(1)?];
        segment
            .page_len
            .get((page - segment.first) as usize)
            .copied()
    }

    /// Every page that has the word, with the places, in page order.
    pub fn postings(&self, word: &str) -> Vec<Posting> {
        self.segments
            .iter()
            .flat_map(|s| s.postings(word))
            .collect()
    }

    /// The pages where these words stand one straight after another.
    pub fn phrase(&self, words: &[&str]) -> Vec<u32> {
        let lists: Vec<Vec<Posting>> = words.iter().map(|w| self.postings(w)).collect();
        let Some((first, rest)) = lists.split_first() else {
            return Vec::new();
        };
        let at = |list: &[Posting], page: u32, position: Option<u32>| {
            let found = list.binary_search_by_key(&page, |p| p.page);
            position.is_some_and(|position| {
                found.is_ok_and(|i| list[i].positions.binary_search(&position).is_ok())
            })
        };
        first
            .iter()
            .filter(|start| {
                start.positions.iter().any(|&p| {
                    (1u32..)
                        .zip(rest)
                        .all(|(i, list)| at(list, start.page, p.checked_add(i)))
                })
            })
            .map(|start| start.page)
            .collect()
    }

    /// The total length of the packed lists: one for each word of each batch.
    pub fn packed_bytes(&self) -> usize {
        self.segments
            .iter()
            .flat_map(|s| s.words.values())
            .map(Vec::len)
            .sum()
    }
}

// ---------- the query language ----------

/// The most brackets and NOTs that may be open at once.
pub const MAX_NESTING: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// a word or a quote with no letter or digit in it
    Nothing,
    Word(String),
    Phrase(Vec<String>),
    /// a bare word with a `*` in it, folded
    Pattern(String),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryError {
    UnterminatedQuote(usize),
    Unexpected(usize),
    UnexpectedEnd,
    TooDeep(usize),
}

/// Lowercases one char at a time. Unlike `str::to_lowercase`, the result for a char never depends on its neighbours.
pub fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

#[derive(Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Quoted(String),
    Bare(String),
}

fn lex(text: &str) -> Result<Vec<(Token, usize)>, QueryError> {
    let ends_word = |c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"');
    let mut chars = text.chars().enumerate().peekable();
    let mut tokens = Vec::new();
    while let Some((pos, c)) = chars.next() {
        let token = match c {
            '(' => Token::Open,
            ')' => Token::Close,
            '"' => {
                let mut inside = String::new();
                loop {
                    match chars.next() {
                        Some((_, '"')) => break,
                        Some((_, c)) => inside.push(c),
                        None => return Err(QueryError::UnterminatedQuote(pos)),
                    }
                }
                Token::Quoted(inside)
            }
            c if c.is_whitespace() => continue,
            c => {
                let mut word = String::from(c);
                while let Some((_, c)) = chars.next_if(|&(_, c)| !ends_word(c)) {
                    word.push(c);
                }
                Token::Bare(word)
            }
        };
        tokens.push((token, pos));
    }
    Ok(tokens)
}

fn words_of(text: &str) -> Expr {
    let mut words = tokenize(text);
    match words.len() {
        0 => Expr::Nothing,
        1 => Expr::Word(words.remove(0)),
        _ => Expr::Phrase(words),
    }
}

/// One item stays as it is; two or more are wrapped.
fn group(mut items: Vec<Expr>, wrap: fn(Vec<Expr>) -> Expr) -> Expr {
    if items.len() == 1 {
        items.remove(0)
    } else {
        wrap(items)
    }
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    at: usize,
    nesting: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(t, _)| t)
    }

    fn is_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek(), Some(Token::Bare(w)) if w == keyword)
    }

    fn stuck(&self) -> QueryError {
        match self.tokens.get(self.at) {
            Some(&(_, pos)) => QueryError::Unexpected(pos),
            None => QueryError::UnexpectedEnd,
        }
    }

    fn or(&mut self) -> Result<Expr, QueryError> {
        let mut items = vec![self.and()?];
        while self.is_keyword("OR") {
            self.at += 1;
            items.push(self.and()?);
        }
        Ok(group(items, Expr::Or))
    }

    fn and(&mut self) -> Result<Expr, QueryError> {
        let mut items = vec![self.not()?];
        while !matches!(self.peek(), None | Some(Token::Close)) && !self.is_keyword("OR") {
            if self.is_keyword("AND") {
                self.at += 1;
            }
            items.push(self.not()?);
        }
        Ok(group(items, Expr::And))
    }

    /// Takes the token at hand, which opens one more level.
    fn go_deeper(&mut self) -> Result<(), QueryError> {
        if self.nesting == MAX_NESTING {
            return Err(QueryError::TooDeep(self.tokens[self.at].1));
        }
        self.nesting += 1;
        self.at += 1;
        Ok(())
    }

    fn not(&mut self) -> Result<Expr, QueryError> {
        if !self.is_keyword("NOT") {
            return self.atom();
        }
        self.go_deeper()?;
        let inner = self.not()?;
        self.nesting -= 1;
        Ok(Expr::Not(Box::new(inner)))
    }

    fn atom(&mut self) -> Result<Expr, QueryError> {
        let item = match self.peek() {
            Some(Token::Open) => {
                self.go_deeper()?;
                let inside = self.or()?;
                if self.peek() != Some(&Token::Close) {
                    return Err(self.stuck());
                }
                self.nesting -= 1;
                inside
            }
            Some(Token::Quoted(text)) => words_of(text),
            Some(Token::Bare(word)) if word != "AND" && word != "OR" => {
                if word.contains('*') {
                    Expr::Pattern(fold(word))
                } else {
                    words_of(word)
                }
            }
            _ => return Err(self.stuck()),
        };
        self.at += 1;
        Ok(item)
    }
}

/// Reads a query that a stranger typed.
pub fn parse_query(text: &str) -> Result<Expr, QueryError> {
    let mut parser = Parser {
        tokens: lex(text)?,
        at: 0,
        nesting: 0,
    };
    let query = parser.or()?;
    if parser.peek().is_some() {
        return Err(parser.stuck());
    }
    Ok(query)
}

// ---------- pieces of words ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternHits {
    /// how many pages had every three-byte window of the pattern, and so had to be read
    pub candidates: usize,
    /// the pages that fit, rising
    pub pages: Vec<u32>,
}

/// Every run of three neighbouring bytes.
fn windows3(text: &str) -> impl Iterator<Item = [u8; 3]> + '_ {
    text.as_bytes().windows(3).map(|w| [w[0], w[1], w[2]])
}

/// Whether the pieces stand in the text in order, without sharing bytes.
fn has_pieces(text: &str, pieces: &[&str]) -> bool {
    let mut rest = text;
    pieces.iter().all(|piece| match rest.find(piece) {
        Some(at) => {
            let mut next = at + 1;
            while !rest.is_char_boundary(next) {
                next += 1;
            }
            rest = &rest[next..];
            true
        }
        None => false,
    })
}

impl Segment {
    fn find_pattern(&self, pieces: &[&str], hits: &mut PatternHits) {
        let mut lists = Vec::new();
        for window in pieces.iter().flat_map(|p| windows3(p)) {
            match self.windows.get(&window) {
                Some(list) => lists.push(list),
                None => return, // no page of this batch has it
            }
        }
        lists.sort_by_key(|list| list.len());
        let candidates: Vec<u32> = match lists.split_first() {
            Some((shortest, others)) => shortest
                .iter()
                .copied()
                .filter(|page| others.iter().all(|l| l.binary_search(page).is_ok()))
                .collect(),
            // every piece is shorter than three bytes
            None => (self.first..).take(self.page_len.len()).collect(),
        };
        hits.candidates += candidates.len();
        hits.pages.extend(
            candidates
                .into_iter()
                .filter(|&page| has_pieces(&self.texts[(page - self.first) as usize], pieces)),
        );
    }
}

impl Index {
    /// The pages whose text holds the pieces of `pattern`, which `*` separates.
    pub fn find_pattern(&self, pattern: &str) -> PatternHits {
        let pattern = fold(pattern);
        let pieces: Vec<&str> = pattern.split('*').filter(|p| !p.is_empty()).collect();
        let mut hits = PatternHits {
            candidates: 0,
            pages: Vec::new(),
        };
        for segment in &self.segments {
            segment.find_pattern(&pieces, &mut hits);
        }
        hits
    }
}
