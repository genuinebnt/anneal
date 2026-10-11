//! A small SQL lexer.

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Tok {
    Ident(String),
    QuotedIdent(String),
    Int(i64),
    Str(String),
    Sym(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum LexErrorKind {
    UnterminatedString,
    UnterminatedIdent,
    UnterminatedComment,
    BadChar,
    IntegerOverflow,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LexError {
    pub pos: usize,
    pub kind: LexErrorKind,
}

/// Splits `sql` into tokens: strings and quoted identifiers use a doubled quote for a literal quote, and comments are skipped. An error carries
/// the byte position and the kind of the mistake.
pub fn tokenize(sql: &str) -> Result<Vec<Tok>, LexError> {
    // @begin 3d-c1
    let b = sql.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    let err = |pos, kind| Err(LexError { pos, kind });
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'-' && b.get(i + 1) == Some(&b'-') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            match sql[i + 2..].find("*/") {
                Some(end) => i += 2 + end + 2,
                None => return err(i, LexErrorKind::UnterminatedComment),
            }
        } else if c == b'\'' || c == b'"' {
            let start = i;
            let mut s = String::new();
            i += 1;
            loop {
                match sql[i..].find(c as char) {
                    None => return err(start, if c == b'\'' { LexErrorKind::UnterminatedString } else { LexErrorKind::UnterminatedIdent }),
                    Some(at) => {
                        s.push_str(&sql[i..i + at]);
                        i += at + 1;
                        if b.get(i) == Some(&c) {
                            s.push(c as char);
                            i += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
            toks.push(if c == b'\'' { Tok::Str(s) } else { Tok::QuotedIdent(s) });
        } else if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            match sql[start..i].parse::<i64>() {
                Ok(v) => toks.push(Tok::Int(v)),
                Err(_) => return err(start, LexErrorKind::IntegerOverflow),
            }
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            toks.push(Tok::Ident(sql[start..i].to_owned()));
        } else if let Some(two) = sql.get(i..i + 2).filter(|t| matches!(*t, "<=" | ">=" | "<>" | "!=")) {
            toks.push(Tok::Sym(two.to_owned()));
            i += 2;
        } else if b"(),;*+-/=<>.".contains(&c) {
            toks.push(Tok::Sym((c as char).to_string()));
            i += 1;
        } else {
            return err(i, LexErrorKind::BadChar);
        }
    }
    Ok(toks)
    //~ todo!("3d-c1: scan the bytes; strings and quoted identifiers with doubled quotes; comments; numbers; identifiers; symbols")
    // @end
}
