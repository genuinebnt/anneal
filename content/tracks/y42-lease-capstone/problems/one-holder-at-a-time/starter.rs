/// Permission to be the one writer until a time, and the number of this grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lease {
    pub token: u64,
    pub expires_at: u64,
}

/// Grants one lease at a time, going by its own clock.
pub struct LeaseService {
    // TODO: the fields
}

impl LeaseService {
    pub fn new(ttl_ms: u64) -> LeaseService {
        todo!()
    }

    /// A new lease for `client`, unless somebody's lease is live.
    pub fn acquire(&mut self, now: u64, client: u32) -> Option<Lease> {
        todo!()
    }

    /// Makes the live lease last longer, if `client` holds it under `token`.
    pub fn renew(&mut self, now: u64, client: u32, token: u64) -> Option<Lease> {
        todo!()
    }

    /// Who holds a live lease.
    pub fn holder(&mut self, now: u64) -> Option<u32> {
        todo!()
    }
}
