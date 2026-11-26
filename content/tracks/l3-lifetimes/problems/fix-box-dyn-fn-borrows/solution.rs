use std::fmt::Display;

/// A predicate accepting exactly the words in `allowed`.
pub fn make_filter<'a>(allowed: &'a [&str]) -> Box<dyn Fn(&str) -> bool + 'a> {
    Box::new(move |w| allowed.contains(&w))
}

/// Named checks. Checks may borrow data that lives for `'a`.
pub struct Checks<'a> {
    list: Vec<(String, Box<dyn Fn(&str) -> bool + 'a>)>,
}

impl<'a> Checks<'a> {
    pub fn new() -> Self {
        Checks { list: Vec::new() }
    }

    pub fn add(&mut self, name: &str, check: impl Fn(&str) -> bool + 'a) {
        self.list.push((name.to_string(), Box::new(check)));
    }

    /// The names of the checks `word` fails, in the order they were added.
    pub fn failures(&self, word: &str) -> Vec<&str> {
        self.list
            .iter()
            .filter(|(_, c)| !c(word))
            .map(|(n, _)| n.as_str())
            .collect()
    }
}

/// How many of `words` pass `check`.
pub fn count_passing(words: &[&str], check: &dyn Fn(&str) -> bool) -> usize {
    words.iter().filter(|w| check(w)).count()
}

/// Each name as something displayable, for later.
pub fn labels(names: &[String]) -> Vec<Box<dyn Display + '_>> {
    names
        .iter()
        .map(|n| Box::new(n.as_str()) as Box<dyn Display>)
        .collect()
}
