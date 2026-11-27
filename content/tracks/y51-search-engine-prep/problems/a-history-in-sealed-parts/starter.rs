pub struct History {
    // your fields
}

impl History {
    pub fn new() -> History {
        todo!()
    }

    /// Adds a batch of `(author, text)` messages, then joins parts by the rule in the statement.
    pub fn append(&mut self, batch: &[(u32, &str)]) {
        todo!()
    }

    /// How many messages each part holds, oldest part first.
    pub fn part_sizes(&self) -> Vec<usize> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    /// The author and text of message `n`.
    pub fn message(&self, n: usize) -> Option<(u32, &str)> {
        todo!()
    }

    /// The numbers of this author's messages, rising.
    pub fn by_author(&self, author: u32) -> Vec<usize> {
        todo!()
    }

    /// The mean length in characters over all messages.
    pub fn mean_length(&self) -> Option<f64> {
        todo!()
    }

    /// A history that answers as this one does now and does not see later appends.
    pub fn snapshot(&self) -> History {
        todo!()
    }
}
