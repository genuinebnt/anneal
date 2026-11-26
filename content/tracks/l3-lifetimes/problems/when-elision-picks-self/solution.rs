use std::collections::HashMap;

pub struct Glossary {
    terms: HashMap<String, String>,
}

/// The first word of `text`: everything before the first space.
pub fn first_word(text: &str) -> &str {
    text.split(' ').next().unwrap_or("")
}

impl Glossary {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Glossary {
            terms: pairs
                .iter()
                .map(|&(w, d)| (w.to_string(), d.to_string()))
                .collect(),
        }
    }

    /// The definition of `word`.
    pub fn define(&self, word: &str) -> Option<&str> {
        self.terms.get(word).map(String::as_str)
    }

    /// The definition of `word`, or `word` itself when the glossary doesn't have it.
    pub fn define_or_echo<'a>(&'a self, word: &'a str) -> &'a str {
        self.define(word).unwrap_or(word)
    }

    /// Whichever of `a` and `b` has the longer definition (`a` on a tie; no definition counts as length 0).
    pub fn pick<'w>(&self, a: &'w str, b: &'w str) -> &'w str {
        let len = |w: &str| self.define(w).map_or(0, str::len);
        if len(b) > len(a) {
            b
        } else {
            a
        }
    }
}

/// The definition of the first word of `text`.
pub fn define_first<'g>(g: &'g Glossary, text: &str) -> Option<&'g str> {
    g.define(first_word(text))
}
