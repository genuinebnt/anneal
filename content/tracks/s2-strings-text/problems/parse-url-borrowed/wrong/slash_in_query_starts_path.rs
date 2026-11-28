#[derive(Debug, PartialEq)]
pub struct Url<'a> {
    pub scheme: &'a str,
    pub host: &'a str,
    pub port: Option<u16>,
    pub path: &'a str,
    pub query: Vec<(&'a str, &'a str)>,
}

pub fn parse_url(s: &str) -> Option<Url<'_>> {
    let (scheme, rest) = s.split_once("://")?;
    // Wrong: cuts the authority at the first '/', even when that '/' is inside the query.
    let (authority, tail) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let (authority, early_query) = authority.split_once('?').unwrap_or((authority, ""));
    let (path, late_query) = tail.split_once('?').unwrap_or((tail, ""));
    let query = if early_query.is_empty() { late_query } else { early_query };
    let path = if path.is_empty() { "/" } else { path };
    let (host, port) = match authority.split_once(':') {
        Some((h, p)) => (h, Some(p.parse().ok()?)),
        None => (authority, None),
    };
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    let query = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
        .collect();
    Some(Url { scheme, host, port, path, query })
}
