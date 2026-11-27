#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// `None` when nothing at all was written.
    pub value: Option<String>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Unclosed,
    AfterQuote,
    StrayQuote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadLine {
    pub problem: Problem,
    pub start: usize,
    pub end: usize,
}

/// The fields of one line, or its first mistake.
pub fn fields(line: &str) -> Result<Vec<Field>, BadLine> {
    let bad = |problem, start, end| {
        Err(BadLine {
            problem,
            start,
            end,
        })
    };
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let mut at = start;
        let value = if line[at..].starts_with('"') {
            at += 1;
            let mut text = String::new();
            loop {
                let Some(length) = line[at..].find('"') else {
                    return bad(Problem::Unclosed, start, line.len());
                };
                text.push_str(&line[at..at + length]);
                at += length + 1;
                if !line[at..].starts_with('"') {
                    break;
                }
                text.push('"');
                at += 1;
            }
            if let Some(next) = line[at..].chars().next() {
                if next != ',' {
                    return bad(Problem::AfterQuote, at, at + next.len_utf8());
                }
            }
            (!text.is_empty()).then_some(text)
        } else {
            while let Some(c) = line[at..].chars().next() {
                if c == ',' {
                    break;
                }
                if c == '"' {
                    return bad(Problem::StrayQuote, at, at + 1);
                }
                at += c.len_utf8();
            }
            (at > start).then(|| line[start..at].to_string())
        };
        out.push(Field {
            value,
            start,
            end: at,
        });
        if at == line.len() {
            return Ok(out);
        }
        start = at + 1; // past the comma
    }
}
