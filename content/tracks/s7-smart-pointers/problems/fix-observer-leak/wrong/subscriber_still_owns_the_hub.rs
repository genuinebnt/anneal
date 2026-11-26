use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct Hub {
    subscribers: RefCell<Vec<Weak<Subscriber>>>,
}

pub struct Subscriber {
    name: String,
    hub: Rc<Hub>,
    inbox: RefCell<Vec<String>>,
}

impl Hub {
    pub fn new() -> Rc<Hub> {
        Rc::new(Hub {
            subscribers: RefCell::new(Vec::new()),
        })
    }

    pub fn subscribe(hub: &Rc<Hub>, name: &str) -> Rc<Subscriber> {
        let subscriber = Rc::new(Subscriber {
            name: name.to_string(),
            hub: hub.clone(),
            inbox: RefCell::new(Vec::new()),
        });
        hub.subscribers
            .borrow_mut()
            .push(Rc::downgrade(&subscriber));
        subscriber
    }

    fn deliver(&self, text: &str, except: Option<&Subscriber>) -> usize {
        let mut delivered = 0;
        self.subscribers.borrow_mut().retain(|entry| {
            let Some(s) = entry.upgrade() else {
                return false;
            };
            if !except.is_some_and(|e| std::ptr::eq(e, s.as_ref())) {
                s.inbox.borrow_mut().push(text.to_string());
                delivered += 1;
            }
            true
        });
        delivered
    }

    pub fn publish(&self, text: &str) -> usize {
        self.deliver(text, None)
    }

    pub fn entries(&self) -> usize {
        self.subscribers.borrow().len()
    }
}

impl Subscriber {
    pub fn post(&self, text: &str) -> Option<usize> {
        Some(
            self.hub
                .deliver(&format!("{}: {text}", self.name), Some(self)),
        )
    }

    pub fn inbox(&self) -> Vec<String> {
        self.inbox.borrow().clone()
    }
}
