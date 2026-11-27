use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

/// A play counts half after this many seconds.
pub const HALF_LIFE: u64 = 3600;
/// No question is answered with more suggestions than this.
pub const MOST: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// The term as it was first spelled, with the whitespace around it removed.
    pub term: String,
    /// Its decayed popularity at the time of the question.
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    /// Pairs that created a term.
    pub added: usize,
    /// Pairs for a term that was already there.
    pub existing: usize,
}

pub struct Terms {
    // TODO: the fields
}

impl Terms {
    pub fn new() -> Terms {
        todo!()
    }

    /// The number of terms held.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adds `count` choices of each term, in order, all at time `now`.
    pub fn add(&mut self, batch: &[(&str, u32)], now: u64) -> Added {
        todo!()
    }

    /// One user chose this term at time `now`. False when there is no such term.
    pub fn choose(&mut self, term: &str, now: u64) -> bool {
        todo!()
    }

    /// The best terms whose key starts with the key of `typed`.
    pub fn suggest(&self, typed: &str, limit: usize, now: u64) -> Vec<Suggestion> {
        todo!()
    }
}

// ---- given: the search key (from "Light up what the user typed") ----

/// What a lowercase letter is replaced with in a key. `None` means: keep it. This table is deliberately small.
fn plain(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' | 'ń' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'ł' => "l",
        'š' | 'ś' => "s",
        'ž' | 'ź' | 'ż' => "z",
        _ => return None,
    })
}

fn is_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// Adds one character of the text to a key that may still end in a space.
fn push(key: &mut String, c: char) {
    if c.is_whitespace() {
        if !key.is_empty() && !key.ends_with(' ') {
            key.push(' ');
        }
    } else if !c.is_control() {
        for low in c.to_lowercase().filter(|&l| !is_mark(l)) {
            match plain(low) {
                Some(letters) => key.push_str(letters),
                None => key.push(low),
            }
        }
    }
}

/// The form of `text` that is used for matching.
pub fn search_key(text: &str) -> String {
    let mut key = String::new();
    for c in text.chars() {
        push(&mut key, c);
    }
    if key.ends_with(' ') {
        key.pop();
    }
    key
}
