use std::cell::RefCell;

pub struct Bus {
    listeners: RefCell<Vec<Box<dyn Fn(&Bus, &str)>>>,
    log: RefCell<Vec<String>>,
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            listeners: RefCell::new(Vec::new()),
            log: RefCell::new(Vec::new()),
        }
    }

    pub fn subscribe(&self, f: impl Fn(&Bus, &str) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    /// Logs `event`, then calls every listener subscribed so far, in order. A listener may emit more events
    /// (each handled completely, right away) and may subscribe new listeners, which hear only later events.
    pub fn emit(&self, event: &str) {
        self.log.borrow_mut().push(event.to_string());
        for l in self.listeners.borrow().iter() {
            l(self, event);
        }
    }

    /// Emits again, in order, every event logged before this call.
    pub fn replay(&self) {
        for e in self.log.borrow().iter() {
            self.emit(e);
        }
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().to_vec()
    }
}
