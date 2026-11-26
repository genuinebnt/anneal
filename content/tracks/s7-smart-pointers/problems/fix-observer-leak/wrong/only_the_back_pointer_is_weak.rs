use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct Hub {
    subscribers: RefCell<Vec<Rc<Subscriber>>>,
}

pub struct Subscriber {
    name: String,
    hub: Weak<Hub>,
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
            hub: Rc::downgrade(hub),
            inbox: RefCell::new(Vec::new()),
        });
        hub.subscribers.borrow_mut().push(subscriber.clone());
        subscriber
    }

    fn deliver(&self, text: &str, except: Option<&Subscriber>) -> usize {
        // Treats "only the hub holds it" as gone, but only notices at the next delivery.
        let mut delivered = 0;
        self.subscribers.borrow_mut().retain(|s| {
            if Rc::strong_count(s) == 1 {
                return false;
            }
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
        let hub = self.hub.upgrade()?;
        Some(hub.deliver(&format!("{}: {text}", self.name), Some(self)))
    }

    pub fn inbox(&self) -> Vec<String> {
        self.inbox.borrow().clone()
    }
}
