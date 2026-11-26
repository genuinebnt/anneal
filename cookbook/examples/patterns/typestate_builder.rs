//! RECIPE 09 (patterns): typestate builder: invalid states do not compile.
//! LEVEL: intermediate
//! Use in: any config/request type later (task specs, API clients).   Run: cargo run --example typestate_builder
//! COMMON USES: HTTP clients, connection/session objects, config types where a required step must not be skippable.
//!
//! Pattern: encode "has the URL been set?" in a type parameter. `build()` only exists once it has.
//! Zero runtime cost; the error moves from runtime to compile time.
//! Read: https://rust-unofficial.github.io/patterns/ (builder and "type state" material)
#![allow(dead_code)]
use std::marker::PhantomData;

struct NoUrl;
struct HasUrl;

struct RequestBuilder<S> {
    url: Option<String>,
    retries: u32,
    _state: PhantomData<S>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Request {
    url: String,
    retries: u32,
}

impl RequestBuilder<NoUrl> {
    fn new() -> Self {
        RequestBuilder {
            url: None,
            retries: 0,
            _state: PhantomData,
        }
    }
    fn url(self, url: &str) -> RequestBuilder<HasUrl> {
        RequestBuilder {
            url: Some(url.to_string()),
            retries: self.retries,
            _state: PhantomData,
        }
    }
}

impl<S> RequestBuilder<S> {
    fn retries(mut self, n: u32) -> Self {
        self.retries = n;
        self
    }
}

impl RequestBuilder<HasUrl> {
    fn build(self) -> Request {
        Request {
            url: self.url.expect("typestate guarantees the url is set"),
            retries: self.retries,
        }
    }
}

fn main() {
    let req = RequestBuilder::new()
        .retries(3)
        .url("https://example.com")
        .build();
    println!("{req:?}");
    // RequestBuilder::new().build(); // <- does not compile: `build` not found for RequestBuilder<NoUrl>
}
