use std::marker::PhantomData;

#[derive(Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,
}

#[derive(Debug, PartialEq)]
pub enum UrlError {
    Empty,
    NoScheme,
}

/// The builder's states: before and after it has a URL.
pub struct NoUrl;
pub struct HasUrl;

pub struct RequestBuilder<S> {
    url: String,
    headers: Vec<(String, String)>,
    timeout_ms: u64,
    state: PhantomData<S>,
}

impl RequestBuilder<NoUrl> {
    /// No URL, no headers, a 30 000 ms timeout.
    pub fn new() -> Self {
        RequestBuilder {
            url: String::new(),
            headers: Vec::new(),
            timeout_ms: 30_000,
            state: PhantomData,
        }
    }

    /// The only way to get a builder that can `build`.
    pub fn url(self, url: &str) -> Result<RequestBuilder<HasUrl>, UrlError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(UrlError::Empty);
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(UrlError::NoScheme);
        }
        // A different type parameter means a new value: move each field across.
        Ok(RequestBuilder {
            url: url.to_string(),
            headers: self.headers,
            timeout_ms: self.timeout_ms,
            state: PhantomData,
        })
    }
}

impl<S> RequestBuilder<S> {
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
}

impl RequestBuilder<HasUrl> {
    pub fn build(self) -> Request {
        Request {
            url: self.url,
            headers: self.headers,
            timeout_ms: self.timeout_ms,
        }
    }
}
