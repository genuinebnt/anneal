use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
pub struct Routed {
    pub id: u64,
    pub route: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub trace: String,
}

/// A request checked out of a pool. Dropping it gives the slot back.
pub struct Pooled {
    pub req: Request,
    pub free_slots: Rc<Cell<usize>>,
}

impl Drop for Pooled {
    fn drop(&mut self) {
        self.free_slots.set(self.free_slots.get() + 1);
    }
}

/// "<id> <method> <path> (<n> headers, <body length> bytes)"; a request without a body has 0 bytes.
pub fn describe(req: &Request) -> String {
    let body = req.body.unwrap_or_default();
    format!(
        "{} {} {} ({} headers, {} bytes)",
        req.id,
        req.method,
        req.path,
        req.headers.len(),
        body.len()
    )
}

/// Consumes the request. The route is "<METHOD in upper case> <path>", the content type is the value of the
/// first "content-type" header (names compared ignoring ASCII case), the body is empty without one, and the
/// trace is `describe` of the request as it came in.
pub fn route(req: Request) -> Routed {
    let Request { method, path, .. } = req;
    let route = format!("{} {path}", method.to_uppercase());
    let content_type = req
        .headers
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v);
    let body = match req.body {
        Some(b) => b,
        None => Vec::new(),
    };
    Routed {
        id: req.id,
        route,
        content_type,
        body,
        trace: describe(&req),
    }
}

/// Routes a pooled request. Its pool slot must still be given back.
pub fn route_pooled(p: Pooled) -> Routed {
    route(p.req)
}
