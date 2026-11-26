use std::any::{Any, TypeId};

/// A piece of data attached to an entity. At most one of each type per entity.
pub trait Component: Any {
    fn name(&self) -> String;
}

#[derive(Default)]
pub struct Entity {
    components: Vec<Box<dyn Component>>,
}

impl Entity {
    fn position<T: Component>(&self) -> Option<usize> {
        // `(**c)` is the dyn Component, so type_id comes from its vtable: the concrete type.
        self.components
            .iter()
            .position(|c| (**c).type_id() == TypeId::of::<T>())
    }

    /// Adds `c`, replacing a component of the same type in place. Returns the one it replaced.
    pub fn insert<T: Component>(&mut self, c: T) -> Option<T> {
        match self.position::<T>() {
            Some(i) => {
                let new: Box<dyn Component> = Box::new(c);
                let old: Box<dyn Any> =
                    std::mem::replace(&mut self.components[i], new) as Box<dyn Component>;
                old.downcast::<T>().ok().map(|b| *b)
            }
            None => {
                self.components.push(Box::new(c));
                None
            }
        }
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        self.components.iter().find_map(|c| {
            let any: &dyn Any = &**c;
            any.downcast_ref::<T>()
        })
    }

    pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
        self.components.iter_mut().find_map(|c| {
            let any: &mut dyn Any = &mut **c;
            any.downcast_mut::<T>()
        })
    }

    pub fn remove<T: Component>(&mut self) -> Option<T> {
        let i = self.position::<T>()?;
        let b: Box<dyn Any> = self.components.remove(i);
        b.downcast::<T>().ok().map(|b| *b)
    }

    /// The components' names in insertion order.
    pub fn names(&self) -> Vec<String> {
        self.components.iter().map(|c| c.name()).collect()
    }
}
