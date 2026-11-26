use solution::*;

/// Exactly this text.
#[allow(dead_code)]
struct Exact(&'static str);
impl Pattern for Exact {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, state: u32, c: char) -> Option<u32> {
        (self.0.chars().nth(state as usize) == Some(c)).then_some(state + 1)
    }
    fn accepts(&self, state: u32) -> bool {
        state as usize == self.0.chars().count()
    }
}

/// One or more characters that pass the test.
#[allow(dead_code)]
struct Many(fn(char) -> bool);
impl Pattern for Many {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, _: u32, c: char) -> Option<u32> {
        (self.0)(c).then_some(1)
    }
    fn accepts(&self, state: u32) -> bool {
        state == 1
    }
}

/// Digits, or digits, a dot and more digits. `1.` on its own is not a number.
#[allow(dead_code)]
struct Number;
impl Pattern for Number {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, state: u32, c: char) -> Option<u32> {
        match (state, c) {
            (0 | 1, '0'..='9') => Some(1),
            (1, '.') => Some(2),
            (2 | 3, '0'..='9') => Some(3),
            _ => None,
        }
    }
    fn accepts(&self, state: u32) -> bool {
        state == 1 || state == 3
    }
}

#[allow(dead_code)]
fn cut(patterns: &[(&dyn Pattern, bool)], text: &str) -> Result<Vec<(usize, String)>, LexError> {
    let rules: Vec<Rule> = patterns.iter().map(|&(pattern, skip)| Rule { pattern, skip }).collect();
    let chars: Vec<char> = text.chars().collect();
    lex(&rules, text).map(|tokens| tokens.iter().map(|t| (t.rule, chars[t.start..t.end].iter().collect())).collect())
}

#[allow(dead_code)]
fn words(list: &[(usize, &str)]) -> Result<Vec<(usize, String)>, LexError> {
    Ok(list.iter().map(|&(r, w)| (r, w.to_string())).collect())
}

#[test]
fn the_longest_word_wins_and_ties_go_to_the_earlier_rule() {
    let (kw, name, space) = (Exact("if"), Many(|c| c.is_alphabetic()), Many(|c| c == ' '));
    let rules: [(&dyn Pattern, bool); 3] = [(&kw, false), (&name, false), (&space, true)];
    check!("if iffy i", cut(&rules, "if iffy i"), words(&[(0, "if"), (1, "iffy"), (1, "i")]));
}

#[test]
fn fall_back_to_the_last_complete_word() {
    let dot = Exact(".");
    let rules: [(&dyn Pattern, bool); 2] = [(&Number, false), (&dot, false)];
    check!("1.5.2.", cut(&rules, "1.5.2."), words(&[(0, "1.5"), (1, "."), (0, "2"), (1, ".")]));
}

#[test]
fn a_rule_can_stop_accepting_and_accept_again() {
    let (a, b) = (Exact("<"), Exact("<<="));
    let rules: [(&dyn Pattern, bool); 2] = [(&a, false), (&b, false)];
    check!("<<=<<", cut(&rules, "<<=<<"), words(&[(1, "<<="), (0, "<"), (0, "<")]));
}

#[test]
fn positions_count_characters() {
    let (name, space) = (Many(|c| c.is_alphabetic()), Many(|c| c == ' '));
    let rules = [Rule { pattern: &name, skip: false }, Rule { pattern: &space, skip: true }];
    check!(
        "né ab",
        lex(&rules, "né ab"),
        Ok(vec![Token { rule: 0, start: 0, end: 2 }, Token { rule: 0, start: 3, end: 5 }])
    );
}

#[test]
fn nothing_to_cut_and_nothing_that_fits() {
    let a = Exact("ab");
    check!("empty text", cut(&[(&a, false)], ""), words(&[]));
    check!("abax", cut(&[(&a, false)], "abax"), Err(LexError::NoToken(2)));
}
