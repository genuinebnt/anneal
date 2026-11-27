use axum::Router;

/// A fresh service with no pages.
pub fn router() -> Router {
    todo!()
}

/// `router()`, calling `hook(slug)` right after each precondition that is found to hold.
pub fn router_with_hook(hook: impl Fn(&str) + Send + Sync + 'static) -> Router {
    todo!()
}
