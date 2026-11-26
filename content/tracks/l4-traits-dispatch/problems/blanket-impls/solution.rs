use std::collections::BTreeMap;
use std::sync::Arc;

pub trait Handler {
    fn call(&self, req: &str) -> String;

    /// A short description for route listings.
    fn name(&self) -> &'static str {
        "handler"
    }
}

/// Always replies with the same body.
pub struct Static(pub String);

impl Handler for Static {
    fn call(&self, _req: &str) -> String {
        self.0.clone()
    }

    fn name(&self) -> &'static str {
        "static"
    }
}

/// Any function or closure from a request to a reply is a handler.
impl<F> Handler for F
where
    F: Fn(&str) -> String,
{
    fn call(&self, req: &str) -> String {
        self(req)
    }
}

/// A shared handler is a handler. (A `Box<H>` impl would conflict with the one above:
/// `Box<F>` is itself `Fn` when `F` is.)
impl<H: Handler + ?Sized> Handler for Arc<H> {
    fn call(&self, req: &str) -> String {
        (**self).call(req)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}

#[derive(Default)]
pub struct Router {
    routes: BTreeMap<String, Box<dyn Handler + Send + Sync>>,
}

impl Router {
    pub fn route(mut self, path: &str, h: impl Handler + Send + Sync + 'static) -> Self {
        self.routes.insert(path.to_string(), Box::new(h));
        self
    }

    /// The reply of the handler for `path`, or "404 <path>".
    pub fn dispatch(&self, path: &str, req: &str) -> String {
        match self.routes.get(path) {
            Some(h) => h.call(req),
            None => format!("404 {path}"),
        }
    }

    /// "<path>: <handler name>" for every route, sorted by path.
    pub fn describe(&self) -> Vec<String> {
        self.routes
            .iter()
            .map(|(p, h)| format!("{p}: {}", h.name()))
            .collect()
    }
}
