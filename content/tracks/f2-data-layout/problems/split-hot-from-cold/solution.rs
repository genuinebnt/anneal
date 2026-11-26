/// The longest prefix of `s` that fits in `max` bytes without splitting a character.
fn truncate(s: &str, max: usize) -> &str {
    let end = s
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .take_while(|&e| e <= max)
        .last()
        .unwrap_or(0);
    &s[..end]
}

/// What every snapshot and vacuum scans: 8 bytes a backend, 8 backends a cache line. A slot with no
/// backend or no transaction holds zeros, so the scans never need to look at the cold part.
#[derive(Clone, Copy, Default)]
struct Hot {
    xid: u32,
    xmin: u32,
}

/// Everything else about a backend, read only when someone asks about that backend.
#[derive(Clone)]
struct Cold {
    connected: bool,
    pid: u32,
    database: u32,
    wait_event: u32,
    backend_start: u64,
    xact_start: u64,
    app_len: u8,
    application_name: [u8; 64],
    client_addr: [u8; 16],
    query_len: u8,
    query: [u8; 128],
}

const EMPTY: Cold = Cold {
    connected: false,
    pid: 0,
    database: 0,
    wait_event: 0,
    backend_start: 0,
    xact_start: 0,
    app_len: 0,
    application_name: [0; 64],
    client_addr: [0; 16],
    query_len: 0,
    query: [0; 128],
};

/// Every backend slot, split by access pattern: `hot[slot]` and `cold[slot]` describe the same backend.
/// Transaction ids are `u32`s from 1 up (0 means none), and never wrap here.
pub struct ProcArray {
    hot: Vec<Hot>,
    cold: Vec<Cold>,
}

impl ProcArray {
    pub fn new(max_backends: usize) -> ProcArray {
        ProcArray {
            hot: vec![Hot::default(); max_backends],
            cold: vec![EMPTY; max_backends],
        }
    }

    /// A backend attaches to a free `slot`. `application` is cut to 64 bytes.
    pub fn connect(&mut self, slot: usize, pid: u32, database: u32, application: &str, now: u64) {
        let app = truncate(application, 64);
        let mut c = Cold {
            connected: true,
            pid,
            database,
            backend_start: now,
            app_len: app.len() as u8,
            ..EMPTY
        };
        c.application_name[..app.len()].copy_from_slice(app.as_bytes());
        self.cold[slot] = c;
        self.hot[slot] = Hot::default();
    }

    /// The backend leaves; its transaction, if any, ends with it.
    pub fn disconnect(&mut self, slot: usize) {
        self.cold[slot] = EMPTY;
        self.hot[slot] = Hot::default();
    }

    /// The backend starts a transaction with id `xid` (at least 1).
    pub fn begin(&mut self, slot: usize, xid: u32, now: u64) {
        self.hot[slot].xid = xid;
        self.cold[slot].xact_start = now;
    }

    /// The backend takes a snapshot that needs every xid from `xmin` on.
    pub fn set_xmin(&mut self, slot: usize, xmin: u32) {
        self.hot[slot].xmin = xmin;
    }

    /// Commit or abort: the backend has no transaction and needs no snapshot.
    pub fn end(&mut self, slot: usize) {
        self.hot[slot] = Hot { xid: 0, xmin: 0 };
        self.cold[slot].xact_start = 0;
    }

    /// Records the statement the backend is running, cut to 128 bytes.
    pub fn set_query(&mut self, slot: usize, query: &str) {
        let q = truncate(query, 128);
        let c = &mut self.cold[slot];
        c.query[..q.len()].copy_from_slice(q.as_bytes());
        c.query_len = q.len() as u8;
    }

    pub fn pid(&self, slot: usize) -> Option<u32> {
        let c = &self.cold[slot];
        c.connected.then_some(c.pid)
    }

    pub fn application(&self, slot: usize) -> Option<&str> {
        let c = &self.cold[slot];
        c.connected
            .then(|| std::str::from_utf8(&c.application_name[..c.app_len as usize]).unwrap())
    }

    pub fn query(&self, slot: usize) -> Option<&str> {
        let c = &self.cold[slot];
        c.connected
            .then(|| std::str::from_utf8(&c.query[..c.query_len as usize]).unwrap())
    }

    /// When the backend's running transaction started, if it has one.
    pub fn xact_start(&self, slot: usize) -> Option<u64> {
        (self.hot[slot].xid != 0).then_some(self.cold[slot].xact_start)
    }

    /// How many backends are connected.
    pub fn connected(&self) -> usize {
        self.cold.iter().filter(|c| c.connected).count()
    }

    /// The oldest xid a connected backend still needs (vacuum keeps every row version newer than this):
    /// the smallest running xid or xmin, or `next_xid` if that's smaller.
    pub fn oldest_xmin(&self, next_xid: u32) -> u32 {
        // Idle and disconnected slots hold 0, which wraps to u32::MAX and so never wins; no branch, and nothing cold is read.
        let least = self.hot.iter().fold(u32::MAX, |m, h| {
            m.min(h.xid.wrapping_sub(1)).min(h.xmin.wrapping_sub(1))
        });
        least.saturating_add(1).min(next_xid)
    }

    /// GetSnapshotData: clears `xip` and fills it with the running xids in ascending order; returns the
    /// snapshot's xmin, the smallest of them, or `next_xid` when none is running.
    pub fn snapshot(&self, next_xid: u32, xip: &mut Vec<u32>) -> u32 {
        xip.clear();
        xip.extend(self.hot.iter().map(|h| h.xid).filter(|&x| x != 0));
        xip.sort_unstable();
        xip.first().copied().unwrap_or(next_xid).min(next_xid)
    }
}
