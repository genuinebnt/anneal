#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub verb: String,
    pub name: String,
    /// In order, names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    NeedMore,
    Whole(Message, usize),
    Invalid,
}

/// The stream cannot be read any further. `before` holds the requests that came before the bad one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub before: Vec<Message>,
}

pub fn read_one(buf: &[u8]) -> Read {
    todo!()
}

pub fn take_all(buf: &mut Vec<u8>) -> Result<Vec<Message>, Invalid> {
    todo!()
}
