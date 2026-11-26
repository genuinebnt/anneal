/// The values a pattern captured.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Params<'path> {
    values: Vec<(String, std::borrow::Cow<'path, str>)>,
}

impl Params<'_> {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_ref())
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Found<'table, 'path, T> {
    Route(&'table T, Params<'path>),
    WrongMethod(Vec<&'table str>),
    Nothing,
}

enum Segment {
    Literal(String),
    One(String),
    Rest(String),
}

struct Route<T> {
    method: String,
    pattern: Vec<Segment>,
    value: T,
}

impl<T> Route<T> {
    fn captures<'p>(&self, path: &[&'p str]) -> Option<Params<'p>> {
        let mut params = Params::default();
        for (i, segment) in self.pattern.iter().enumerate() {
            match segment {
                Segment::Literal(text) if path.get(i) == Some(&text.as_str()) => {}
                Segment::Literal(_) => return None,
                Segment::One(name) => {
                    let value = path.get(i).filter(|v| !v.is_empty())?;
                    params.values.push((name.clone(), (*value).into()));
                }
                Segment::Rest(name) => {
                    params.values.push((name.clone(), path.get(i..)?.join("/").into()));
                    return Some(params);
                }
            }
        }
        Some(params)
    }
}

pub struct Table<T> {
    routes: Vec<Route<T>>,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Table::new()
    }
}

impl<T> Table<T> {
    pub fn new() -> Table<T> {
        Table { routes: Vec::new() }
    }

    pub fn add(&mut self, method: &str, pattern: &str, value: T) {
        let pattern = pattern
            .split('/')
            .skip(1)
            .map(|s| {
                if let Some(name) = s.strip_prefix(':') {
                    Segment::One(name.to_string())
                } else if let Some(name) = s.strip_prefix('*') {
                    Segment::Rest(name.to_string())
                } else {
                    Segment::Literal(s.to_string())
                }
            })
            .collect();
        self.routes.push(Route { method: method.to_string(), pattern, value });
    }

    pub fn find<'table, 'path>(&'table self, method: &str, path: &'path str) -> Found<'table, 'path, T> {
        let path = path.split_once('?').map_or(path, |(path, _)| path);
        let segments: Vec<&str> = path.split('/').skip(1).collect();
        let mut allowed: Vec<&str> = Vec::new();
        let mut get_for_head = None;
        for route in &self.routes {
            let Some(params) = route.captures(&segments) else {
                continue;
            };
            if route.method == method {
                return Found::Route(&route.value, params);
            }
            if method == "HEAD" && route.method == "GET" && get_for_head.is_none() {
                get_for_head = Some((&route.value, params));
            }
            if !allowed.contains(&route.method.as_str()) {
                allowed.push(&route.method);
            }
        }
        match get_for_head {
            Some((value, params)) => Found::Route(value, params),
            None if allowed.is_empty() => Found::Nothing,
            None => Found::WrongMethod(allowed),
        }
    }
}
