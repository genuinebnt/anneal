use std::cell::RefCell;
use std::rc::Rc;

pub struct Hub {
    subscribers: RefCell<Vec<Rc<Subscriber>>>,
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
        hub.subscribers.borrow_mut().push(subscriber.clone());
        subscriber
    }

    pub fn publish(&self, text: &str) -> usize {
        let subscribers = self.subscribers.borrow();
        for subscriber in subscribers.iter() {
            subscriber.inbox.borrow_mut().push(text.to_string());
        }
        subscribers.len()
    }

    pub fn entries(&self) -> usize {
        self.subscribers.borrow().len()
    }
}

impl Subscriber {
    pub fn post(&self, text: &str) -> Option<usize> {
        let message = format!("{}: {text}", self.name);
        let mut delivered = 0;
        for other in self.hub.subscribers.borrow().iter() {
            if !std::ptr::eq(other.as_ref(), self) {
                other.inbox.borrow_mut().push(message.clone());
                delivered += 1;
            }
        }
        Some(delivered)
    }

    pub fn inbox(&self) -> Vec<String> {
        self.inbox.borrow().clone()
    }
}
