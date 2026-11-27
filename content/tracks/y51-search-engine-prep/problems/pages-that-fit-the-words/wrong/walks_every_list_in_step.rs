use std::collections::HashMap;

/// A rising list of page numbers that can only be read one number at a time.
pub trait PageList {
    /// How many numbers it holds. Asking is free.
    fn len(&self) -> usize;
    /// The number at place `i`, which must be below `len()`. The tests count every call.
    fn get(&self, i: usize) -> u32;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The first place at or after `from` whose number is not below `wanted`.
fn first_not_below(list: &dyn PageList, from: usize, wanted: u32) -> usize {
    let mut low = from;
    while low < list.len() && list.get(low) < wanted {
        low += 1;
    }
    low
}

/// The numbers that are in every list, rising. No lists give no numbers.
pub fn in_every_list(lists: &[&dyn PageList]) -> Vec<u32> {
    let Some(driver) = (0..lists.len()).min_by_key(|&i| lists[i].len()) else {
        return Vec::new();
    };
    // where each list was left: the wanted numbers only rise
    let mut from = vec![0; lists.len()];
    let mut found = Vec::new();
    'numbers: for place in 0..lists[driver].len() {
        let number = lists[driver].get(place);
        for (i, list) in lists.iter().enumerate() {
            if i == driver {
                continue;
            }
            from[i] = first_not_below(*list, from[i], number);
            if from[i] == list.len() || list.get(from[i]) != number {
                continue 'numbers;
            }
        }
        found.push(number);
    }
    found
}

pub struct Shelf {
    /// word -> the pages that hold it, rising
    lists: HashMap<String, Vec<u32>>,
    pages: u32,
}

impl Shelf {
    pub fn new() -> Shelf {
        Shelf {
            lists: HashMap::new(),
            pages: 0,
        }
    }

    /// Stores the next page and returns its number: 0, 1, 2, ...
    pub fn add(&mut self, text: &str) -> u32 {
        let page = self.pages;
        self.pages += 1;
        for word in text.split(|c: char| !c.is_alphanumeric()) {
            if word.is_empty() {
                continue;
            }
            let list = self.lists.entry(word.to_lowercase()).or_default();
            if list.last() != Some(&page) {
                list.push(page);
            }
        }
        page
    }

    fn list(&self, word: &str) -> &[u32] {
        self.lists
            .get(&word.to_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// The pages that hold `word`, in rising order.
    pub fn pages_with(&self, word: &str) -> Vec<u32> {
        self.list(word).to_vec()
    }

    /// The pages with every word of `all`, at least one of `any` (unless it is empty) and none of `none`.
    pub fn find(&self, all: &[&str], any: &[&str], none: &[&str]) -> Vec<u32> {
        let all: Vec<&[u32]> = all.iter().map(|w| self.list(w)).collect();
        let any: Vec<&[u32]> = any.iter().map(|w| self.list(w)).collect();
        let none: Vec<&[u32]> = none.iter().map(|w| self.list(w)).collect();
        let start: Vec<u32> = match all.iter().min_by_key(|l| l.len()) {
            Some(shortest) => shortest.to_vec(),
            None if any.is_empty() => (0..self.pages).collect(),
            None => {
                let mut joined = any.concat();
                joined.sort_unstable();
                joined.dedup();
                joined
            }
        };
        let holds = |list: &&[u32], page: u32| list.binary_search(&page).is_ok();
        start
            .into_iter()
            .filter(|&p| all.iter().all(|l| holds(l, p)))
            .filter(|&p| any.is_empty() || any.iter().any(|l| holds(l, p)))
            .filter(|&p| !none.iter().any(|l| holds(l, p)))
            .collect()
    }
}
