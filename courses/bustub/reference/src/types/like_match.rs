//! SQL `LIKE`.

#[derive(Debug, PartialEq, Eq)]
pub enum LikeError {
    TrailingEscape,
}

/// Whether `text` matches `pattern`. `escape`, when present, makes the following pattern character literal.
pub fn like(text: &str, pattern: &str, escape: Option<char>) -> Result<bool, LikeError> {
    // @begin 3a-c5
    #[derive(Clone, Copy)]
    enum Tok {
        Any,
        One,
        Lit(char),
    }
    let mut toks = Vec::new();
    let mut it = pattern.chars();
    while let Some(c) = it.next() {
        if Some(c) == escape {
            toks.push(Tok::Lit(it.next().ok_or(LikeError::TrailingEscape)?));
        } else if c == '%' {
            if !matches!(toks.last(), Some(Tok::Any)) {
                toks.push(Tok::Any);
            }
        } else if c == '_' {
            toks.push(Tok::One);
        } else {
            toks.push(Tok::Lit(c));
        }
    }
    let t: Vec<char> = text.chars().collect();
    let (mut ti, mut pi) = (0, 0);
    let mut star: Option<(usize, usize)> = None; // (pattern index after the %, text index it has absorbed up to)
    while ti < t.len() {
        match toks.get(pi) {
            Some(Tok::Any) => {
                star = Some((pi + 1, ti));
                pi += 1;
            }
            Some(Tok::One) => {
                ti += 1;
                pi += 1;
            }
            Some(Tok::Lit(c)) if *c == t[ti] => {
                ti += 1;
                pi += 1;
            }
            _ => match star {
                Some((p, s)) => {
                    star = Some((p, s + 1));
                    pi = p;
                    ti = s + 1;
                }
                None => return Ok(false),
            },
        }
    }
    while matches!(toks.get(pi), Some(Tok::Any)) {
        pi += 1;
    }
    Ok(pi == toks.len())
    //~ todo!("3a-c5: tokenise the pattern, then match with one backtrack point for the latest %")
    // @end
}
