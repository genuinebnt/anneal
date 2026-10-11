//! A page store whose snapshots read the past.

use std::collections::BTreeMap;

pub type SnapshotId = u64;

pub struct VersionedPages {
    // @begin 1a-c4
    /// page -> (version number, value), in increasing version order.
    versions: BTreeMap<u32, Vec<(u64, u64)>>,
    clock: u64,
    next_id: SnapshotId,
    /// snapshot id -> the clock when it was taken.
    snapshots: BTreeMap<SnapshotId, u64>,
    //~ _versions: (),
    // @end
}

impl VersionedPages {
    /// An empty store.
    pub fn new() -> VersionedPages {
        // @begin 1a-c4
        VersionedPages { versions: BTreeMap::new(), clock: 0, next_id: 0, snapshots: BTreeMap::new() }
        //~ todo!("1a-c4: an empty store")
        // @end
    }

    /// Writes a new version of `page`; older versions are kept.
    pub fn write(&mut self, page: u32, value: u64) {
        // @begin 1a-c4
        self.clock += 1;
        self.versions.entry(page).or_default().push((self.clock, value));
        //~ todo!("1a-c4: add a version, never overwrite")
        // @end
    }

    /// The newest value of `page`; `None` if it was never written.
    pub fn read(&self, page: u32) -> Option<u64> {
        // @begin 1a-c4
        self.versions.get(&page).and_then(|v| v.last()).map(|&(_, x)| x)
        //~ todo!("1a-c4: the newest value")
        // @end
    }

    /// Takes a snapshot: a name for the store as it is now, which later writes do not change.
    pub fn snapshot(&mut self) -> SnapshotId {
        // @begin 1a-c4
        let id = self.next_id;
        self.next_id += 1;
        self.snapshots.insert(id, self.clock);
        id
        //~ todo!("1a-c4: remember how far the store had got")
        // @end
    }

    /// The value `page` had when `snapshot` was taken: the newest one written no later than it. `None` for an unknown snapshot or a page not yet
    /// written then.
    pub fn read_at(&self, snapshot: SnapshotId, page: u32) -> Option<u64> {
        // @begin 1a-c4
        let at = *self.snapshots.get(&snapshot)?;
        self.versions.get(&page)?.iter().rev().find(|&&(ts, _)| ts <= at).map(|&(_, x)| x)
        //~ todo!("1a-c4: the newest value written no later than the snapshot")
        // @end
    }

    /// Forgets `snapshot`; returns whether it existed.
    pub fn drop_snapshot(&mut self, id: SnapshotId) -> bool {
        // @begin 1a-c4
        self.snapshots.remove(&id).is_some()
        //~ todo!("1a-c4: forget the snapshot")
        // @end
    }
}

impl Default for VersionedPages {
    fn default() -> Self {
        VersionedPages::new()
    }
}
