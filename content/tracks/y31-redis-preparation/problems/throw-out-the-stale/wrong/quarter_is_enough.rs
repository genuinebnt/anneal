use std::collections::HashMap;

/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

/// Sessions and when each one ends.
#[derive(Debug, Default)]
pub struct Sessions {
    /// In no particular order, so that any one can be picked by its index.
    items: Vec<(String, u64)>,
    places: HashMap<String, usize>,
}

impl Sessions {
    pub fn new() -> Sessions {
        Sessions::default()
    }

    /// Adds the session, or moves its end.
    pub fn set(&mut self, id: &str, ends_at: u64) {
        match self.places.get(id) {
            Some(&i) => self.items[i].1 = ends_at,
            None => {
                self.places.insert(id.to_string(), self.items.len());
                self.items.push((id.to_string(), ends_at));
            }
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let Some(i) = self.places.remove(id) else {
            return false;
        };
        // the last item moves into the hole
        self.items.swap_remove(i);
        if let Some((moved, _)) = self.items.get(i) {
            self.places.insert(moved.clone(), i);
        }
        true
    }

    pub fn ends_at(&self, id: &str) -> Option<u64> {
        self.places.get(id).map(|&i| self.items[i].1)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Removes ended sessions found by chance, and returns their ids.
    pub fn sweep(&mut self, now: u64, dice: &mut dyn Dice, max_rounds: usize) -> Vec<String> {
        let mut removed = Vec::new();
        for _ in 0..max_rounds {
            let looks = self.len().min(20);
            if looks == 0 {
                break;
            }
            let before = removed.len();
            for _ in 0..looks {
                if self.is_empty() {
                    break;
                }
                let (id, ends_at) = &self.items[dice.below(self.len())];
                if *ends_at <= now {
                    let id = id.clone();
                    self.remove(&id);
                    removed.push(id);
                }
            }
            if (removed.len() - before) * 4 < looks {
                break;
            }
        }
        removed
    }
}
