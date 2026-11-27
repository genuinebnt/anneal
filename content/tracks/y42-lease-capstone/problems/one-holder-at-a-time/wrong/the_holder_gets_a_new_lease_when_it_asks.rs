// ---------------------------------------------------------------------------------------------------------------------
// The lease service (capstone step 1)
// ---------------------------------------------------------------------------------------------------------------------

/// Permission to be the one writer until a time, and the number of this grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lease {
    pub token: u64,
    pub expires_at: u64,
}

/// Grants one lease at a time, going by its own clock.
pub struct LeaseService {
    ttl_ms: u64,
    /// The latest time any call has named.
    now: u64,
    /// Grants made so far: the last token.
    granted: u64,
    /// The latest grant and who got it. It may have run out.
    last: Option<(u32, Lease)>,
}

impl LeaseService {
    pub fn new(ttl_ms: u64) -> LeaseService {
        assert!(ttl_ms > 0, "a lease must last for some time");
        LeaseService {
            ttl_ms,
            now: 0,
            granted: 0,
            last: None,
        }
    }

    /// Moves the clock, never backwards, and says whose lease is live.
    fn live(&mut self, now: u64) -> Option<(u32, Lease)> {
        self.now = self.now.max(now);
        self.last.filter(|(_, lease)| self.now < lease.expires_at)
    }

    fn lease_from_now(&self, token: u64) -> Lease {
        Lease {
            token,
            expires_at: self.now.saturating_add(self.ttl_ms),
        }
    }

    /// A new lease for `client`, unless somebody's lease is live.
    pub fn acquire(&mut self, now: u64, client: u32) -> Option<Lease> {
        if self.live(now).is_some_and(|(holder, _)| holder != client) {
            return None;
        }
        self.granted += 1;
        let lease = self.lease_from_now(self.granted);
        self.last = Some((client, lease));
        Some(lease)
    }

    /// Makes the live lease last longer, if `client` holds it under `token`.
    pub fn renew(&mut self, now: u64, client: u32, token: u64) -> Option<Lease> {
        let (holder, held) = self.live(now)?;
        if holder != client || held.token != token {
            return None;
        }
        let lease = self.lease_from_now(token);
        self.last = Some((client, lease));
        Some(lease)
    }

    /// Who holds a live lease.
    pub fn holder(&mut self, now: u64) -> Option<u32> {
        self.live(now).map(|(client, _)| client)
    }
}
