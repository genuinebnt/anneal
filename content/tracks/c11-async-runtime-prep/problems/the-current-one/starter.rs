use std::marker::PhantomData;
use std::sync::Arc;

/// Something that can be made current on a thread.
pub trait Context: Send + Sync + 'static {
    fn name(&self) -> String;
}

/// Keeps its context current until dropped.
pub struct Entered {
    // TODO: the fields
    /// A guard belongs to the thread that made it, so it must not be `Send`.
    _this_thread: PhantomData<*const ()>,
}

/// Makes `context` current on this thread while the guard lives.
pub fn enter(context: Arc<dyn Context>) -> Entered {
    todo!()
}

/// The context of the innermost live guard on this thread.
pub fn current() -> Option<Arc<dyn Context>> {
    todo!()
}

/// Calls `f` with the current context.
pub fn with_current<R>(f: impl FnOnce(Option<&dyn Context>) -> R) -> R {
    todo!()
}
