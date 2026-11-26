pub struct Library {
    books: Vec<String>,
}

impl Library {
    pub fn new(books: Vec<String>) -> Self {
        Library { books }
    }

    /// Titles containing `needle`.
    pub fn search(&self, needle: &str) -> Vec<String> {
        self.books
            .clone()
            .into_iter()
            .filter(|b| b.contains(needle))
            .collect()
    }

    /// The longest title; the last one on a tie.
    pub fn longest(&self) -> Option<String> {
        let mut books = self.books.clone();
        books.sort_by_key(|b| b.len());
        books.last().cloned()
    }

    pub fn count_with(&self, needle: &str) -> usize {
        self.search(&needle.to_string()).len()
    }
}
