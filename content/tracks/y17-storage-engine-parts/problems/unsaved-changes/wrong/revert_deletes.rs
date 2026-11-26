use std::collections::BTreeMap;

/// A large, sorted, read-only set of entries kept elsewhere. Every call is a round trip.
pub trait Catalogue {
    fn get(&self, key: &str) -> Option<String>;
    /// The entry with the smallest key that is `key` or later.
    fn at_or_after(&self, key: &str) -> Option<(String, String)>;
    /// The entry with the smallest key later than `key`.
    fn after(&self, key: &str) -> Option<(String, String)>;
}

/// Changes to a catalogue that have not been sent to it yet.
pub struct Draft<C: Catalogue> {
    base: C,
    /// `None` is a delete. It has to be kept: it hides what the catalogue still holds.
    changes: BTreeMap<String, Option<String>>,
}

impl<C: Catalogue> Draft<C> {
    pub fn new(base: C) -> Draft<C> {
        Draft {
            base,
            changes: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.changes.insert(key.to_string(), Some(value.to_string()));
    }

    pub fn delete(&mut self, key: &str) {
        self.changes.insert(key.to_string(), None);
    }

    /// Forgets the unsaved change to `key`, if there is one.
    pub fn revert(&mut self, key: &str) {
        self.changes.insert(key.to_string(), None);
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match self.changes.get(key) {
            Some(change) => change.clone(),
            None => self.base.get(key),
        }
    }

    /// Up to `limit` entries as the catalogue would look after saving, from key `from` on, in key order.
    pub fn page(&self, from: &str, limit: usize) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        let mut local = self
            .changes
            .range::<str, _>((std::ops::Bound::Included(from), std::ops::Bound::Unbounded))
            .peekable();
        let mut theirs = self.base.at_or_after(from);
        loop {
            // take whichever side has the smaller key; on a tie the local change wins and both move on
            let take_local = match (local.peek(), &theirs) {
                (Some((mine, _)), Some((their, _))) => *mine <= their,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => return out,
            };
            let mut used_base: Option<String> = None;
            if take_local {
                let Some((key, change)) = local.next() else {
                    return out;
                };
                if theirs.as_ref().is_some_and(|t| t.0 == *key) {
                    used_base = theirs.take().map(|t| t.0);
                }
                if let Some(value) = change {
                    out.push((key.clone(), value.clone()));
                }
            } else if let Some((key, value)) = theirs.take() {
                used_base = Some(key.clone());
                out.push((key, value));
            }
            if out.len() == limit {
                return out;
            }
            // ask for the next catalogue entry only once it is known to be needed
            if let Some(key) = used_base {
                theirs = self.base.after(&key);
            }
        }
    }

    /// What saving would send, in key order: a new value, or `None` for a delete.
    pub fn changes(&self) -> Vec<(String, Option<String>)> {
        self.changes
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
