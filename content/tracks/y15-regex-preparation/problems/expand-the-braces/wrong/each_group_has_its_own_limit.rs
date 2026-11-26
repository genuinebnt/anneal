pub const MAX_WORDS: usize = 10_000;
pub const MAX_OPEN: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceError {
    UnmatchedClose(usize),
    Unclosed(usize),
    TrailingEscape(usize),
    TooDeep(usize),
    TooManyWords,
}

enum Node {
    Text(String),
    Sequence(Vec<Node>),
    Choice(Vec<Node>),
}

impl Node {
    fn count(&self) -> usize {
        match self {
            Node::Text(_) => 1,
            Node::Sequence(parts) => parts.iter().fold(1, |n, p| n.saturating_mul(p.count())),
            Node::Choice(choices) => choices.iter().fold(0, |n, c| n.saturating_add(c.count())),
        }
    }

    fn words(&self) -> Vec<String> {
        match self {
            Node::Text(t) => vec![t.clone()],
            Node::Choice(choices) => choices.iter().flat_map(Node::words).collect(),
            Node::Sequence(parts) => {
                let mut out = vec![String::new()];
                for part in parts {
                    let tails = part.words();
                    if let [only] = tails.as_slice() {
                        out.iter_mut().for_each(|w| w.push_str(only));
                    } else {
                        out = out
                            .iter()
                            .flat_map(|head| tails.iter().map(move |t| format!("{head}{t}")))
                            .collect();
                    }
                }
                out
            }
        }
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        self.pos += usize::from(c.is_some());
        c
    }

    /// Reads until the end of the text or, inside a group, until a `,` or `}` that is left unread.
    fn sequence(&mut self, open: usize) -> Result<Node, BraceError> {
        let mut parts = Vec::new();
        let mut text = String::new();
        while let Some(&c) = self.chars.get(self.pos) {
            if open > 0 && (c == ',' || c == '}') {
                break;
            }
            let at = self.pos;
            self.pos += 1;
            match c {
                '}' => return Err(BraceError::UnmatchedClose(at)),
                '\\' => text.push(self.bump().ok_or(BraceError::TrailingEscape(at))?),
                '{' => {
                    if open == MAX_OPEN {
                        return Err(BraceError::TooDeep(at));
                    }
                    if !text.is_empty() {
                        parts.push(Node::Text(std::mem::take(&mut text)));
                    }
                    parts.push(self.group(at, open + 1)?);
                }
                other => text.push(other),
            }
        }
        if !text.is_empty() {
            parts.push(Node::Text(text));
        }
        Ok(Node::Sequence(parts))
    }

    fn group(&mut self, start: usize, open: usize) -> Result<Node, BraceError> {
        let mut choices = Vec::new();
        loop {
            choices.push(self.sequence(open)?);
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(Node::Choice(choices)),
                _ => return Err(BraceError::Unclosed(start)),
            }
        }
    }
}

/// Every word the pattern stands for, in order.
pub fn expand(pattern: &str) -> Result<Vec<String>, BraceError> {
    let mut parser = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
    };
    let tree = parser.sequence(0)?;
    fn widest(n: &Node) -> usize {
        match n {
            Node::Text(_) => 1,
            Node::Choice(c) => c.len().max(c.iter().map(widest).max().unwrap_or(0)),
            Node::Sequence(p) => p.iter().map(widest).max().unwrap_or(0),
        }
    }
    if widest(&tree) > MAX_WORDS {
        return Err(BraceError::TooManyWords);
    }
    Ok(tree.words())
}
