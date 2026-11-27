use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::Arc;

pub trait Context: Send + Sync + 'static {
    fn name(&self) -> String;
}

thread_local! {
    static SLOT: RefCell<Option<Arc<dyn Context>>> = const { RefCell::new(None) };
}

pub struct Entered {
    previous: Option<Arc<dyn Context>>,
    _this_thread: PhantomData<*const ()>,
}

pub fn enter(context: Arc<dyn Context>) -> Entered {
    let previous = SLOT.with(|s| s.borrow_mut().replace(context));
    Entered {
        previous,
        _this_thread: PhantomData,
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        let previous = self.previous.take();
        let removed = SLOT.try_with(|s| std::mem::replace(&mut *s.borrow_mut(), previous));
        drop(removed);
    }
}

pub fn current() -> Option<Arc<dyn Context>> {
    SLOT.with(|s| s.borrow().clone())
}

pub fn with_current<R>(f: impl FnOnce(Option<&dyn Context>) -> R) -> R {
    let context = current();
    f(context.as_deref())
}
