use std::collections::HashMap;

/// The part of `line` after `prefix`, if `line` starts with it.
pub fn after<'a>(line: &'a str, prefix: &'a str) -> Option<&'a str> {
    line.strip_prefix(prefix)
}

/// The value for `key`.
pub fn lookup<'m>(map: &'m HashMap<String, String>, key: &'m str) -> Option<&'m str> {
    map.get(key).map(String::as_str)
}

/// `value`, or `default` when there's none.
pub fn or_default<'a>(value: Option<&'a str>, default: &'a str) -> &'a str {
    value.unwrap_or(default)
}

/// Settings looked up in `local` first, then in `global`. Global settings usually live for the whole
/// program; local ones for one request.
pub struct Layered<'g, 'l> {
    pub global: &'g [(&'g str, &'g str)],
    pub local: &'l [(&'l str, &'l str)],
}

impl<'g, 'l> Layered<'g, 'l> {
    pub fn get(&self, key: &str) -> Option<&'l str> {
        self.local
            .iter()
            .chain(self.global.iter())
            .find(|e| e.0 == key)
            .map(|e| e.1)
    }
}
