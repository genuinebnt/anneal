//! RECIPE 96 (advanced): a type-safe event bus built on `TypeId` and `dyn Any`: runtime dispatch with compile-time-typed handlers.
//! LEVEL: advanced
//! Use in: plugin/event systems, ECS-style registries, "service locators", anywhere values of unrelated types share one container.   Run: cargo run --release --example any_event_bus
//! COMMON USES: plugin systems, UI/event buses, ECS resources (Bevy), service locators.
#![allow(dead_code)]
//!
//! `Any` gives every `'static` type a runtime `TypeId` and lets you downcast `&dyn Any` back to the concrete type. The bus stores,
//! per event TYPE, a list of type-erased handlers. `subscribe::<E>(closure taking &E)` wraps the typed closure in a function over
//! `&dyn Any` that downcasts first; `publish(&event)` looks up handlers by `TypeId::of::<E>()`. Callers never see `Any`.
//! Combines generics, closures, trait objects, `'static` bounds and interior mutability.  Real use: https://docs.rs/anymap ,
//! bevy's resource/event registries https://github.com/bevyengine/bevy , and `Box<dyn Any>` in `std::panic::catch_unwind` payloads.
use std::any::{Any, TypeId, type_name};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type Handler = Box<dyn Fn(&dyn Any)>;

#[derive(Default)]
struct Bus {
    handlers: HashMap<TypeId, Vec<Handler>>,
}

impl Bus {
    fn subscribe<E: 'static>(&mut self, f: impl Fn(&E) + 'static) {
        let erased: Handler = Box::new(move |any: &dyn Any| {
            // always succeeds because we only register under TypeId::of::<E>(), but never `unwrap` blindly in library code
            if let Some(e) = any.downcast_ref::<E>() {
                f(e);
            }
        });
        self.handlers
            .entry(TypeId::of::<E>())
            .or_default()
            .push(erased);
    }

    /// returns how many handlers received the event
    fn publish<E: 'static>(&self, event: &E) -> usize {
        match self.handlers.get(&TypeId::of::<E>()) {
            Some(hs) => {
                hs.iter().for_each(|h| h(event));
                hs.len()
            }
            None => 0,
        }
    }
}

#[derive(Debug)]
struct UserCreated {
    id: u32,
}
#[derive(Debug)]
struct OrderPaid {
    order: &'static str,
    cents: u64,
}

// A heterogeneous "type map": one value per type, e.g. app-wide services.
#[derive(Default)]
struct Registry {
    items: HashMap<TypeId, Box<dyn Any>>,
}
impl Registry {
    fn insert<T: 'static>(&mut self, v: T) -> Option<T> {
        self.items
            .insert(TypeId::of::<T>(), Box::new(v))
            .and_then(|old| old.downcast::<T>().ok().map(|b| *b))
    }
    fn get<T: 'static>(&self) -> Option<&T> {
        self.items.get(&TypeId::of::<T>())?.downcast_ref()
    }
    fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.items.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }
}

fn main() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let mut bus = Bus::default();

    let l = log.clone();
    bus.subscribe(move |e: &UserCreated| {
        l.borrow_mut()
            .push(format!("welcome email to user {}", e.id))
    });
    let l = log.clone();
    bus.subscribe(move |e: &UserCreated| {
        l.borrow_mut().push(format!("audit: user {} created", e.id))
    });
    let l = log.clone();
    bus.subscribe(move |e: &OrderPaid| {
        l.borrow_mut().push(format!(
            "invoice {} for ${:.2}",
            e.order,
            e.cents as f64 / 100.0
        ))
    });

    println!("{} handlers ran", bus.publish(&UserCreated { id: 7 }));
    println!(
        "{} handlers ran",
        bus.publish(&OrderPaid {
            order: "A-1",
            cents: 1999
        })
    );
    println!(
        "{} handlers ran for an unsubscribed type ({})",
        bus.publish(&"just a string"),
        type_name::<&str>()
    );
    for line in log.borrow().iter() {
        println!("  {line}");
    }
    assert_eq!(log.borrow().len(), 3);

    let mut reg = Registry::default();
    reg.insert(42u32);
    reg.insert(String::from("config"));
    println!(
        "{:?} {:?} {:?}",
        reg.get::<u32>(),
        reg.get::<String>(),
        reg.get::<f64>()
    );
    *reg.get_mut::<u32>().unwrap() += 1;
    println!(
        "replaced old value: {:?}, now {:?}",
        reg.insert(100u32),
        reg.get::<u32>()
    );

    // downcasting a Box<dyn Any> BY VALUE, as seen in thread/panic payloads
    let boxed: Box<dyn Any> = Box::new(String::from("owned"));
    match boxed.downcast::<i32>() {
        Ok(n) => println!("int {n}"),
        Err(still_boxed) => println!(
            "not an i32; string? {}",
            still_boxed.downcast_ref::<String>().is_some()
        ),
    }
    println!(
        "lesson: `Any` is for open-ended type sets. If you know all the types, an enum is faster, exhaustive, and clearer."
    );
}
