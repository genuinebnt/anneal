//! Shared test doubles: an object store that can be paused or made to fail.

use std::sync::Arc;

use async_trait::async_trait;
use backend_ref::objstore::{MemoryStore, ObjectStore, ObjectWriter, StoreError};
use bytes::Bytes;
use tokio::sync::Semaphore;

/// `get` signals `entered`, then waits for a permit on `release`. This holds a request "in flight" for
/// exactly as long as a test wants, with no sleeps.
pub struct GatedStore {
    pub inner: MemoryStore,
    pub entered: Semaphore,
    pub release: Semaphore,
}

impl GatedStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { inner: MemoryStore::default(), entered: Semaphore::new(0), release: Semaphore::new(0) })
    }

    #[allow(dead_code)] // each test binary uses a different part of this module
    pub async fn wait_until_entered(&self) {
        self.entered.acquire().await.unwrap().forget();
    }
}

#[async_trait]
impl ObjectStore for GatedStore {
    async fn begin_put(&self, key: &str) -> Result<Box<dyn ObjectWriter>, StoreError> {
        self.inner.begin_put(key).await
    }
    async fn get(&self, key: &str) -> Result<Bytes, StoreError> {
        self.entered.add_permits(1);
        self.release.acquire().await.unwrap().forget();
        self.inner.get(key).await
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner.delete(key).await
    }
}
