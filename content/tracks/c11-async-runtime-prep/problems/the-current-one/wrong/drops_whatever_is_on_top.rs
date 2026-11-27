use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::Arc;

/// Something that can be made current on a thread.
pub trait Context: Send + Sync + 'static {
    fn name(&self) -> String;
}

/// One entry per live guard, oldest first.
type Stack = Vec<(u64, Arc<dyn Context>)>;

thread_local! {
    static STACK: RefCell<Stack> = const { RefCell::new(Vec::new()) };
    static NEXT: Cell<u64> = const { Cell::new(0) };
}

/// Keeps its context current until dropped.
pub struct Entered {
    id: u64,
    /// A guard belongs to the thread that made it, so it must not be `Send`.
    _this_thread: PhantomData<*const ()>,
}

/// Makes `context` current on this thread while the guard lives.
pub fn enter(context: Arc<dyn Context>) -> Entered {
    let id = NEXT.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    STACK.with(|s| s.borrow_mut().push((id, context)));
    Entered {
        id,
        _this_thread: PhantomData,
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        // remove this guard's own entry, wherever it is; `try_with` covers a guard dropped while the thread exits
        let removed = STACK.try_with(|s| {
            let mut stack = s.borrow_mut();
            stack.pop()
        });
        // the borrow has ended, so the context's destructor may use this module again
        drop(removed);
    }
}

/// The context of the innermost live guard on this thread.
pub fn current() -> Option<Arc<dyn Context>> {
    STACK.with(|s| s.borrow().last().map(|(_, c)| c.clone()))
}

/// Calls `f` with the current context.
pub fn with_current<R>(f: impl FnOnce(Option<&dyn Context>) -> R) -> R {
    // a clone, so no borrow of the stack is held while `f` runs
    let context = current();
    f(context.as_deref())
}
