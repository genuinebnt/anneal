//! RECIPE 80 (async): `async fn` in traits, why `dyn` needs boxing, and an adapter that makes any impl object-safe.
//! LEVEL: advanced
//! Use in: swappable async backends (storage, queue, clock) you want to fake in tests or choose at runtime.   Run: cargo run --release --example async_traits_dyn
//! COMMON USES: swappable async backends: storage/queue/clock traits with fakes in tests and dyn dispatch at runtime.
#![allow(dead_code)]
//!
//! `async fn` in traits is stable, and returns an opaque `impl Future`. That is great for GENERIC use (`S: Store`, static
//! dispatch, zero cost) but it makes the trait NOT dyn-compatible: a vtable needs one concrete return type. For `dyn` you
//! return `Pin<Box<dyn Future + Send + 'a>>` (one allocation per call). A blanket impl can derive the boxed `DynStore` from
//! any `Store`, so you write the nice async version once. Spelling the future as `impl Future + Send` in the trait lets
//! generic code (and `tokio::spawn`) require that the future is Send.
//! Crates automating this: https://docs.rs/async-trait (boxes everything) and https://docs.rs/trait-variant
//! Read: https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

trait Store: Send + Sync {
    fn get(&self, key: &str) -> impl Future<Output = Option<String>> + Send;
}

struct MemStore(HashMap<String, String>);
struct SlowStore;

impl Store for MemStore {
    async fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}
impl Store for SlowStore {
    async fn get(&self, key: &str) -> Option<String> {
        tokio::time::sleep(Duration::from_millis(5)).await;
        Some(format!("slow:{key}"))
    }
}

// generic: monomorphised per store type, no boxing
async fn lookup_all<S: Store>(store: &S, keys: &[&str]) -> Vec<Option<String>> {
    let mut out = Vec::new();
    for k in keys {
        out.push(store.get(k).await);
    }
    out
}

// dyn-compatible twin of the trait
trait DynStore: Send + Sync {
    fn get<'a>(&'a self, key: &'a str)
    -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;
}
// every Store automatically is a DynStore
impl<T: Store> DynStore for T {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>> {
        Box::pin(Store::get(self, key))
    }
}

// runtime choice and heterogeneous collections need dyn
struct Router {
    backends: Vec<(&'static str, Arc<dyn DynStore>)>,
}
impl Router {
    async fn first_hit(&self, key: &str) -> Option<(&'static str, String)> {
        for (name, b) in &self.backends {
            if let Some(v) = b.get(key).await {
                return Some((name, v));
            }
        }
        None
    }
}

#[tokio::main]
async fn main() {
    let mem = MemStore(HashMap::from([("a".to_string(), "alpha".to_string())]));
    println!("{:?}", lookup_all(&mem, &["a", "b"]).await);
    println!("{:?}", lookup_all(&SlowStore, &["x"]).await);

    let router = Router {
        backends: vec![("mem", Arc::new(mem)), ("slow", Arc::new(SlowStore))],
    };
    println!("{:?}", router.first_hit("a").await);
    println!("{:?}", router.first_hit("zzz").await);

    // Send-ness lets the future cross tasks:
    let router = Arc::new(router);
    let r2 = router.clone();
    let handle = tokio::spawn(async move { r2.first_hit("a").await });
    println!("{:?}", handle.await.unwrap());
}
