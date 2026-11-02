use std::collections::{BTreeMap, BTreeSet, HashMap};

pub struct Index {
    docs: BTreeMap<u32, String>,
    grams: HashMap<[char; 3], BTreeSet<u32>>,
}

fn grams_of(text: &str) -> BTreeSet<[char; 3]> {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

impl Index {
    pub fn new() -> Index {
        Index { docs: BTreeMap::new(), grams: HashMap::new() }
    }

    pub fn add(&mut self, id: u32, text: &str) {
        self.remove(id); // a replaced document must not keep matching its old trigrams
        for g in grams_of(text) {
            self.grams.entry(g).or_default().insert(id);
        }
        self.docs.insert(id, text.to_string());
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let Some(old) = self.docs.remove(&id) else {
            return false;
        };
        for g in grams_of(&old) {
            if let Some(set) = self.grams.get_mut(&g) {
                set.remove(&id);
                if set.is_empty() {
                    self.grams.remove(&g);
                }
            }
        }
        true
    }

    pub fn candidates(&self, query: &str) -> Option<Vec<u32>> {
        if query.chars().count() < 3 {
            return None;
        }
        let mut lists: Vec<&BTreeSet<u32>> = Vec::new();
        for g in grams_of(query) {
            match self.grams.get(&g) {
                Some(set) => lists.push(set),
                None => {}
            }
        }
        lists.sort_by_key(|s| s.len()); // start from the rarest trigram
        let mut result: Vec<u32> = lists[0].iter().copied().collect();
        for other in &lists[1..] {
            result.retain(|id| other.contains(id));
        }
        Some(result)
    }

    pub fn search(&self, query: &str) -> Vec<u32> {
        match self.candidates(query) {
            Some(c) => c.into_iter().filter(|id| self.docs[id].contains(query)).collect(), // trigrams present is not the same as contiguous
            None => self.docs.iter().filter(|(_, t)| t.contains(query)).map(|(&id, _)| id).collect(),
        }
    }
}
