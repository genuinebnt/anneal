use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::Debug;

/// Holds at most one value of each type, like `http::Extensions`. Values of any type that owns its data (or
/// borrows only `'static` data) can go in.
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl Extensions {
    pub fn new() -> Self {
        Extensions {
            map: HashMap::new(),
        }
    }

    pub fn insert<T: Any>(&mut self, value: T) -> Option<T> {
        let old = self.map.insert(TypeId::of::<T>(), Box::new(value))?;
        Some(*old.downcast::<T>().expect("stored under its own TypeId"))
    }

    pub fn get<T: Any>(&self) -> Option<&T> {
        self.map.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.map.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn remove<T: Any>(&mut self) -> Option<T> {
        let b = self.map.remove(&TypeId::of::<T>())?;
        Some(*b.downcast::<T>().expect("stored under its own TypeId"))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Keeps `value` in `log` for debug output later.
pub fn remember<T: Debug + 'static>(log: &mut Vec<Box<dyn Debug>>, value: T) {
    log.push(Box::new(value));
}

/// A name made at run time that lives for the rest of the program. Each call leaks its String on purpose.
pub fn intern_forever(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}
