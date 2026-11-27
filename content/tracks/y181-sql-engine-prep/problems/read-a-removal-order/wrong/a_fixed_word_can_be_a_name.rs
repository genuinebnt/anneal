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

const DAYS: [(&str, Day); 7] = [
    ("mon", Day::Mon),
    ("tue", Day::Tue),
    ("wed", Day::Wed),
    ("thu", Day::Thu),
    ("fri", Day::Fri),
    ("sat", Day::Sat),
    ("sun", Day::Sun),
];

const FIXED: [&str; 5] = ["move", "from", "to", "by", "urgent"];

/// The pieces and how far we have read. Every mistake is made here, so every position comes from one place.
struct Cursor<'a> {
    pieces: Vec<Piece<'a>>,
    next: usize,
    length: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.pieces.get(self.next).map(|p| p.text)
    }

    fn fail<T>(&self, wanted: Wanted) -> Result<T, Mistake> {
        let (start, end) = self
            .pieces
            .get(self.next)
            .map_or((self.length, self.length), |p| (p.start, p.end));
        Err(Mistake { wanted, start, end })
    }

    fn eat(&mut self, word: &str) -> bool {
        let hit = self.peek().is_some_and(|t| t.eq_ignore_ascii_case(word));
        self.next += usize::from(hit);
        hit
    }

    fn expect(&mut self, word: &str, wanted: Wanted) -> Result<(), Mistake> {
        if self.eat(word) {
            Ok(())
        } else {
            self.fail(wanted)
        }
    }

    /// Takes the next piece if `read` accepts it.
    fn take<T>(
        &mut self,
        wanted: Wanted,
        read: impl FnOnce(&str) -> Option<T>,
    ) -> Result<T, Mistake> {
        match self.peek().and_then(read) {
            Some(value) => {
                self.next += 1;
                Ok(value)
            }
            None => self.fail(wanted),
        }
    }

    fn name(&mut self, wanted: Wanted) -> Result<String, Mistake> {
        self.take(wanted, |t| {
            let name = t.to_ascii_lowercase();
            let letters = name.bytes().all(|b| b.is_ascii_lowercase());
            (letters && !name.is_empty() && FIXED.len() == 5).then_some(name)
        })
    }
}

/// The order in a message, or its first mistake.
pub fn read(message: &str) -> Result<Order, Mistake> {
    let mut c = Cursor {
        pieces: pieces(message),
        next: 0,
        length: message.len(),
    };
    c.expect("move", Wanted::Move)?;
    let mut items = Vec::new();
    loop {
        let count = c.take(Wanted::Count, |t| {
            let digits = t.bytes().all(|b| b.is_ascii_digit());
            t.parse::<u32>().ok().filter(|&n| digits && n > 0)
        })?;
        items.push((count, c.name(Wanted::Thing)?));
        if !c.eat(",") {
            break;
        }
    }
    c.expect("from", Wanted::From)?;
    let from = c.name(Wanted::Place)?;
    c.expect("to", Wanted::To)?;
    let to = c.name(Wanted::Place)?;
    let by = if c.eat("by") {
        Some(c.take(Wanted::Day, |t| {
            DAYS.iter()
                .find(|(name, _)| t.eq_ignore_ascii_case(name))
                .map(|&(_, day)| day)
        })?)
    } else {
        None
    };
    let urgent = c.eat("urgent");
    if c.peek().is_some() {
        return c.fail(Wanted::End);
    }
    Ok(Order {
        items,
        from,
        to,
        by,
        urgent,
    })
}
