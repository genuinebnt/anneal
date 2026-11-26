use std::io::{self, Write};

pub enum Content {
    Full(Vec<u8>),
    /// Produced while it is being sent.
    Pieces(Box<dyn Iterator<Item = Vec<u8>>>),
}

pub struct Reply {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub content: Content,
}

pub fn send<W: Write>(out: &mut W, reply: Reply, head_only: bool) -> io::Result<()> {
    todo!()
}
