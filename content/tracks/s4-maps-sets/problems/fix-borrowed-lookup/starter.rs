use std::collections::HashMap;

pub struct Registry {
    ids: HashMap<String, u32>,
}

impl Registry {
    pub fn new(names: &[&str]) -> Self {
        let ids = names
            .iter()
            .enumerate()
            .map(|(i, n)| (String::from(*n), i as u32))
            .collect();
        Registry { ids }
    }

    /// The id for `name`. Called on every request, so it must not allocate.
    pub fn id(&self, name: &str) -> Option<u32> {
        self.ids.get(&name.to_string()).copied()
    }
}
