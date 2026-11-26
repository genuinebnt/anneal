pub struct Library {
    books: Vec<String>,
}

impl Library {
    pub fn new(books: Vec<String>) -> Self {
        Library { books }
    }

    /// Titles containing `needle`.
    pub fn search(&self, needle: &str) -> Vec<&str> {
        self.books
            .iter()
            .filter(|b| b.contains(needle))
            .map(String::as_str)
            .collect()
    }

    /// The longest title; the last one on a tie.
    pub fn longest(&self) -> Option<&str> {
        self.books
            .iter()
            .max_by_key(|b| b.len())
            .map(String::as_str)
    }

    pub fn count_with(&self, needle: &str) -> usize {
        self.books.iter().filter(|b| b.contains(needle)).count()
    }
}
