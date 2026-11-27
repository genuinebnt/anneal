/// A word or a comma, with the bytes of the message it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
}

/// Cuts a message at white space. A comma is always a piece of its own.
pub fn pieces(message: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut open = None;
    let mut push = |start: usize, end: usize| {
        out.push(Piece {
            text: &message[start..end],
            start,
            end,
        })
    };
    for (at, c) in message.char_indices() {
        if c.is_whitespace() || c == ',' {
            if let Some(start) = open.take() {
                push(start, at);
            }
            if c == ',' {
                push(at, at + 1);
            }
        } else if open.is_none() {
            open = Some(at);
        }
    }
    if let Some(start) = open {
        push(start, message.len());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    /// How many of what, in the order written.
    pub items: Vec<(u32, String)>,
    pub from: String,
    pub to: String,
    pub by: Option<Day>,
    pub urgent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted {
    Move,
    Count,
    Thing,
    From,
    Place,
    To,
    Day,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mistake {
    pub wanted: Wanted,
    pub start: usize,
    pub end: usize,
}

/// The order in a message, or its first mistake.
pub fn read(message: &str) -> Result<Order, Mistake> {
    let _ = message;
    todo!()
}
