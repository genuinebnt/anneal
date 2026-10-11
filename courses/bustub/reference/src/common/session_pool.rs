//! A bounded pool of sessions.

use std::sync::{Condvar, Mutex};

use crate::common::bustub_instance::BusTubInstance;
use crate::common::session::Session;

pub struct SessionPool<'a> {
    db: &'a BusTubInstance,
    // @begin 4e-c5
    max: usize,
    state: Mutex<PoolState<'a>>,
    freed: Condvar,
    //~ _pool: (),
    // @end
}

// @begin 4e-c5
struct PoolState<'a> {
    idle: Vec<Session<'a>>,
    created: usize,
}
// @end

impl<'a> SessionPool<'a> {
    /// An empty pool that may create up to `max` sessions (at least 1).
    pub fn new(db: &'a BusTubInstance, max: usize) -> SessionPool<'a> {
        // @begin 4e-c5
        SessionPool { db, max: max.max(1), state: Mutex::new(PoolState { idle: Vec::new(), created: 0 }), freed: Condvar::new() }
        //~ let _ = (db, max);
        //~ todo!("4e-c5: an empty pool that may create up to `max` sessions")
        // @end
    }

    /// Sessions that exist.
    pub fn created(&self) -> usize {
        // @begin 4e-c5
        self.state.lock().unwrap().created
        //~ todo!("4e-c5: how many sessions were created")
        // @end
    }

    /// Sessions waiting to be lent.
    pub fn idle(&self) -> usize {
        // @begin 4e-c5
        self.state.lock().unwrap().idle.len()
        //~ todo!("4e-c5: how many sessions are waiting")
        // @end
    }

    /// Lends a session to `f`, then resets it and takes it back.
    pub fn with<R>(&self, f: impl FnOnce(&mut Session<'a>) -> R) -> R {
        // @begin 4e-c5
        let mut session = {
            let mut st = self.state.lock().unwrap();
            loop {
                if let Some(s) = st.idle.pop() {
                    break s;
                }
                if st.created < self.max {
                    st.created += 1;
                    break Session::new(self.db);
                }
                st = self.freed.wait(st).unwrap();
            }
        };
        let result = f(&mut session);
        // reset: nobody may inherit a transaction or a setting
        if session.in_transaction() {
            let _ = session.execute("rollback");
        }
        let _ = session.execute("set default_transaction_isolation = 'snapshot'");
        self.state.lock().unwrap().idle.push(session);
        self.freed.notify_one();
        result
        //~ let _ = f;
        //~ todo!("4e-c5: take an idle session (or create one, or wait); run f; reset the session; give it back and wake a waiter")
        // @end
    }
}
