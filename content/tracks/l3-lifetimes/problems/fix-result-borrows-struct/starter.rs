pub struct Index<'a> {
    lines: Vec<&'a str>,
}

impl<'a> Index<'a> {
    pub fn new(text: &'a str) -> Self {
        Index {
            lines: text.lines().collect(),
        }
    }

    /// The longest line; the first on a tie.
    pub fn longest_line(&self) -> &str {
        self.lines
            .iter()
            .copied()
            .fold("", |best, l| if l.len() > best.len() { l } else { best })
    }

    /// Every line containing `word`.
    pub fn lines_with(&self, word: &str) -> Vec<&str> {
        self.lines
            .iter()
            .copied()
            .filter(|l| l.contains(word))
            .collect()
    }

    /// The lines, in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().copied()
    }

    /// Hands the lines over.
    pub fn into_lines(self) -> Vec<&str> {
        self.lines
    }
}
