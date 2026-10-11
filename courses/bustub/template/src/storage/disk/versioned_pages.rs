//! A page store whose snapshots read the past.

use std::collections::BTreeMap;

pub type SnapshotId = u64;

pub struct VersionedPages {
    _versions: (),
}

impl VersionedPages {
    /// An empty store.
    pub fn new() -> VersionedPages {
        todo!("1a-c4: an empty store")
    }

    /// Writes a new version of `page`; older versions are kept.
    pub fn write(&mut self, page: u32, value: u64) {
        todo!("1a-c4: add a version, never overwrite")
    }

    /// The newest value of `page`; `None` if it was never written.
    pub fn read(&self, page: u32) -> Option<u64> {
        todo!("1a-c4: the newest value")
    }

    /// Takes a snapshot: a name for the store as it is now, which later writes do not change.
    pub fn snapshot(&mut self) -> SnapshotId {
        todo!("1a-c4: remember how far the store had got")
    }

    /// The value `page` had when `snapshot` was taken: the newest one written no later than it. `None` for an unknown snapshot or a page not yet
    /// written then.
    pub fn read_at(&self, snapshot: SnapshotId, page: u32) -> Option<u64> {
        todo!("1a-c4: the newest value written no later than the snapshot")
    }

    /// Forgets `snapshot`; returns whether it existed.
    pub fn drop_snapshot(&mut self, id: SnapshotId) -> bool {
        todo!("1a-c4: forget the snapshot")
    }
}

impl Default for VersionedPages {
    fn default() -> Self {
        VersionedPages::new()
    }
}
