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

/// One backend's slot in shared memory: a much-reduced PGPROC. 248 bytes.
#[derive(Clone)]
struct Proc {
    connected: bool,
    pid: u32,
    database: u32,
    xid: u32,
    xmin: u32,
    wait_event: u32,
    backend_start: u64,
    xact_start: u64,
    app_len: u8,
    application_name: [u8; 64],
    client_addr: [u8; 16],
    query_len: u8,
    query: [u8; 128],
}

const EMPTY: Proc = Proc {
    connected: false,
    pid: 0,
    database: 0,
    xid: 0,
    xmin: 0,
    wait_event: 0,
    backend_start: 0,
    xact_start: 0,
    app_len: 0,
    application_name: [0; 64],
    client_addr: [0; 16],
    query_len: 0,
    query: [0; 128],
};

/// Every backend slot. Transaction ids are `u32`s from 1 up (0 means none), and never wrap here.
pub struct ProcArray {
    procs: Vec<Proc>,
}

impl ProcArray {
    pub fn new(max_backends: usize) -> ProcArray {
        ProcArray {
            procs: vec![EMPTY; max_backends],
        }
    }

    /// A backend attaches to a free `slot`. `application` is cut to 64 bytes.
    pub fn connect(&mut self, slot: usize, pid: u32, database: u32, application: &str, now: u64) {
        let app = truncate(application, 64);
        let mut p = Proc {
            connected: true,
            pid,
            database,
            backend_start: now,
            app_len: app.len() as u8,
            ..EMPTY
        };
        p.application_name[..app.len()].copy_from_slice(app.as_bytes());
        self.procs[slot] = p;
    }

    /// The backend leaves; its transaction, if any, ends with it.
    pub fn disconnect(&mut self, slot: usize) {
        self.procs[slot] = EMPTY;
    }

    /// The backend starts a transaction with id `xid` (at least 1).
    pub fn begin(&mut self, slot: usize, xid: u32, now: u64) {
        let p = &mut self.procs[slot];
        p.xid = xid;
        p.xact_start = now;
    }

    /// The backend takes a snapshot that needs every xid from `xmin` on.
    pub fn set_xmin(&mut self, slot: usize, xmin: u32) {
        self.procs[slot].xmin = xmin;
    }

    /// Commit or abort: the backend has no transaction and needs no snapshot.
    pub fn end(&mut self, slot: usize) {
        let p = &mut self.procs[slot];
        p.xid = 0;
        p.xmin = 0;
        p.xact_start = 0;
    }

    /// Records the statement the backend is running, cut to 128 bytes.
    pub fn set_query(&mut self, slot: usize, query: &str) {
        let q = truncate(query, 128);
        let p = &mut self.procs[slot];
        p.query[..q.len()].copy_from_slice(q.as_bytes());
        p.query_len = q.len() as u8;
    }

    pub fn pid(&self, slot: usize) -> Option<u32> {
        let p = &self.procs[slot];
        p.connected.then_some(p.pid)
    }

    pub fn application(&self, slot: usize) -> Option<&str> {
        let p = &self.procs[slot];
        p.connected
            .then(|| std::str::from_utf8(&p.application_name[..p.app_len as usize]).unwrap())
    }

    pub fn query(&self, slot: usize) -> Option<&str> {
        let p = &self.procs[slot];
        p.connected
            .then(|| std::str::from_utf8(&p.query[..p.query_len as usize]).unwrap())
    }

    /// When the backend's running transaction started, if it has one.
    pub fn xact_start(&self, slot: usize) -> Option<u64> {
        let p = &self.procs[slot];
        (p.xid != 0).then_some(p.xact_start)
    }

    /// How many backends are connected.
    pub fn connected(&self) -> usize {
        self.procs.iter().filter(|p| p.connected).count()
    }

    /// The oldest xid a connected backend still needs (vacuum keeps every row version newer than this):
    /// the smallest running xid or xmin, or `next_xid` if that's smaller.
    pub fn oldest_xmin(&self, next_xid: u32) -> u32 {
        let mut oldest = next_xid;
        for p in &self.procs {
            if !p.connected {
                continue;
            }
            if p.xid != 0 && p.xid < oldest {
                oldest = p.xid;
            }
            if p.xmin != 0 && p.xmin < oldest {
                oldest = p.xmin;
            }
        }
        oldest
    }

    /// GetSnapshotData: clears `xip` and fills it with the running xids in ascending order; returns the
    /// snapshot's xmin, the smallest of them, or `next_xid` when none is running.
    pub fn snapshot(&self, next_xid: u32, xip: &mut Vec<u32>) -> u32 {
        xip.clear();
        for p in &self.procs {
            if p.connected && p.xid != 0 {
                xip.push(p.xid);
            }
        }
        xip.sort_unstable();
        xip.first().copied().unwrap_or(next_xid).min(next_xid)
    }
}
