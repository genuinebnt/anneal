use std::collections::HashMap;

pub struct Profile {
    pub name: String,
    nickname: Option<String>,
    pub labels: HashMap<u32, String>,
}

impl Profile {
    pub fn new(name: &str, nickname: Option<&str>) -> Profile {
        Profile {
            name: String::from(name),
            nickname: nickname.map(String::from),
            labels: HashMap::new(),
        }
    }

    /// The nickname, if any.
    pub fn nickname(&self) -> &Option<String> {
        &self.nickname
    }

    /// The label for `id`, if any.
    pub fn label(&self, id: u32) -> Option<&String> {
        self.labels.get(&id)
    }
}

/// "Hello, <nickname>!" if there is a nickname, else "Hello, <name>!".
pub fn greeting(name: &String, nickname: &Option<String>) -> String {
    format!("Hello, {}!", nickname.as_ref().unwrap_or(name))
}

/// The label for `id`, or `default`.
pub fn label_or<'a>(p: &'a Profile, id: u32, default: &'a String) -> &'a String {
    p.label(id).unwrap_or(default)
}
