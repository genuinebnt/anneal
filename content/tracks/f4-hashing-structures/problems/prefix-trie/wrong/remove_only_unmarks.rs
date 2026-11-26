#[derive(Default)]
struct Node {
    /// Sorted by char, so a walk in order yields words in ascending order.
    children: Vec<(char, usize)>,
    ends_word: bool,
    /// Stored words that end at this node or below it.
    words: usize,
}

pub struct Dictionary {
    nodes: Vec<Node>,
    free: Vec<usize>,
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl Dictionary {
    pub fn new() -> Self {
        Dictionary {
            nodes: vec![Node::default()],
            free: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.nodes[0].words
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn child(&self, node: usize, c: char) -> Option<usize> {
        let children = &self.nodes[node].children;
        children
            .binary_search_by_key(&c, |&(c, _)| c)
            .ok()
            .map(|i| children[i].1)
    }

    fn walk(&self, prefix: &str) -> Option<usize> {
        prefix.chars().try_fold(0, |node, c| self.child(node, c))
    }

    fn allocate(&mut self) -> usize {
        match self.free.pop() {
            Some(i) => i,
            None => {
                self.nodes.push(Node::default());
                self.nodes.len() - 1
            }
        }
    }

    pub fn insert(&mut self, word: &str) -> bool {
        if self.contains(word) {
            return false;
        }
        let mut node = 0;
        self.nodes[node].words += 1;
        for c in word.chars() {
            node = match self.child(node, c) {
                Some(next) => next,
                None => {
                    let next = self.allocate();
                    let children = &mut self.nodes[node].children;
                    let at = children.partition_point(|&(other, _)| other < c);
                    children.insert(at, (c, next));
                    next
                }
            };
            self.nodes[node].words += 1;
        }
        self.nodes[node].ends_word = true;
        true
    }

    pub fn contains(&self, word: &str) -> bool {
        self.walk(word).is_some_and(|n| self.nodes[n].ends_word)
    }

    pub fn remove(&mut self, word: &str) -> bool {
        if !self.contains(word) {
            return false;
        }
        let mut node = 0;
        self.nodes[node].words -= 1;
        for (at, c) in word.char_indices() {
            let Some(next) = self.child(node, c) else {
                return false;
            };
            if self.nodes[next].words == usize::MAX {
                // Only this word goes below here: unlink the chain and recycle it.
                self.nodes[node].children.retain(|&(_, n)| n != next);
                self.recycle(next, &word[at + c.len_utf8()..]);
                return true;
            }
            node = next;
            self.nodes[node].words -= 1;
        }
        self.nodes[node].ends_word = false;
        true
    }

    /// Frees `node` and the chain below it that spells `rest`.
    fn recycle(&mut self, mut node: usize, rest: &str) {
        for c in rest.chars() {
            let next = self.child(node, c);
            self.nodes[node] = Node::default();
            self.free.push(node);
            match next {
                Some(n) => node = n,
                None => return,
            }
        }
        self.nodes[node] = Node::default();
        self.free.push(node);
    }

    pub fn count_prefix(&self, prefix: &str) -> usize {
        self.walk(prefix).map_or(0, |n| self.nodes[n].words)
    }

    pub fn complete(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(start) = self.walk(prefix) {
            self.collect(start, &mut prefix.to_string(), limit, &mut out);
        }
        out
    }

    /// Uses its own stack, so a very long word cannot overflow the call stack.
    fn collect(&self, start: usize, word: &mut String, limit: usize, out: &mut Vec<String>) {
        let mut stack = vec![(start, 0usize)];
        if limit > 0 && self.nodes[start].ends_word {
            out.push(word.clone());
        }
        while out.len() < limit {
            let Some(top) = stack.last_mut() else { break };
            let (node, index) = *top;
            match self.nodes[node].children.get(index) {
                Some(&(c, child)) => {
                    top.1 += 1;
                    word.push(c);
                    if self.nodes[child].ends_word {
                        out.push(word.clone());
                    }
                    stack.push((child, 0));
                }
                None => {
                    stack.pop();
                    word.pop();
                }
            }
        }
    }

    pub fn longest_stored_prefix<'t>(&self, text: &'t str) -> Option<&'t str> {
        let mut node = 0;
        let mut best = self.nodes[0].ends_word.then_some(0);
        for (at, c) in text.char_indices() {
            let Some(next) = self.child(node, c) else {
                break;
            };
            node = next;
            if self.nodes[node].ends_word {
                best = Some(at + c.len_utf8());
            }
        }
        best.map(|end| &text[..end])
    }

    pub fn prefix_count(&self) -> usize {
        self.nodes.len() - 1 - self.free.len()
    }
}
