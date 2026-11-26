#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedChar(usize),
    UnexpectedToken(usize),
    UnexpectedEnd(usize),
    ChainedComparison(usize),
    BadAssignTarget(usize),
    TooDeep(usize),
}

const MAX_DEPTH: usize = 450;
const PREFIX_POWER: u8 = 13;
const POSTFIX_POWER: u8 = 17;

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    at: usize,
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn lex(source: &str) -> Result<Vec<Token<'_>>, ParseError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
            continue;
        }
        let two = bytes.get(i..i + 2);
        let len = if b.is_ascii_digit() {
            bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count()
        } else if is_word(b) {
            bytes[i..].iter().take_while(|&&b| is_word(b)).count()
        } else if matches!(two, Some(b"||" | b"&&" | b"==" | b"!=" | b"<=" | b">=")) {
            2
        } else if b"=<>+-*/%!^()[],".contains(&b) {
            1
        } else {
            return Err(ParseError::UnexpectedChar(i));
        };
        tokens.push(Token {
            text: &source[i..i + len],
            at: i,
        });
        i += len;
    }
    Ok(tokens)
}

/// `(left, right)` binding powers.
fn infix(op: &str) -> Option<(u8, u8)> {
    Some(match op {
        "=" => (2, 1),
        "||" => (3, 4),
        "&&" => (5, 6),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => (7, 8),
        "+" | "-" => (9, 10),
        "*" | "/" | "%" => (11, 12),
        "^" => (16, 15),
        _ => return None,
    })
}

fn is_comparison(op: &str) -> bool {
    matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=")
}

struct Node {
    sexpr: String,
    assignable: bool,
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    next: usize,
    end: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.next).copied()
    }

    fn bump(&mut self) -> Result<Token<'a>, ParseError> {
        let token = self.peek().ok_or(ParseError::UnexpectedEnd(self.end))?;
        self.next += 1;
        Ok(token)
    }

    fn expect(&mut self, text: &str) -> Result<(), ParseError> {
        let token = self.bump()?;
        if token.text == text {
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken(token.at))
        }
    }

    fn eat(&mut self, text: &str) -> bool {
        let found = self.peek().is_some_and(|t| t.text == text);
        self.next += usize::from(found);
        found
    }

    fn expr(&mut self, min: u8) -> Result<Node, ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ParseError::TooDeep(self.peek().map_or(self.end, |t| t.at)));
        }
        let node = self.operators(min);
        self.depth -= 1;
        node
    }

    fn prefix(&mut self) -> Result<Node, ParseError> {
        let token = self.bump()?;
        match token.text {
            "-" | "!" => {
                let operand = self.expr(PREFIX_POWER)?;
                Ok(Node {
                    sexpr: format!("({} {})", token.text, operand.sexpr),
                    assignable: false,
                })
            }
            "(" => {
                let inner = self.expr(0)?;
                self.expect(")")?;
                Ok(inner)
            }
            word if is_word(word.as_bytes()[0]) => {
                let identifier = !word.as_bytes()[0].is_ascii_digit();
                Ok(Node {
                    sexpr: word.to_string(),
                    assignable: identifier,
                })
            }
            _ => Err(ParseError::UnexpectedToken(token.at)),
        }
    }

    fn operators(&mut self, min: u8) -> Result<Node, ParseError> {
        let mut left = self.prefix()?;
        let mut compared = false;
        while let Some(token) = self.peek() {
            if matches!(token.text, "(" | "[") && POSTFIX_POWER >= min {
                self.next += 1;
                left = if token.text == "[" {
                    let index = self.expr(0)?;
                    self.expect("]")?;
                    Node {
                        sexpr: format!("(index {} {})", left.sexpr, index.sexpr),
                        assignable: true,
                    }
                } else {
                    let mut call = format!("(call {}", left.sexpr);
                    if !self.eat(")") {
                        loop {
                            let argument = self.expr(0)?;
                            call.push(' ');
                            call.push_str(&argument.sexpr);
                            if !self.eat(",") {
                                self.expect(")")?;
                                break;
                            }
                        }
                    }
                    call.push(')');
                    Node {
                        sexpr: call,
                        assignable: false,
                    }
                };
                continue;
            }
            let Some((left_power, right_power)) = infix(token.text) else {
                break;
            };
            if left_power < min {
                break;
            }
            if is_comparison(token.text) {
                if compared && false {
                    return Err(ParseError::ChainedComparison(token.at));
                }
                compared = true;
            } else if left_power < 7 {
                // `&&`, `||` and `=` start a new comparison level.
                compared = false;
            }
            if token.text == "=" && !left.assignable {
                return Err(ParseError::BadAssignTarget(token.at));
            }
            self.next += 1;
            let right = self.expr(right_power)?;
            left = Node {
                sexpr: format!("({} {} {})", token.text, left.sexpr, right.sexpr),
                assignable: false,
            };
        }
        Ok(left)
    }
}

pub fn parse(source: &str) -> Result<String, ParseError> {
    let mut parser = Parser {
        tokens: lex(source)?,
        next: 0,
        end: source.len(),
        depth: 0,
    };
    let node = parser.expr(0)?;
    match parser.peek() {
        Some(extra) => Err(ParseError::UnexpectedToken(extra.at)),
        None => Ok(node.sexpr),
    }
}
