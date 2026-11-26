pub trait Pattern {
    fn start(&self) -> u32;
    fn step(&self, state: u32, c: char) -> Option<u32>;
    fn accepts(&self, state: u32) -> bool;
}

pub struct Rule<'a> {
    pub pattern: &'a dyn Pattern,
    pub skip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub rule: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexError {
    NoToken(usize),
}

/// The end and rule of the longest non-empty word starting at `start`.
fn longest(rules: &[Rule], text: &[char], start: usize) -> Option<(usize, usize)> {
    let mut states: Vec<Option<u32>> = rules.iter().map(|r| Some(r.pattern.start())).collect();
    let mut best = None;
    for (at, &c) in text.iter().enumerate().skip(start) {
        let mut alive = false;
        let mut accepted = None;
        for (index, (rule, state)) in rules.iter().zip(&mut states).enumerate() {
            *state = state.and_then(|s| rule.pattern.step(s, c));
            if let Some(s) = *state {
                alive = true;
                if rule.pattern.accepts(s) {
                    accepted = Some(index);
                }
            }
        }
        if let Some(rule) = accepted {
            best = Some((at + 1, rule));
        }
        if !alive {
            break;
        }
    }
    best
}

pub fn lex(rules: &[Rule], text: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let (end, rule) = longest(rules, &chars, start).ok_or(LexError::NoToken(start))?;
        if !rules[rule].skip {
            tokens.push(Token { rule, start, end });
        }
        start = end;
    }
    Ok(tokens)
}
