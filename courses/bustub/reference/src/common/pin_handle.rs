//! Pins counted by the handles that exist.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct Pins {
    // @begin 1g-c2
    count: Arc<AtomicUsize>,
    //~ _pins: (),
    // @end
}

pub struct PinHandle {
    // @begin 1g-c2
    count: Arc<AtomicUsize>,
    //~ _handle: (),
    // @end
}

impl Pins {
    /// A count with no live handle.
    pub fn new() -> Pins {
        // @begin 1g-c2
        Pins { count: Arc::new(AtomicUsize::new(0)) }
        //~ todo!("1g-c2: nothing is pinned")
        // @end
    }

    /// Counts one more pin and returns its handle; the pin ends when the handle is dropped.
    pub fn pin(&self) -> PinHandle {
        // @begin 1g-c2
        self.count.fetch_add(1, Ordering::SeqCst);
        PinHandle { count: Arc::clone(&self.count) }
        //~ todo!("1g-c2: count one more pin and give out a handle")
        // @end
    }

    /// The number of live handles.
    pub fn count(&self) -> usize {
        // @begin 1g-c2
        self.count.load(Ordering::SeqCst)
        //~ todo!("1g-c2: the live handles")
        // @end
    }
}

impl Default for Pins {
    fn default() -> Self {
        Pins::new()
    }
}

impl Clone for PinHandle {
    fn clone(&self) -> PinHandle {
        // @begin 1g-c2
        self.count.fetch_add(1, Ordering::SeqCst);
        PinHandle { count: Arc::clone(&self.count) }
        //~ todo!("1g-c2: a clone is another pin")
        // @end
    }
}

impl Drop for PinHandle {
    fn drop(&mut self) {
        // @begin 1g-c2
        self.count.fetch_sub(1, Ordering::SeqCst);
        //~ // TODO(1g-c2): uncount this pin
        // @end
    }
}
