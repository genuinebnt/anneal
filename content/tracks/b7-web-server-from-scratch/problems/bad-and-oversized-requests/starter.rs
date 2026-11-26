#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_line: usize,
    pub max_head: usize,
    pub max_headers: usize,
    pub max_body: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    BadRequest,
    LineTooLong,
    HeadTooLarge,
    BodyTooLarge,
}

impl Refusal {
    pub fn status(self) -> u16 {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub head_len: usize,
    pub body_len: usize,
}

pub fn check_head(buf: &[u8], limits: &Limits) -> Result<Option<Head>, Refusal> {
    todo!()
}
