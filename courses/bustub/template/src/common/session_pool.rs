//! A bounded pool of sessions.

use std::sync::{Condvar, Mutex};

use crate::common::bustub_instance::BusTubInstance;
use crate::common::session::Session;

pub struct SessionPool<'a> {
    db: &'a BusTubInstance,
    _pool: (),
}


impl<'a> SessionPool<'a> {
    /// An empty pool that may create up to `max` sessions (at least 1).
    pub fn new(db: &'a BusTubInstance, max: usize) -> SessionPool<'a> {
        let _ = (db, max);
        todo!("4e-c5: an empty pool that may create up to `max` sessions")
    }

    /// Sessions that exist.
    pub fn created(&self) -> usize {
        todo!("4e-c5: how many sessions were created")
    }

    /// Sessions waiting to be lent.
    pub fn idle(&self) -> usize {
        todo!("4e-c5: how many sessions are waiting")
    }

    /// Lends a session to `f`, then resets it and takes it back.
    pub fn with<R>(&self, f: impl FnOnce(&mut Session<'a>) -> R) -> R {
        let _ = f;
        todo!("4e-c5: take an idle session (or create one, or wait); run f; reset the session; give it back and wake a waiter")
    }
}
