//! Splitting a SQL script into statements.

/// The statements of a script, split at each `;` that is not inside a string, a quoted identifier or a comment.
pub fn split_statements(text: &str) -> Vec<String> {
    // @begin 4e-c2
    #[derive(PartialEq)]
    enum S {
        Code,
        Str,
        Ident,
        Line,
        Block,
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut state = S::Code;
    let mut i = 0;
    let mut push = |cur: &mut String, out: &mut Vec<String>| {
        let t = cur.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
        cur.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            S::Code => match (c, next) {
                (';', _) => {
                    push(&mut cur, &mut out);
                }
                ('\'', _) => {
                    state = S::Str;
                    cur.push(c);
                }
                ('"', _) => {
                    state = S::Ident;
                    cur.push(c);
                }
                ('-', Some('-')) => {
                    state = S::Line;
                    cur.push_str("--");
                    i += 1;
                }
                ('/', Some('*')) => {
                    state = S::Block;
                    cur.push_str("/*");
                    i += 1;
                }
                _ => cur.push(c),
            },
            S::Str | S::Ident => {
                let quote = if state == S::Str { '\'' } else { '"' };
                cur.push(c);
                if c == quote {
                    if next == Some(quote) {
                        cur.push(quote);
                        i += 1;
                    } else {
                        state = S::Code;
                    }
                }
            }
            S::Line => {
                cur.push(c);
                if c == '\n' {
                    state = S::Code;
                }
            }
            S::Block => {
                cur.push(c);
                if c == '*' && next == Some('/') {
                    cur.push('/');
                    i += 1;
                    state = S::Code;
                }
            }
        }
        i += 1;
    }
    push(&mut cur, &mut out);
    out
    //~ todo!("4e-c2: a small state machine over the characters: code, string, quoted identifier, line comment, block comment")
    // @end
}
