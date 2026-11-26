pub struct Dictionary {}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl Dictionary {
    pub fn new() -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, word: &str) -> bool {
        todo!()
    }

    pub fn contains(&self, word: &str) -> bool {
        todo!()
    }

    pub fn remove(&mut self, word: &str) -> bool {
        todo!()
    }

    pub fn count_prefix(&self, prefix: &str) -> usize {
        todo!()
    }

    pub fn complete(&self, prefix: &str, limit: usize) -> Vec<String> {
        todo!()
    }

    pub fn longest_stored_prefix<'t>(&self, text: &'t str) -> Option<&'t str> {
        todo!()
    }

    pub fn prefix_count(&self) -> usize {
        todo!()
    }
}
