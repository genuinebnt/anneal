//! A thread-safe store over a persistent [`Trie`]. Readers take a snapshot (a clone of the root pointer, cheap) and read it without holding any
//! lock; one writer at a time builds the next version and publishes it by swapping the root.

use std::any::Any;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use super::trie::Trie;

/// A value found in the store. It keeps the value alive (shared with the trie version it came from) however many writes happen after.
pub struct ValueGuard<T> {
    value: Arc<T>,
}

impl<T> Deref for ValueGuard<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

#[derive(Default)]
pub struct TrieStore {
    // @begin 0a-03
    /// The current version. Held only to clone or replace the pointer, never while reading or building.
    root: Mutex<Trie>,
    /// Makes writers take turns, so that two writes cannot both start from the same version and lose one of the changes.
    write_lock: Mutex<()>,
    //~ _store: (),
    //~ // TODO(0a-03): your fields: the current version of the trie behind a lock, and whatever makes writers take turns (the type must stay Default)
    // @end
}

impl TrieStore {
    pub fn new() -> TrieStore {
        TrieStore::default()
    }

    /// Looks `key` up in the current version.
    pub fn get<T: Any + Send + Sync>(&self, key: &str) -> Option<ValueGuard<T>> {
        // @begin 0a-03
        let snapshot = self.root.lock().unwrap().clone();
        snapshot.get_shared::<T>(key).map(|value| ValueGuard { value })
        //~ todo!("0a-03: take the root lock only to clone the current trie; read from the clone with get_shared; wrap the value in a guard")
        // @end
    }

    /// Stores `value` under `key`. Writers are serialised and readers never wait for them: a reader sees the old trie or the new one, never a partial one.
    pub fn put<T: Any + Send + Sync>(&self, key: &str, value: T) {
        // @begin 0a-03
        let _writer = self.write_lock.lock().unwrap();
        let current = self.root.lock().unwrap().clone();
        let next = current.put(key, value);
        *self.root.lock().unwrap() = next;
        //~ todo!("0a-03: take the write lock for the whole operation; clone the current root; build the new trie without the root lock held; then take the root lock only to publish it")
        // @end
    }

    /// Removes `key`, with the same guarantees as [`put`](Self::put).
    pub fn remove(&self, key: &str) {
        // @begin 0a-03
        let _writer = self.write_lock.lock().unwrap();
        let current = self.root.lock().unwrap().clone();
        let next = current.remove(key);
        *self.root.lock().unwrap() = next;
        //~ todo!("0a-03: as put, with remove")
        // @end
    }
}
