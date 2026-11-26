use std::collections::HashMap;

/// Stores each distinct name once. Callers compare and hash the `&str`s it hands out instead of copying names.
pub struct Interner {
    ids: HashMap<String, usize>,
    names: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Interner {
            ids: HashMap::new(),
            names: Vec::new(),
        }
    }

    /// Stores `name` if it's new, and returns the stored name.
    pub fn intern(&mut self, name: &str) -> &str {
        let i = match self.ids.get(name) {
            Some(&i) => i,
            None => {
                self.ids.insert(name.to_string(), self.names.len());
                self.names.push(name.to_string());
                self.names.len() - 1
            }
        };
        &self.names[i]
    }

    /// The stored name equal to `name`, if it has been interned.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.ids.get(name).map(|&i| self.names[i].as_str())
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }
}

/// Interns every name and returns the stored names, in the same order.
pub fn intern_all<'i>(interner: &'i mut Interner, names: &[&str]) -> Vec<&'i str> {
    let mut out = Vec::new();
    for n in names {
        out.push(interner.intern(n));
    }
    out
}

/// Interns both ends of every edge. Returns the edges as pairs of stored names, and how many names were new.
pub fn intern_edges<'i>(
    interner: &'i mut Interner,
    edges: &[(&str, &str)],
) -> (Vec<(&'i str, &'i str)>, usize) {
    let before = interner.len();
    let mut out = Vec::new();
    for &(a, b) in edges {
        let a = interner.intern(a);
        let b = interner.intern(b);
        out.push((a, b));
    }
    (out, interner.len() - before)
}
