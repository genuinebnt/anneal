//! BLOCK 8: in-process cache with TTL, singleflight and tag invalidation.
//!
//! * singleflight: N concurrent misses for a key run the loader ONCE (no cache stampede);
//! * tags: writers invalidate "project:42" without knowing which keys depend on it. Each tag has a
//!   generation number; an entry remembers the generations it was loaded under and is stale if any moved.
//!
//! The subtle race: a loader reads the DB, a write commits and invalidates, THEN the loader stores its
//! now-stale result. Generations are therefore captured BEFORE loading and re-checked before storing,
//! and a caller arriving after the invalidation does not join the older flight.
//! Pitfalls: cache keys MUST include the tenant id; this cache is per process, so with several replicas an
//! invalidation must be broadcast (or rely on a short TTL). Production alternatives: `moka`, Redis.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::sync::OnceCell;

type Stamps = Vec<(String, u64)>;

struct Entry<V> {
    value: V,
    expires_ms: i64,
    stamps: Stamps,
}

struct Flight<V> {
    stamps: Stamps,
    cell: Arc<OnceCell<V>>,
}

struct Inner<V> {
    entries: HashMap<String, Entry<V>>,
    flights: HashMap<String, Flight<V>>,
    generations: HashMap<String, u64>,
}

impl<V> Inner<V> {
    fn stamps(&self, tags: &[&str]) -> Stamps {
        tags.iter().map(|t| (t.to_string(), self.generations.get(*t).copied().unwrap_or(0))).collect()
    }

    fn is_current(&self, stamps: &Stamps) -> bool {
        stamps.iter().all(|(t, g)| self.generations.get(t).copied().unwrap_or(0) == *g)
    }
}

pub struct TagCache<V> {
    ttl_ms: i64,
    capacity: usize,
    inner: Mutex<Inner<V>>,
}

impl<V: Clone + Send + Sync> TagCache<V> {
    #[must_use]
    pub fn new(ttl_ms: i64, capacity: usize) -> Self {
        let inner = Inner { entries: HashMap::new(), flights: HashMap::new(), generations: HashMap::new() };
        Self { ttl_ms, capacity: capacity.max(1), inner: Mutex::new(inner) }
    }

    pub async fn get_or_load<E, F, Fut>(&self, key: &str, tags: &[&str], now_ms: i64, load: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        // The std mutex is only held inside this block, never across an await.
        let (cell, stamps) = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(e) = inner.entries.get(key)
                && now_ms < e.expires_ms
                && inner.is_current(&e.stamps)
            {
                return Ok(e.value.clone());
            }
            let stamps = inner.stamps(tags);
            match inner.flights.get(key) {
                Some(f) if f.stamps == stamps => (f.cell.clone(), stamps),
                _ => {
                    let cell = Arc::new(OnceCell::new());
                    inner.flights.insert(key.to_string(), Flight { stamps: stamps.clone(), cell: cell.clone() });
                    (cell, stamps)
                }
            }
        };
        // If the initialising caller is cancelled or fails, `OnceCell` lets the next waiter run its loader.
        let result = cell.get_or_try_init(load).await.cloned();

        let mut inner = self.inner.lock().unwrap();
        if inner.flights.get(key).is_some_and(|f| Arc::ptr_eq(&f.cell, &cell)) {
            inner.flights.remove(key);
        }
        if let Ok(value) = &result
            && inner.is_current(&stamps)
        {
            if inner.entries.len() >= self.capacity && !inner.entries.contains_key(key) {
                // Crude eviction (soonest to expire). A real cache uses LRU or TinyLFU: see `moka`.
                let victim = inner.entries.iter().min_by_key(|(_, e)| e.expires_ms).map(|(k, _)| k.clone());
                if let Some(v) = victim {
                    inner.entries.remove(&v);
                }
            }
            let entry = Entry { value: value.clone(), expires_ms: now_ms.saturating_add(self.ttl_ms), stamps };
            inner.entries.insert(key.to_string(), entry);
        }
        result
    }

    /// Call AFTER the write commits: invalidating before lets a reader re-cache the old row.
    pub fn invalidate(&self, tag: &str) {
        *self.inner.lock().unwrap().generations.entry(tag.to_string()).or_insert(0) += 1;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::Notify;

    async fn load(c: &TagCache<u32>, key: &str, now: i64, v: u32) -> u32 {
        c.get_or_load(key, &["t"], now, || async move { Ok::<_, Infallible>(v) }).await.unwrap()
    }

    #[tokio::test]
    async fn hits_until_ttl_or_invalidation() {
        let c = TagCache::new(100, 10);
        assert_eq!(load(&c, "k", 0, 1).await, 1);
        assert_eq!(load(&c, "k", 99, 2).await, 1, "cached");
        assert_eq!(load(&c, "k", 100, 3).await, 3, "expired");
        c.invalidate("t");
        assert_eq!(load(&c, "k", 101, 4).await, 4, "invalidated");
        c.invalidate("unrelated");
        assert_eq!(load(&c, "k", 102, 5).await, 4);
    }

    #[tokio::test]
    async fn concurrent_misses_load_once() {
        let c = Arc::new(TagCache::<u32>::new(1_000, 10));
        let (calls, gate) = (Arc::new(AtomicU32::new(0)), Arc::new(Notify::new()));
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let (c, calls, gate) = (c.clone(), calls.clone(), gate.clone());
            set.spawn(async move {
                c.get_or_load("k", &["t"], 0, || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    gate.notified().await;
                    Ok::<_, Infallible>(7)
                })
                .await
                .unwrap()
            });
        }
        // Single-threaded test runtime: yielding lets every task reach the cell before the gate opens.
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        gate.notify_one();
        while let Some(r) = set.join_next().await {
            assert_eq!(r.unwrap(), 7);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn result_loaded_across_an_invalidation_is_not_cached() {
        let c = TagCache::new(1_000, 10);
        let stale = c
            .get_or_load("k", &["t"], 0, || async {
                c.invalidate("t"); // a write commits while we are loading
                Ok::<_, Infallible>(1)
            })
            .await
            .unwrap();
        assert_eq!(stale, 1);
        assert!(c.is_empty());
        assert_eq!(load(&c, "k", 0, 2).await, 2);
    }

    #[tokio::test]
    async fn errors_are_not_cached_and_capacity_is_bounded() {
        let c = TagCache::<u32>::new(1_000, 3);
        let r: Result<u32, &str> = c.get_or_load("k", &[], 0, || async { Err("db down") }).await;
        assert_eq!(r, Err("db down"));
        assert_eq!(load(&c, "k", 0, 9).await, 9);
        for i in 0..10 {
            load(&c, &format!("k{i}"), i, 0).await;
        }
        assert_eq!(c.len(), 3);
    }
}
