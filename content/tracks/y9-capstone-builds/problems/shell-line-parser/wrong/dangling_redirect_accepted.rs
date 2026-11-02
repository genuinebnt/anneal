use std::collections::HashMap;

/// An output redirection: `> path` (truncate) or `>> path` (append).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub path: String,
    pub append: bool,
}

/// One simple command of a pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub stdout: Option<Redirect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnterminatedQuote,
    /// A pipeline stage with no command word: `| a`, `a |`, `a | | b`, or only a redirection (`> f`).
    EmptyCommand,
    /// `<`, `>` or `>>` with no filename after it.
    MissingRedirectTarget,
    /// `${NAME` with no closing brace.
    BadVariable,
}

enum Tok {
    Word(String),
    Pipe,
    In,
    Out,
    Append,
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Expands the `$` at `chars[*i]`, appending to `out`; leaves `*i` after the variable.
fn expand(chars: &[char], i: &mut usize, env: &HashMap<String, String>, out: &mut String) -> Result<(), ParseError> {
    *i += 1; // the `$`
    let name: String;
    if chars.get(*i) == Some(&'{') {
        let close = chars[*i..].iter().position(|&c| c == '}').ok_or(ParseError::BadVariable)?;
        name = chars[*i + 1..*i + close].iter().collect();
        *i += close + 1;
    } else if chars.get(*i).map_or(false, |&c| is_name_start(c)) {
        let start = *i;
        while *i < chars.len() && (chars[*i].is_ascii_alphanumeric() || chars[*i] == '_') {
            *i += 1;
        }
        name = chars[start..*i].iter().collect();
    } else {
        out.push('$'); // a lone `$` is just a dollar sign
        return Ok(());
    }
    if let Some(v) = env.get(&name) {
        out.push_str(v);
    }
    Ok(())
}

fn tokenize(line: &str, env: &HashMap<String, String>) -> Result<Vec<Tok>, ParseError> {
    let chars: Vec<char> = line.chars().collect();
    let mut toks = Vec::new();
    let mut cur: Option<String> = None; // Some = a word is in progress (possibly still empty, e.g. after "")
    let mut i = 0;
    macro_rules! flush {
        () => {
            if let Some(w) = cur.take() {
                toks.push(Tok::Word(w));
            }
        };
    }
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                flush!();
                i += 1;
            }
            '#' if cur.is_none() => break, // a comment runs to the end of the line
            '|' => {
                flush!();
                toks.push(Tok::Pipe);
                i += 1;
            }
            '<' => {
                flush!();
                toks.push(Tok::In);
                i += 1;
            }
            '>' => {
                flush!();
                if chars.get(i + 1) == Some(&'>') {
                    toks.push(Tok::Append);
                    i += 2;
                } else {
                    toks.push(Tok::Out);
                    i += 1;
                }
            }
            '\'' => {
                let w = cur.get_or_insert_with(String::new);
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(ParseError::UnterminatedQuote),
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(&c) => {
                            w.push(c);
                            i += 1;
                        }
                    }
                }
            }
            '"' => {
                let w = cur.get_or_insert_with(String::new);
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(ParseError::UnterminatedQuote),
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') if matches!(chars.get(i + 1), Some('"') | Some('\\') | Some('$')) => {
                            w.push(chars[i + 1]);
                            i += 2;
                        }
                        Some('$') => expand(&chars, &mut i, env, w)?,
                        Some(&c) => {
                            w.push(c);
                            i += 1;
                        }
                    }
                }
            }
            '\\' => {
                let w = cur.get_or_insert_with(String::new);
                match chars.get(i + 1) {
                    Some(&n) => {
                        w.push(n);
                        i += 2;
                    }
                    None => {
                        w.push('\\');
                        i += 1;
                    }
                }
            }
            '$' => {
                let mut s = String::new();
                expand(&chars, &mut i, env, &mut s)?;
                if !s.is_empty() {
                    cur.get_or_insert_with(String::new).push_str(&s); // an empty unquoted expansion adds no word
                }
            }
            other => {
                cur.get_or_insert_with(String::new).push(other);
                i += 1;
            }
        }
    }
    flush!();
    Ok(toks)
}

pub fn parse(line: &str, env: &HashMap<String, String>) -> Result<Vec<Command>, ParseError> {
    let toks = tokenize(line, env)?;
    if toks.is_empty() {
        return Ok(Vec::new());
    }
    let mut cmds = Vec::new();
    let mut cur = Command::default();
    let mut pending: Option<Tok> = None;
    for t in toks {
        match t {
            Tok::Word(w) => match pending.take() {
                Some(Tok::In) => cur.stdin = Some(w),
                Some(Tok::Out) => cur.stdout = Some(Redirect { path: w, append: false }),
                Some(Tok::Append) => cur.stdout = Some(Redirect { path: w, append: true }),
                _ => cur.args.push(w),
            },
            Tok::Pipe => {
                if pending.is_some() {
                    return Err(ParseError::MissingRedirectTarget);
                }
                if cur.args.is_empty() {
                    return Err(ParseError::EmptyCommand);
                }
                cmds.push(std::mem::take(&mut cur));
            }
            redirect => {
                if pending.is_some() {
                    return Err(ParseError::MissingRedirectTarget);
                }
                pending = Some(redirect);
            }
        }
    }
    if cur.args.is_empty() {
        return Err(ParseError::EmptyCommand);
    }
    cmds.push(cur);
    Ok(cmds)
}
