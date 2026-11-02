from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

WITHIN = """
fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}
"""

# ---------------------------------------------------------------------------------------------------------------
# 1. Shell line parser
SH_TYPES = """
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
"""

SH_STARTER = SH_TYPES + """
/// Parses one command line into a pipeline. `env` supplies `$VAR` values.
pub fn parse(line: &str, env: &HashMap<String, String>) -> Result<Vec<Command>, ParseError> {
    todo!()
}
"""

SH = SH_TYPES + """
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
            ' ' | '\\t' => {
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
            '\\'' => {
                let w = cur.get_or_insert_with(String::new);
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(ParseError::UnterminatedQuote),
                        Some('\\'') => {
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
                        Some('\\\\') if matches!(chars.get(i + 1), Some('"') | Some('\\\\') | Some('$')) => {
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
            '\\\\' => {
                let w = cur.get_or_insert_with(String::new);
                match chars.get(i + 1) {
                    Some(&n) => {
                        w.push(n);
                        i += 2;
                    }
                    None => {
                        w.push('\\\\');
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
    if pending.is_some() {
        return Err(ParseError::MissingRedirectTarget);
    }
    if cur.args.is_empty() {
        return Err(ParseError::EmptyCommand);
    }
    cmds.push(cur);
    Ok(cmds)
}
"""

SH_HELPERS = """
        use std::collections::HashMap;

        fn env() -> HashMap<String, String> {
            HashMap::from([
                ("HOME".to_string(), "/home/u".to_string()),
                ("NAME".to_string(), "world".to_string()),
                ("EMPTY".to_string(), String::new()),
                ("A".to_string(), "1".to_string()),
            ])
        }

        fn argv(line: &str) -> Result<Vec<Vec<String>>, ParseError> {
            parse(line, &env()).map(|cs| cs.into_iter().map(|c| c.args).collect())
        }

        fn v(a: &[&str]) -> Vec<String> {
            a.iter().map(|s| s.to_string()).collect()
        }
"""


def A(line):
    return f'argv("{line}")'


P.append(dict(
    slug="shell-line-parser", title="Parse a shell command line: quotes, expansion, pipes, redirects", level="medium", stage="build",
    tags=["shell", "tokenizer", "quoting", "variable expansion", "pipelines", "redirection", "state machine", "error handling"],
    teaches=["Shell syntax is a small state machine over characters: unquoted, inside `'...'` (everything literal), inside `\"...\"` (expansion and a few escapes) and after a backslash. Adjacent pieces glue into one word.",
             "Tokenising and parsing are separate steps: first words and operators (`|`, `<`, `>`, `>>`), then group the tokens into commands. Errors belong to the step that can see them.",
             "A word can be empty (`\"\"`) yet still a word, while an unquoted empty expansion is not: tracking 'is a word in progress' separately from its text is the key detail."],
    statement="""
        Build the front end of a shell: `parse(line, env)` turns one command line into a pipeline of `Command`s
        (`args`, an optional `stdin` file, an optional `stdout` redirect).

        - Words are separated by spaces or tabs. `|` separates commands; `<`, `>`, `>>` redirect, and **need no surrounding spaces**
          (`a|b`, `cat<in>out`). A redirect's target is the next word. If a command has several redirects of the same kind, the last wins;
          they may appear anywhere in the command.
        - `'...'` keeps every character literally. `"..."` keeps everything literally except `$` expansion and the escapes `\\"`, `\\\\`
          and `\\$` (any other backslash stays a backslash). Outside quotes a backslash makes the next character literal (`a\\ b` is one
          word, `\\|` is a pipe character). Adjacent pieces form one word: `a"b c"'d'` is `ab cd`. `""` is an empty word.
        - `$NAME` (letters, digits, `_`, not starting with a digit) and `${NAME}` expand from `env`; an undefined name is the empty
          string; a `$` followed by anything else is a literal `$`. The result is **not** split into words, except that an unquoted
          expansion to the empty string adds no word at all (`echo $EMPTY x` has two arguments).
        - `#` at the start of a word begins a comment that runs to the end of the line.
        - An empty (or comment-only) line is `Ok(vec![])`. Errors: `UnterminatedQuote`; `EmptyCommand` for a stage without a command
          word (`| a`, `a |`, `a | | b`, `> f`); `MissingRedirectTarget` when `<`/`>`/`>>` has no word after it (including `a > | b` and `a > > f`);
          `BadVariable` for `${NAME` without `}`.
    """,
    starter=SH_STARTER,
    solution=SH,
    visible=[
        SH_HELPERS + """
        #[test]
        fn redirects_are_collected() {
            let c = parse("sort < in > out", &env()).unwrap();
            check!("one command", c.len(), 1);
            check!("args", c[0].args.clone(), v(&["sort"]));
            check!("stdin", c[0].stdin.clone(), Some("in".to_string()));
            check!("stdout", c[0].stdout.clone(), Some(Redirect { path: "out".to_string(), append: false }));
        }
        """,
        T("one_command", "plain words", A("ls -l /tmp"), "Ok(vec![v(&[\"ls\", \"-l\", \"/tmp\"])])"),
        T("a_pipeline", "three stages", A("cat f | grep x | wc -l"), "Ok(vec![v(&[\"cat\", \"f\"]), v(&[\"grep\", \"x\"]), v(&[\"wc\", \"-l\"])])"),
        T("pipe_needs_no_spaces", "a|b", A("a|b"), "Ok(vec![v(&[\"a\"]), v(&[\"b\"])])"),
        T("quotes_group_words", "double and single", "argv(\"echo \\\"a b\\\" 'c  d'\")", "Ok(vec![v(&[\"echo\", \"a b\", \"c  d\"])])"),
        T("variables_expand_in_double_quotes", "but not in single quotes", "argv(\"echo \\\"$HOME/bin\\\" '$HOME'\")", "Ok(vec![v(&[\"echo\", \"/home/u/bin\", \"$HOME\"])])"),
        T("empty_line", "no commands", "(argv(\"\"), argv(\"   \"))", "(Ok(vec![]), Ok(vec![]))"),
        T("unterminated_quote", "an error, not a panic", "argv(\"echo \\\"abc\")", "Err(ParseError::UnterminatedQuote)"),
    ],
    hidden=[
        SH_HELPERS + """
        #[test]
        fn redirects_in_every_position() {
            let c = parse("a>f b", &env()).unwrap();
            check!("unspaced redirect, then another arg", (c[0].args.clone(), c[0].stdout.clone()), (v(&["a", "b"]), Some(Redirect { path: "f".to_string(), append: false })));
            let c = parse("a>>f", &env()).unwrap();
            check!("append", c[0].stdout.clone(), Some(Redirect { path: "f".to_string(), append: true }));
            let c = parse("a<f", &env()).unwrap();
            check!("stdin unspaced", c[0].stdin.clone(), Some("f".to_string()));
            let c = parse("a > x > y", &env()).unwrap();
            check!("the last one wins", c[0].stdout.clone(), Some(Redirect { path: "y".to_string(), append: false }));
            let c = parse("a > \\"my file\\"", &env()).unwrap();
            check!("quoted target", c[0].stdout.clone(), Some(Redirect { path: "my file".to_string(), append: false }));
            let c = parse("> f a", &env()).unwrap();
            check!("redirect first", (c[0].args.clone(), c[0].stdout.clone()), (v(&["a"]), Some(Redirect { path: "f".to_string(), append: false })));
        }

        #[test]
        fn each_stage_has_its_own_redirects() {
            let c = parse("a < in | b | c >> out", &env()).unwrap();
            check!("three stages", c.len(), 3);
            check!("first reads", (c[0].stdin.clone(), c[0].stdout.clone()), (Some("in".to_string()), None));
            check!("middle has none", (c[1].stdin.clone(), c[1].stdout.clone()), (None, None));
            check!("last appends", (c[2].stdin.clone(), c[2].stdout.clone()), (None, Some(Redirect { path: "out".to_string(), append: true })));
        }
        """,
        T("single_quotes_are_literal", "no expansion, no escapes", "argv(\"echo '$HOME \\\"x\\\" \\\\n'\")", "Ok(vec![v(&[\"echo\", \"$HOME \\\"x\\\" \\\\n\"])])"),
        T("braces_and_names", "${NAME}s and longer names", "argv(\"echo \\\"${NAME}s $NAME_x $A$A\\\"\")", "Ok(vec![v(&[\"echo\", \"worlds  11\"])])"),
        T("lone_dollar", "stays a dollar", "argv(\"echo \\\"$ x\\\" a$\")", "Ok(vec![v(&[\"echo\", \"$ x\", \"a$\"])])"),
        T("empty_expansions", "unquoted vanishes, quoted stays", "(argv(\"echo $EMPTY x\"), argv(\"echo \\\"$EMPTY\\\" x\"), argv(\"echo $NOPE x\"))", "(Ok(vec![v(&[\"echo\", \"x\"])]), Ok(vec![v(&[\"echo\", \"\", \"x\"])]), Ok(vec![v(&[\"echo\", \"x\"])]))"),
        T("backslash_escapes", "outside and inside quotes", "argv(\"echo a\\\\ b \\\\| \\\\> \\\"q\\\\\\\"r\\\" \\\"s\\\\nt\\\"\")", "Ok(vec![v(&[\"echo\", \"a b\", \"|\", \">\", \"q\\\"r\", \"s\\\\nt\"])])"),
        T("operators_inside_quotes_are_text", "quoted pipe and redirect", "argv(\"echo \\\"a|b\\\" 'c>d'\")", "Ok(vec![v(&[\"echo\", \"a|b\", \"c>d\"])])"),
        T("adjacent_pieces_make_one_word", "mixed quoting", "argv(\"echo a\\\"b c\\\"'d'e\")", "Ok(vec![v(&[\"echo\", \"ab cde\"])])"),
        T("empty_arguments_are_arguments", "two empty strings", "argv(\"echo \\\"\\\" ''\")", "Ok(vec![v(&[\"echo\", \"\", \"\"])])"),
        T("comments", "only at the start of a word", "(argv(\"echo hi # there\"), argv(\"echo a#b\"), argv(\"# only\"), argv(\"echo \\\"#x\\\"\"))", "(Ok(vec![v(&[\"echo\", \"hi\"])]), Ok(vec![v(&[\"echo\", \"a#b\"])]), Ok(vec![]), Ok(vec![v(&[\"echo\", \"#x\"])]))"),
        T("empty_command_errors", "bad pipelines", "(argv(\"| a\"), argv(\"a |\"), argv(\"a | | b\"), argv(\"> f\"))", "(Err(ParseError::EmptyCommand), Err(ParseError::EmptyCommand), Err(ParseError::EmptyCommand), Err(ParseError::EmptyCommand))"),
        T("redirect_target_errors", "nothing to open", "(argv(\"a >\"), argv(\"a > | b\"), argv(\"a > > f\"), argv(\"a < > f\"), argv(\"a <\"))", "(Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget), Err(ParseError::MissingRedirectTarget))"),
        T("quote_and_brace_errors", "unterminated things", "(argv(\"echo 'abc\"), argv(\"echo \\\"abc\"), argv(\"echo ${HOME\"), argv(\"echo \\\"${HOME\\\"\"))", "(Err(ParseError::UnterminatedQuote), Err(ParseError::UnterminatedQuote), Err(ParseError::BadVariable), Err(ParseError::BadVariable))"),
    ],
    wrong=dict(
        expands_in_single_quotes=sub(SH, "            '\\'' => {\n                let w = cur.get_or_insert_with(String::new);\n                i += 1;\n                loop {\n                    match chars.get(i) {\n                        None => return Err(ParseError::UnterminatedQuote),\n                        Some('\\'') => {\n                            i += 1;\n                            break;\n                        }\n                        Some(&c) => {", "            '\\'' => {\n                let w = cur.get_or_insert_with(String::new);\n                i += 1;\n                loop {\n                    match chars.get(i) {\n                        None => return Err(ParseError::UnterminatedQuote),\n                        Some('\\'') => {\n                            i += 1;\n                            break;\n                        }\n                        Some('$') => expand(&chars, &mut i, env, w)?,\n                        Some(&c) => {"),
        empty_quotes_vanish=sub(SH, "            if let Some(w) = cur.take() {\n                toks.push(Tok::Word(w));\n            }", "            if let Some(w) = cur.take() {\n                if !w.is_empty() {\n                    toks.push(Tok::Word(w));\n                }\n            }"),
        empty_expansion_is_an_empty_word=sub(SH, "                if !s.is_empty() {\n                    cur.get_or_insert_with(String::new).push_str(&s); // an empty unquoted expansion adds no word\n                }", "                cur.get_or_insert_with(String::new).push_str(&s);"),
        comment_anywhere=sub(SH, "            '#' if cur.is_none() => break, // a comment runs to the end of the line", "            '#' => break,"),
        pipe_needs_spaces=sub(SH, "            '|' => {\n                flush!();\n                toks.push(Tok::Pipe);\n                i += 1;\n            }", "            '|' if cur.is_none() => {\n                toks.push(Tok::Pipe);\n                i += 1;\n            }"),
        append_is_two_redirects=sub(SH, "                if chars.get(i + 1) == Some(&'>') {\n                    toks.push(Tok::Append);\n                    i += 2;\n                } else {\n                    toks.push(Tok::Out);\n                    i += 1;\n                }", "                toks.push(Tok::Out);\n                i += 1;"),
        trailing_pipe_accepted=sub(SH, "    if cur.args.is_empty() {\n        return Err(ParseError::EmptyCommand);\n    }\n    cmds.push(cur);\n    Ok(cmds)", "    if !cur.args.is_empty() {\n        cmds.push(cur);\n    }\n    Ok(cmds)"),
        dangling_redirect_accepted=sub(SH, "    if pending.is_some() {\n        return Err(ParseError::MissingRedirectTarget);\n    }\n    if cur.args.is_empty() {", "    if cur.args.is_empty() {"),
        escapes_every_char_in_double_quotes=sub(SH, "Some('\\\\') if matches!(chars.get(i + 1), Some('\"') | Some('\\\\') | Some('$')) => {", "Some('\\\\') if chars.get(i + 1).is_some() => {"),
        unterminated_brace_is_literal=sub(SH, "chars[*i..].iter().position(|&c| c == '}').ok_or(ParseError::BadVariable)?;", "chars[*i..].iter().position(|&c| c == '}').unwrap_or(chars.len() - *i - 1);"),
    ),
    hints=[("approach", "Two passes. `tokenize` walks the characters with a `cur: Option<String>` for the word in progress: `None` means 'between words', `Some(\"\")` means 'a word has started but is empty' (that is what `\"\"` produces). Operators flush the current word and emit a token. `parse` then folds tokens into commands, remembering a pending redirect so the next word becomes its target."),
           ("rust", "Collect `chars` into a `Vec<char>` and index with `chars.get(i)` so running off the end is a `None`, not a panic. A little `expand(chars, &mut i, env, out)` function that advances `i` and appends the value keeps the quote handling readable. A `macro_rules! flush` (or a closure) avoids repeating the 'push the word if any' code."),
           ("edge", "Check the order of errors: an unterminated quote is found while tokenising, so it beats any grammar error. A redirect followed directly by `|` or another redirect is `MissingRedirectTarget`, and a stage with only redirects is `EmptyCommand`. `$HOME_x` is the variable `HOME_x`, not `$HOME` followed by `_x`.")],
    notes=("This is the lexer-and-parser of `sh`, restricted to the parts that are pure syntax; real shells add word splitting, globbing, command substitution and `&&`/`;`/`&`, each of which can be layered on this structure. The quoting rules are where shell-injection bugs live: knowing that expansion happens after parsing (and that quoted expansions stay one word) is the whole reason `\"$var\"` is the safe form.", "O(line length)", "O(line length)"),
    follow_up="1) Add `;`, `&&` and `||` between pipelines and return a small AST. 2) Add word splitting for unquoted expansions (`IFS` whitespace) and see which of your tests change. 3) Add glob expansion (`*`, `?`) against a provided file list, applied only to unquoted words. 4) Add `$?` and `$$` as special variables. 5) Build the executor: `std::process::Command` with `Stdio::piped` between stages and a file for each redirect, and check a few pipelines against `sh -c`.",
    related=["S2", "Y8", "B4"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. Operational transformation
OT_TYPES = """
/// One edit by one site (a user), at **character** positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Insert `ch` so that it ends up at index `pos` (`0..=len`). `site` identifies the author and breaks ties.
    Insert { pos: usize, ch: char, site: u32 },
    /// Delete the character at index `pos` (`0..len`).
    Delete { pos: usize },
    /// Does nothing: what a delete becomes when the same character was already deleted.
    Nop,
}

/// Applies one op to a string. (Provided.)
pub fn apply(s: &str, op: &Op) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    match op {
        Op::Insert { pos, ch, .. } => chars.insert(*pos, *ch),
        Op::Delete { pos } => {
            chars.remove(*pos);
        }
        Op::Nop => {}
    }
    chars.into_iter().collect()
}

/// Applies a sequence of ops, in order. (Provided.)
pub fn apply_all(s: &str, ops: &[Op]) -> String {
    ops.iter().fold(s.to_string(), |acc, op| apply(&acc, op))
}
"""

OT_STARTER = OT_TYPES + """
/// Given two ops `a` and `b` made **concurrently** against the same text, returns `(a', b')` such that
/// `apply(apply(s, a), b') == apply(apply(s, b), a')`: `b'` is `b` as it must be applied *after* `a`, and `a'` is `a` as
/// it must be applied *after* `b`.
///
/// Two inserts at the same index are ordered by site: the **lower** site's character comes first (to the left).
pub fn transform(a: &Op, b: &Op) -> (Op, Op) {
    todo!()
}

/// The same for two whole sequences: `a` and `b` are lists of ops that were each made one after another against the same
/// starting text. Returns `(a', b')` with `apply_all(apply_all(s, a), b') == apply_all(apply_all(s, b), a')`.
pub fn transform_lists(a: &[Op], b: &[Op]) -> (Vec<Op>, Vec<Op>) {
    todo!()
}
"""

OT = OT_TYPES + """
pub fn transform(a: &Op, b: &Op) -> (Op, Op) {
    use Op::*;
    match (a, b) {
        (Nop, _) | (_, Nop) => (a.clone(), b.clone()),
        (Insert { pos: pa, site: sa, .. }, Insert { pos: pb, site: sb, .. }) => {
            if pa < pb || (pa == pb && sa < sb) {
                // a lands to the left of b: b shifts right
                (a.clone(), shift_insert(b, 1))
            } else {
                (shift_insert(a, 1), b.clone())
            }
        }
        (Insert { pos: pa, .. }, Delete { pos: pb }) => {
            if pa <= pb {
                (a.clone(), Delete { pos: pb + 1 }) // the deleted character moved right
            } else {
                (shift_back(a), b.clone()) // the insert point moved left
            }
        }
        (Delete { .. }, Insert { .. }) => {
            let (b2, a2) = transform(b, a);
            (a2, b2)
        }
        (Delete { pos: pa }, Delete { pos: pb }) => {
            if pa < pb {
                (a.clone(), Delete { pos: pb - 1 })
            } else if pa > pb {
                (Delete { pos: pa - 1 }, b.clone())
            } else {
                (Nop, Nop) // both removed the same character
            }
        }
    }
}

fn shift_insert(op: &Op, by: usize) -> Op {
    match op {
        Op::Insert { pos, ch, site } => Op::Insert { pos: pos + by, ch: *ch, site: *site },
        other => other.clone(),
    }
}

fn shift_back(op: &Op) -> Op {
    match op {
        Op::Insert { pos, ch, site } => Op::Insert { pos: pos - 1, ch: *ch, site: *site },
        other => other.clone(),
    }
}

pub fn transform_lists(a: &[Op], b: &[Op]) -> (Vec<Op>, Vec<Op>) {
    let mut b_cur = b.to_vec();
    let mut a_out = Vec::with_capacity(a.len());
    for a_op in a {
        let mut a_cur = a_op.clone();
        for b_op in b_cur.iter_mut() {
            let (a2, b2) = transform(&a_cur, b_op);
            a_cur = a2;
            *b_op = b2;
        }
        a_out.push(a_cur);
    }
    (a_out, b_cur)
}
"""


def I(pos, ch, site):
    return f"Op::Insert {{ pos: {pos}, ch: '{ch}', site: {site} }}"


def D(pos):
    return f"Op::Delete {{ pos: {pos} }}"


OT_HELPERS = """
        fn random_op(rng: &mut anneal_prelude::Rng, text: &str, site: u32) -> Op {
            let len = text.chars().count();
            if len > 0 && rng.below(3) == 0 {
                Op::Delete { pos: rng.below(len) }
            } else {
                Op::Insert { pos: rng.below(len + 1), ch: (b'a' + rng.below(26) as u8) as char, site }
            }
        }

        fn random_seq(rng: &mut anneal_prelude::Rng, start: &str, site: u32, n: usize) -> Vec<Op> {
            let mut text = start.to_string();
            let mut ops = Vec::new();
            for _ in 0..n {
                let op = random_op(rng, &text, site);
                text = apply(&text, &op);
                ops.push(op);
            }
            ops
        }
"""

P.append(dict(
    slug="ot-collaborative-text", title="Operational transformation: two people edit the same text", level="hard", stage="build",
    tags=["operational transformation", "collaborative editing", "convergence", "concurrency", "Google Docs", "tie-breaking", "distributed systems"],
    teaches=["Two users edit the same text at once; each applies their own edit immediately and sends it over. To converge, each side must **transform** the other's edit against what it already did, so the same final text appears everywhere.",
             "The convergence condition: `apply(apply(s, a), b') == apply(apply(s, b), a')`. Every case of the transform function (insert/insert, insert/delete, delete/delete) is checked against it.",
             "Ties need a deterministic rule that both sides compute identically (lower site id first); otherwise two inserts at the same index land in opposite orders on the two machines."],
    statement="""
        Implement the transformation function at the heart of OT-based editors (the family behind Google Docs and Etherpad), for
        single-character edits. `Op::Insert { pos, ch, site }` inserts at a character index, `Op::Delete { pos }` removes the
        character at an index, `Op::Nop` does nothing. `apply` and `apply_all` are provided.

        `transform(a, b)` takes two ops made concurrently against the **same** text and returns `(a', b')` with
        `apply(apply(s, a), b') == apply(apply(s, b), a')` for every text `s` both ops are valid on. Cases:

        - insert vs insert: the one at the smaller index stays, the other shifts right by one; at the **same index the lower `site`
          goes first** (stays), the higher shifts;
        - insert vs delete: if the insert index is `<=` the deleted index, the delete moves right by one; otherwise the insert moves
          left by one;
        - delete vs delete: the one at the smaller index stays, the other index drops by one; deleting the same character twice
          becomes `Nop` on both sides;
        - `Nop` transforms with anything unchanged.

        `transform_lists(a, b)` lifts that to two whole sequences of ops, each made one after another against the same start text, so
        that applying `a` then `b'` equals applying `b` then `a'`.
    """,
    starter=OT_STARTER,
    solution=OT,
    visible=[
        T("inserts_at_different_positions", "left stays, right shifts", "transform(&" + I(2, 'x', 1) + ", &" + I(5, 'y', 2) + ")", "(" + I(2, 'x', 1) + ", " + I(6, 'y', 2) + ")"),
        T("inserts_at_the_same_position_lower_site_first", "tie on index", "(transform(&" + I(3, 'x', 1) + ", &" + I(3, 'y', 2) + "), transform(&" + I(3, 'y', 2) + ", &" + I(3, 'x', 1) + "))", "((" + I(3, 'x', 1) + ", " + I(4, 'y', 2) + "), (" + I(4, 'y', 2) + ", " + I(3, 'x', 1) + "))"),
        T("insert_before_a_delete", "the deleted character moves right", "transform(&" + I(2, 'x', 1) + ", &" + D(2) + ")", "(" + I(2, 'x', 1) + ", " + D(3) + ")"),
        T("insert_after_a_delete", "the insert point moves left", "transform(&" + I(5, 'x', 1) + ", &" + D(2) + ")", "(" + I(4, 'x', 1) + ", " + D(2) + ")"),
        T("delete_the_same_character", "both become Nop", "transform(&" + D(4) + ", &" + D(4) + ")", "(Op::Nop, Op::Nop)"),
        T("deletes_at_different_positions", "the later one moves left", "(transform(&" + D(1) + ", &" + D(4) + "), transform(&" + D(4) + ", &" + D(1) + "))", "((" + D(1) + ", " + D(3) + "), (" + D(3) + ", " + D(1) + "))"),
        T("a_concrete_session", "abcdef, both converge", "{ let s = \"abcdef\"; let a = " + I(2, 'X', 1) + "; let b = " + D(3) + "; let (a2, b2) = transform(&a, &b); (apply(&apply(s, &a), &b2), apply(&apply(s, &b), &a2)) }", "(\"abXcef\".to_string(), \"abXcef\".to_string())"),
    ],
    hidden=[
        T("delete_vs_insert_mirrors_insert_vs_delete", "arguments swapped", "(transform(&" + D(2) + ", &" + I(2, 'x', 1) + "), transform(&" + D(2) + ", &" + I(5, 'x', 1) + "))", "((" + D(3) + ", " + I(2, 'x', 1) + "), (" + D(2) + ", " + I(4, 'x', 1) + "))"),
        T("nop_is_neutral", "both sides", "(transform(&Op::Nop, &" + I(1, 'a', 1) + "), transform(&" + D(0) + ", &Op::Nop), transform(&Op::Nop, &Op::Nop))", "((Op::Nop, " + I(1, 'a', 1) + "), (" + D(0) + ", Op::Nop), (Op::Nop, Op::Nop))"),
        T("insert_at_the_deleted_position", "boundary: <= moves the delete", "transform(&" + I(3, 'x', 1) + ", &" + D(3) + ")", "(" + I(3, 'x', 1) + ", " + D(4) + ")"),
        T("insert_right_after_the_deleted_character", "boundary: > moves the insert", "transform(&" + I(4, 'x', 1) + ", &" + D(3) + ")", "(" + I(3, 'x', 1) + ", " + D(3) + ")"),
        T("two_inserts_at_the_start_and_end", "extreme positions", "(transform(&" + I(0, 'a', 2) + ", &" + I(0, 'b', 1) + "), transform(&" + I(9, 'a', 1) + ", &" + I(9, 'b', 2) + "))", "((" + I(1, 'a', 2) + ", " + I(0, 'b', 1) + "), (" + I(9, 'a', 1) + ", " + I(10, 'b', 2) + "))"),
        T("lists_of_ops", "two edits each", "{ let s = \"hello\"; let a = vec![" + I(0, '>', 1) + ", " + D(3) + "]; let b = vec![" + I(5, '!', 2) + ", " + D(0) + "]; let (a2, b2) = transform_lists(&a, &b); (apply_all(&apply_all(s, &a), &b2), apply_all(&apply_all(s, &b), &a2), a2.len(), b2.len()) }", "(\">elo!\".to_string(), \">elo!\".to_string(), 2, 2)"),
        T("empty_lists", "nothing to transform", "{ let a = vec![" + I(0, 'x', 1) + "]; let (a2, b2) = transform_lists(&a, &[]); (a2 == a, b2.len()) }", "(true, 0)"),
        T("both_delete_everything", "same deletions become Nops", "{ let s = \"abc\"; let a = vec![" + D(0) + ", " + D(0) + ", " + D(0) + "]; let (a2, b2) = transform_lists(&a, &a); (apply_all(&apply_all(s, &a), &b2), apply_all(&apply_all(s, &a), &a2) == \"\", b2.iter().all(|o| *o == Op::Nop)) }", "(\"\".to_string(), true, true)"),
        OT_HELPERS + """
        #[test]
        fn single_ops_always_converge() {
            let mut rng = anneal_prelude::Rng::new(2024);
            for round in 0..3000 {
                let len = rng.below(8);
                let s: String = (0..len).map(|_| (b'a' + rng.below(26) as u8) as char).collect();
                let a = random_op(&mut rng, &s, 1);
                let b = random_op(&mut rng, &s, 2);
                let (a2, b2) = transform(&a, &b);
                let left = apply(&apply(&s, &a), &b2);
                let right = apply(&apply(&s, &b), &a2);
                check!(format!("round {round}: {s:?} a={a:?} b={b:?}"), left, right);
            }
        }

        #[test]
        fn sequences_always_converge() {
            let mut rng = anneal_prelude::Rng::new(99);
            for round in 0..1500 {
                let len = rng.below(10);
                let s: String = (0..len).map(|_| (b'A' + rng.below(26) as u8) as char).collect();
                let (na, nb) = (rng.below(6), rng.below(6));
                let a = random_seq(&mut rng, &s, 1, na);
                let b = random_seq(&mut rng, &s, 2, nb);
                let (a2, b2) = transform_lists(&a, &b);
                check!(format!("round {round}: lengths are kept"), (a2.len(), b2.len()), (a.len(), b.len()));
                let left = apply_all(&apply_all(&s, &a), &b2);
                let right = apply_all(&apply_all(&s, &b), &a2);
                check!(format!("round {round}: {s:?} a={a:?} b={b:?}"), left, right);
            }
        }

        #[test]
        fn identical_characters_stress_the_tie_breaks() {
            // all characters equal, so only positions and sites decide: any mishandled tie shows up as a different length or order of edits
            let mut rng = anneal_prelude::Rng::new(5);
            for round in 0..800 {
                let len = rng.below(6);
                let s: String = (0..len).map(|_| 'z').collect();
                let (na, nb) = (rng.below(5), rng.below(5));
                let a = random_seq(&mut rng, &s, 1, na);
                let b = random_seq(&mut rng, &s, 2, nb);
                let (a_for_b, b_for_a) = transform_lists(&a, &b);
                let at_a = apply_all(&apply_all(&s, &a), &b_for_a);
                let at_b = apply_all(&apply_all(&s, &b), &a_for_b);
                check!(format!("round {round}: replicas agree"), at_a, at_b);
            }
        }
        """,
    ],
    wrong=dict(
        tie_shifts_both=sub(OT, "            if pa < pb || (pa == pb && sa < sb) {\n                // a lands to the left of b: b shifts right\n                (a.clone(), shift_insert(b, 1))\n            } else {\n                (shift_insert(a, 1), b.clone())\n            }", "            if pa < pb {\n                (a.clone(), shift_insert(b, 1))\n            } else if pa > pb {\n                (shift_insert(a, 1), b.clone())\n            } else {\n                let _ = (sa, sb);\n                (shift_insert(a, 1), shift_insert(b, 1))\n            }"),
        higher_site_goes_first=sub(OT, "if pa < pb || (pa == pb && sa < sb) {", "if pa < pb || (pa == pb && sa > sb) {"),
        insert_at_the_deleted_position_moves_the_insert=sub(OT, "            if pa <= pb {\n                (a.clone(), Delete { pos: pb + 1 }) // the deleted character moved right", "            if pa < pb {\n                (a.clone(), Delete { pos: pb + 1 })"),
        insert_after_a_delete_is_not_moved=sub(OT, "                (shift_back(a), b.clone()) // the insert point moved left", "                (a.clone(), b.clone())"),
        double_delete_is_not_a_nop=sub(OT, "                (Nop, Nop) // both removed the same character", "                (b.clone(), a.clone())"),
        later_delete_keeps_its_index=sub(OT, "            if pa < pb {\n                (a.clone(), Delete { pos: pb - 1 })\n            } else if pa > pb {\n                (Delete { pos: pa - 1 }, b.clone())", "            if pa < pb {\n                (a.clone(), b.clone())\n            } else if pa > pb {\n                (a.clone(), b.clone())"),
        lists_leave_b_untransformed=sub(OT, "    (a_out, b_cur)", "    (a_out, b.to_vec())"),
        lists_do_not_update_a_between_steps=sub(OT, "            let (a2, b2) = transform(&a_cur, b_op);\n            a_cur = a2;\n            *b_op = b2;", "            let (_, b2) = transform(&a_cur, b_op);\n            *b_op = b2;"),
        insert_vs_insert_ignores_the_position=sub(OT, "            if pa < pb || (pa == pb && sa < sb) {", "            if sa < sb {"),
    ),
    hints=[("approach", "Handle the cases one pair at a time and test each against the convergence equation by hand on a short string. Inserts shift the indexes of everything at or after them; deletes shift the indexes of everything after them. The only subtle case is two inserts at the same index, where a deterministic tie-break (the lower `site`) makes both machines choose the same order."),
           ("rust", "`match (a, b)` with a case per pair; for `(Delete, Insert)` call `transform(b, a)` and swap the results rather than writing the mirror image twice. For lists: for each op of `a`, transform it against every (already updated) op of `b` in turn, replacing both: `a` is carried along, each `b` op is updated in place, and the finished `a` op is pushed to the output."),
           ("edge", "`pa <= pb` for insert-vs-delete: an insert at index 3 and a delete of the character currently at index 3 means the new character goes *before* it, so the delete must move to index 4. Deleting the same character on both sides must become `Nop` on both, or one side removes a second, innocent character.")],
    notes=("This is Ellis and Gibbs' 1989 OT as used in Google Wave/Docs and Etherpad. The control algorithm here (transform a list against a list, one op at a time) is the 'grid' every OT server runs per client; production systems add range deletes, undo, and a central server to totally order operations because the transformation functions for more than two concurrent users are notoriously hard to get right (the TP2 problem). CRDTs (RGA, Yjs, Automerge) avoid transformation entirely by giving each character a unique id.", "O(len(a) x len(b)) transformations", "O(len(a) + len(b))"),
    follow_up="1) Generalise to ranged operations (`Insert { pos, text }`, `Delete { pos, len }`) and handle a delete range that an insert lands inside (the delete splits in two). 2) Write a tiny central server: it keeps a history of applied ops with a version number; a client sends `(base_version, op)` and the server transforms against everything since. Test with random interleavings of three clients. 3) Implement undo as a transformed inverse of an earlier op. 4) Implement an RGA (replicated growable array) CRDT with per-character ids and show it needs no transform function. 5) Find a three-way scenario where naive pairwise transformation diverges and explain why the server's total order fixes it.",
    related=["Y7", "C2", "S2"],
))

# ---------------------------------------------------------------------------------------------------------------
# 3. KV store with a write-ahead log
KV_COMMON = """
use std::collections::BTreeMap;

/// FNV-1a, 32 bit: the record checksum. (Provided.)
pub fn checksum(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// `vlen` value that marks a delete (a tombstone): the record has no value bytes.
pub const TOMBSTONE: u32 = u32::MAX;

/// A key-value store that logs every change to a byte-vector write-ahead log before it applies it.
///
/// Log record: `[crc: u32][klen: u32][vlen: u32][key][value]`, integers little-endian. `vlen == TOMBSTONE` means delete (no value
/// bytes follow). `crc` is `checksum` of everything after the crc field (klen, vlen, key, value).
pub struct Store {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
    log: Vec<u8>,
}
"""

KV_STARTER = KV_COMMON + """
impl Store {
    pub fn new() -> Store {
        todo!()
    }

    /// Appends a put record to the log, then updates the map.
    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        todo!()
    }

    /// Appends a tombstone record, then removes the key. (Deleting a missing key still logs it.)
    pub fn delete(&mut self, key: &[u8]) {
        todo!()
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        todo!()
    }

    /// The number of live keys.
    pub fn len(&self) -> usize {
        todo!()
    }

    /// The write-ahead log: what would be on disk.
    pub fn log(&self) -> &[u8] {
        todo!()
    }

    /// Rebuilds a store by replaying `log` from the start. Stops at the first record that is incomplete (a torn write at the
    /// tail) or whose checksum is wrong, and ignores everything from there on. Returns the store and the number of bytes that
    /// were valid. The returned store's own log is exactly that valid prefix, so later writes append cleanly.
    pub fn recover(log: &[u8]) -> (Store, usize) {
        todo!()
    }

    /// Rewrites the log so it holds one put record per live key (in key order) and nothing else.
    pub fn compact(&mut self) {
        todo!()
    }
}
"""

KV = KV_COMMON + """
fn record(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let vlen = value.map_or(TOMBSTONE, |v| v.len() as u32);
    let mut body = Vec::new();
    body.extend_from_slice(&(key.len() as u32).to_le_bytes());
    body.extend_from_slice(&vlen.to_le_bytes());
    body.extend_from_slice(key);
    if let Some(v) = value {
        body.extend_from_slice(v);
    }
    let mut rec = checksum(&body).to_le_bytes().to_vec();
    rec.extend_from_slice(&body);
    rec
}

impl Store {
    pub fn new() -> Store {
        Store { map: BTreeMap::new(), log: Vec::new() }
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        self.log.extend_from_slice(&record(key, Some(value))); // log first, then apply
        self.map.insert(key.to_vec(), value.to_vec());
    }

    pub fn delete(&mut self, key: &[u8]) {
        self.log.extend_from_slice(&record(key, None));
        self.map.remove(key);
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.map.get(key).map(|v| v.as_slice())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn log(&self) -> &[u8] {
        &self.log
    }

    pub fn recover(log: &[u8]) -> (Store, usize) {
        let mut map = BTreeMap::new();
        let mut at = 0usize;
        while log.len() - at >= 12 {
            let crc = u32::from_le_bytes(log[at..at + 4].try_into().unwrap());
            let klen = u32::from_le_bytes(log[at + 4..at + 8].try_into().unwrap());
            let vlen = u32::from_le_bytes(log[at + 8..at + 12].try_into().unwrap());
            let value_len = if vlen == TOMBSTONE { 0 } else { vlen as usize };
            let body_len = 8 + klen as usize + value_len; // klen and vlen fields, key, value
            let end = at + 4 + body_len;
            if end > log.len() {
                break; // torn: the record claims more bytes than exist
            }
            if checksum(&log[at + 4..end]) != crc {
                break; // corrupt
            }
            let key = log[at + 12..at + 12 + klen as usize].to_vec();
            if vlen == TOMBSTONE {
                map.remove(&key);
            } else {
                map.insert(key, log[at + 12 + klen as usize..end].to_vec());
            }
            at = end;
        }
        (Store { map, log: log[..at].to_vec() }, at)
    }

    pub fn compact(&mut self) {
        let mut log = Vec::new();
        for (k, v) in &self.map {
            log.extend_from_slice(&record(k, Some(v)));
        }
        self.log = log;
    }
}
"""

KV_HELPERS = WITHIN + """
        use std::collections::BTreeMap;

        /// The same operations applied to a plain map: the oracle.
        #[derive(Clone, Debug)]
        enum Step {
            Put(Vec<u8>, Vec<u8>),
            Del(Vec<u8>),
        }

        fn model(steps: &[Step]) -> BTreeMap<Vec<u8>, Vec<u8>> {
            let mut m = BTreeMap::new();
            for s in steps {
                match s {
                    Step::Put(k, v) => {
                        m.insert(k.clone(), v.clone());
                    }
                    Step::Del(k) => {
                        m.remove(k);
                    }
                }
            }
            m
        }

        fn contents(s: &Store) -> BTreeMap<Vec<u8>, Vec<u8>> {
            let mut out = BTreeMap::new();
            for k in 0u8..=255 {
                for key in [vec![k], vec![k, k], vec![b'k', k]] {
                    if let Some(v) = s.get(&key) {
                        out.insert(key, v.to_vec());
                    }
                }
            }
            out
        }

        /// Builds a store from the steps and returns it with the log length after each step.
        fn build(steps: &[Step]) -> (Store, Vec<usize>) {
            let mut s = Store::new();
            let mut ends = vec![0];
            for st in steps {
                match st {
                    Step::Put(k, v) => s.put(k, v),
                    Step::Del(k) => s.delete(k),
                }
                ends.push(s.log().len());
            }
            (s, ends)
        }

        fn sample_steps() -> Vec<Step> {
            vec![
                Step::Put(vec![1], b"one".to_vec()),
                Step::Put(vec![2, 2], b"two".to_vec()),
                Step::Put(vec![1], b"uno".to_vec()),
                Step::Del(vec![2, 2]),
                Step::Put(vec![b'k', 9], Vec::new()),
                Step::Put(vec![3], vec![7u8; 40]),
                Step::Del(vec![42]),
            ]
        }
"""

P.append(dict(
    slug="kv-store-wal", title="A key-value store with a write-ahead log that survives torn writes", level="hard", stage="build",
    tags=["write-ahead log", "durability", "crash recovery", "checksums", "tombstones", "compaction", "torn writes", "storage engines"],
    teaches=["Write-ahead logging: record the change in an append-only log *before* applying it, and the log alone is enough to rebuild the state after a crash.",
             "A crash can leave a **torn** last record (half written). Recovery must accept exactly the complete, checksum-valid prefix and stop at the first bad record, never apply garbage and never panic on a lying length field.",
             "Deletes are records too (tombstones), distinct from an empty value; compaction rewrites the log with only the live data."],
    statement="""
        Build the durability core of an LSM-style store (the shape of Bitcask, RocksDB's WAL, and Redis AOF). `Store` keeps a
        `BTreeMap` in memory and an append-only **log** (a `Vec<u8>` standing for the file). Every `put` and `delete` appends a record
        to the log first, then updates the map.

        A record is `[crc: u32][klen: u32][vlen: u32][key][value]` (little-endian). `crc` is the provided `checksum` of everything
        after the crc field. A delete is a record with `vlen == TOMBSTONE` (`u32::MAX`) and **no value bytes**; an empty value
        (`vlen == 0`) is a different thing.

        `Store::recover(log)` replays a log: it applies records in order and **stops** at the first one that is incomplete (the log ends
        before its bytes do) or whose checksum does not match, ignoring that record and everything after it. It returns the rebuilt
        store and the number of valid bytes; the store's own log is exactly that valid prefix, so new writes append cleanly after a
        torn tail. Lengths in a damaged record can be anything (up to 4 GB): never trust them for an allocation or an index.

        `compact` rewrites the log as one put record per live key in key order (no tombstones, no overwritten values).
    """,
    starter=KV_STARTER,
    solution=KV,
    visible=[
        T("put_get_delete", "the basic map behaviour", "{ let mut s = Store::new(); s.put(b\"a\", b\"1\"); s.put(b\"b\", b\"2\"); s.put(b\"a\", b\"3\"); s.delete(b\"b\"); (s.get(b\"a\").map(|v| v.to_vec()), s.get(b\"b\").map(|v| v.to_vec()), s.len()) }", "(Some(b\"3\".to_vec()), None, 1)"),
        T("a_put_record_layout", "header and payload", "{ let mut s = Store::new(); s.put(b\"k\", b\"vv\"); let l = s.log().to_vec(); (l.len(), l[4..12].to_vec(), l[12..].to_vec(), u32::from_le_bytes(l[0..4].try_into().unwrap()) == checksum(&l[4..])) }", "(15, vec![1, 0, 0, 0, 2, 0, 0, 0], b\"kvv\".to_vec(), true)"),
        T("a_delete_is_a_tombstone_record", "vlen is u32::MAX, no value bytes", "{ let mut s = Store::new(); s.delete(b\"k\"); let l = s.log().to_vec(); (l.len(), l[8..12].to_vec()) }", "(13, vec![255, 255, 255, 255])"),
        T("recover_replays_the_log", "same contents, all bytes valid", "{ let mut s = Store::new(); s.put(b\"a\", b\"1\"); s.put(b\"b\", b\"2\"); s.delete(b\"a\"); let (r, n) = Store::recover(s.log()); (r.get(b\"a\").is_none(), r.get(b\"b\").map(|v| v.to_vec()), r.len(), n == s.log().len(), r.log() == s.log()) }", "(true, Some(b\"2\".to_vec()), 1, true, true)"),
        T("a_torn_tail_is_dropped", "cut in the middle of the last record", "{ let mut s = Store::new(); s.put(b\"a\", b\"1\"); let good = s.log().len(); s.put(b\"b\", b\"22\"); let cut = &s.log()[..s.log().len() - 1]; let (r, n) = Store::recover(cut); (r.get(b\"a\").map(|v| v.to_vec()), r.get(b\"b\").is_none(), n == good) }", "(Some(b\"1\".to_vec()), true, true)"),
        T("a_flipped_bit_stops_recovery", "checksum catches it", "{ let mut s = Store::new(); s.put(b\"a\", b\"1\"); let good = s.log().len(); s.put(b\"b\", b\"2\"); s.put(b\"c\", b\"3\"); let mut bad = s.log().to_vec(); bad[good + 13] ^= 1; let (r, n) = Store::recover(&bad); (r.len(), r.get(b\"c\").is_none(), n == good) }", "(1, true, true)"),
    ],
    hidden=[
        KV_HELPERS + """
        #[test]
        fn every_truncation_point_recovers_a_consistent_prefix() {
            let steps = sample_steps();
            let (s, ends) = build(&steps);
            let log = s.log().to_vec();
            for cut in 0..=log.len() {
                let (r, n) = Store::recover(&log[..cut]);
                // the number of complete records inside the first `cut` bytes
                let complete = ends.iter().rposition(|&e| e <= cut).unwrap();
                check!(format!("cut {cut}: valid bytes"), n, ends[complete]);
                check!(format!("cut {cut}: contents"), contents(&r), model(&steps[..complete]));
                check!(format!("cut {cut}: the store's own log is the valid prefix"), r.log().to_vec(), log[..n].to_vec());
            }
        }

        #[test]
        fn corrupting_any_byte_never_applies_the_damaged_record_or_later_ones() {
            let steps = sample_steps();
            let (s, ends) = build(&steps);
            let log = s.log().to_vec();
            for pos in 0..log.len() {
                for flip in [0x01u8, 0x80, 0xFF] {
                    let mut bad = log.clone();
                    bad[pos] ^= flip;
                    let (r, n) = Store::recover(&bad);
                    let record = ends.iter().rposition(|&e| e <= pos).unwrap(); // index of the damaged record
                    check!(format!("byte {pos} ^ {flip:#x}: valid bytes stop at the damaged record"), n, ends[record]);
                    check!(format!("byte {pos}: contents are the state before it"), contents(&r), model(&steps[..record]));
                }
            }
        }

        #[test]
        fn writes_after_a_torn_tail_append_cleanly() {
            let steps = sample_steps();
            let (s, ends) = build(&steps);
            let torn = &s.log()[..ends[3] + 5]; // three whole records and 5 stray bytes
            let (mut r, n) = Store::recover(torn);
            check!("valid prefix", n, ends[3]);
            r.put(b"new", b"value");
            let (again, m) = Store::recover(r.log());
            check!("the new record is recovered, nothing is lost", m, r.log().len());
            check!("contents", again.get(b"new").map(|v| v.to_vec()), Some(b"value".to_vec()));
            check!("and the earlier records are still there", again.get(&[1]).map(|v| v.to_vec()), Some(b"uno".to_vec()));
        }

        #[test]
        fn absurd_lengths_do_not_panic_or_allocate() {
            // a 12-byte header that claims a 4 GB key and a 4 GB value, with a checksum that cannot match
            let mut log = vec![0u8; 12];
            log[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
            log[8..12].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
            let (r, n) = within(5000, move || { let (r, n) = Store::recover(&log); (r.len(), n) }).expect("recovery must be quick");
            check!("nothing recovered", (r, n), (0, 0));
        }

        #[test]
        fn compaction_keeps_exactly_the_live_data() {
            let mut s = Store::new();
            for round in 0..50u8 {
                for k in 0..10u8 {
                    s.put(&[k], &[round, k]);
                }
            }
            for k in 0..5u8 {
                s.delete(&[k]);
            }
            let before = s.log().len();
            let live: Vec<(u8, Vec<u8>)> = (5..10u8).map(|k| (k, vec![49, k])).collect();
            s.compact();
            check!("the log shrank a lot", s.log().len() < before / 20, true);
            check!("five records of 13 + 3 bytes", s.log().len(), 5 * (12 + 1 + 2));
            check!("still the same keys", s.len(), 5);
            for (k, v) in &live {
                check!(format!("key {k}"), s.get(&[*k]).map(|x| x.to_vec()), Some(v.clone()));
            }
            let (r, n) = Store::recover(s.log());
            check!("a compacted log recovers", (r.len(), n == s.log().len()), (5, true));
            check!("no tombstones survive", s.log().windows(4).all(|w| w != [255, 255, 255, 255]), true);
        }
        """,
        T("empty_value_is_not_a_delete", "vlen 0 vs tombstone", "{ let mut s = Store::new(); s.put(b\"k\", b\"\"); let a = s.get(b\"k\").map(|v| v.len()); let (r, _) = Store::recover(s.log()); let b = r.get(b\"k\").map(|v| v.len()); s.delete(b\"k\"); (a, b, s.get(b\"k\").is_none()) }", "(Some(0), Some(0), true)"),
        T("empty_key_is_allowed", "klen 0", "{ let mut s = Store::new(); s.put(b\"\", b\"v\"); let (r, n) = Store::recover(s.log()); (r.get(b\"\").map(|v| v.to_vec()), n == s.log().len()) }", "(Some(b\"v\".to_vec()), true)"),
        T("deleting_a_missing_key_is_logged_and_harmless", "a tombstone for nothing", "{ let mut s = Store::new(); s.delete(b\"ghost\"); let (r, n) = Store::recover(s.log()); (s.log().len() > 0, r.len(), n == s.log().len()) }", "(true, 0, true)"),
        T("delete_then_put_again", "the key comes back", "{ let mut s = Store::new(); s.put(b\"k\", b\"1\"); s.delete(b\"k\"); s.put(b\"k\", b\"2\"); let (r, _) = Store::recover(s.log()); r.get(b\"k\").map(|v| v.to_vec()) }", "Some(b\"2\".to_vec())"),
        T("empty_log_recovers_to_an_empty_store", "nothing to replay", "{ let (r, n) = Store::recover(&[]); (r.len(), n, r.log().len()) }", "(0, 0, 0)"),
        T("big_values", "100 KB", "{ let mut s = Store::new(); s.put(b\"big\", &vec![9u8; 100_000]); let (r, n) = Store::recover(s.log()); (r.get(b\"big\").map(|v| v.len()), n == s.log().len()) }", "(Some(100_000), true)"),
        T("recover_of_a_log_with_trailing_garbage_keeps_the_prefix", "7 stray bytes", "{ let mut s = Store::new(); s.put(b\"a\", b\"1\"); let good = s.log().len(); let mut l = s.log().to_vec(); l.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7]); let (r, n) = Store::recover(&l); (r.len(), n == good, r.log().len() == good) }", "(1, true, true)"),
    ],
    wrong=dict(
        recovery_ignores_the_checksum=sub(KV, "            if checksum(&log[at + 4..end]) != crc {\n                break; // corrupt\n            }\n", ""),
        torn_record_is_not_detected=sub(KV, "            if end > log.len() {\n                break; // torn: the record claims more bytes than exist\n            }\n", ""),
        valid_length_is_the_whole_input=sub(KV, "        (Store { map, log: log[..at].to_vec() }, at)", "        (Store { map, log: log.to_vec() }, log.len())"),
        tombstone_is_an_empty_value=sub(KV, "    let vlen = value.map_or(TOMBSTONE, |v| v.len() as u32);", "    let vlen = value.map_or(0, |v| v.len() as u32);"),
        compaction_keeps_tombstones=sub(KV, "        let mut log = Vec::new();\n        for (k, v) in &self.map {\n            log.extend_from_slice(&record(k, Some(v)));\n        }\n        self.log = log;", "        let mut log = self.log.clone();\n        log.extend_from_slice(&record(b\"\", None));\n        self.log = log;"),
        checksum_does_not_cover_the_header=sub(KV, "    let mut rec = checksum(&body).to_le_bytes().to_vec();", "    let mut rec = checksum(&body[8..]).to_le_bytes().to_vec();"),
        recover_checks_after_applying_so_bad_data_lands=sub(KV, "            if checksum(&log[at + 4..end]) != crc {\n                break; // corrupt\n            }\n            let key = log[at + 12..at + 12 + klen as usize].to_vec();\n            if vlen == TOMBSTONE {\n                map.remove(&key);\n            } else {\n                map.insert(key, log[at + 12 + klen as usize..end].to_vec());\n            }\n            at = end;", "            let key = log[at + 12..at + 12 + klen as usize].to_vec();\n            if vlen == TOMBSTONE {\n                map.remove(&key);\n            } else {\n                map.insert(key, log[at + 12 + klen as usize..end].to_vec());\n            }\n            if checksum(&log[at + 4..end]) != crc {\n                break;\n            }\n            at = end;"),
        put_updates_the_map_but_not_the_log=sub(KV, "        self.log.extend_from_slice(&record(key, Some(value))); // log first, then apply\n", ""),
        delete_does_not_log=sub(KV, "        self.log.extend_from_slice(&record(key, None));\n        self.map.remove(key);", "        self.map.remove(key);"),
    ),
    hints=[("approach", "One `record(key, Option<value>)` function builds the bytes for both puts and deletes and is the only place the format is written; `put`/`delete` append it, then update the map. `recover` is a loop over offsets: read the 12-byte header, work out the record's end, **check it fits**, check the checksum, apply it, move on; the first failure ends the loop."),
           ("rust", "Use `u32::from_le_bytes(log[a..b].try_into().unwrap())` for the header fields. Do the end-of-record arithmetic in `usize` (no overflow on 64-bit even for 4 GB lengths) and compare with `log.len()` before slicing. The checksum covers `log[at + 4..end]`. After the loop, the recovered log is `log[..at].to_vec()`."),
           ("edge", "Test the length before trusting any field: a record whose `klen` is 4 GB must be 'torn', not a panic or a giant allocation. Tombstones have *no* value bytes, so their body length uses 0, not `u32::MAX`. Stop at the first bad record: records after a corrupt one may be valid by themselves, but you can no longer know where they start.")],
    notes=("This is the recovery path of Bitcask, LevelDB/RocksDB's WAL, Redis AOF and SQLite's journal in miniature. Real systems add an fsync policy (the log is only durable once it is synced; that is the throughput/safety trade-off), record batching, a sequence number to detect a log that was truncated in the middle rather than at the end, and compaction that runs in the background into a new file followed by an atomic rename.", "put/delete O(key+value); recover O(log); compact O(live data)", "O(live data + log)"),
    follow_up="1) Add an `fsync` model: a `File` trait with `append` and `sync`, a fake that loses unsynced bytes on a simulated crash, and a test that finds a bug when you forget to sync before acknowledging. 2) Add a sequence number to each record and use it to detect a log with a hole in the middle. 3) Add a snapshot + log scheme: `compact` writes a snapshot and truncates the log, recovery loads the snapshot then the log; what must happen atomically? 4) Make records batchable (one checksum for a group of puts that must apply together or not at all). 5) Build the next layer: an in-memory memtable flushed to sorted immutable files (an SSTable) and a merge read path.",
    related=["F4", "B3", "Y7"],
))

STAGES = [("build", "Build it", "hard")]

if __name__ == "__main__":
    P.sort(key=lambda p: {"medium": 0, "hard": 1}[p["level"]])
    n = write_track("y9-capstone-builds", "Y9", "Capstone builds: parts of real systems", "Y", "sde3", 9,
                    "Build the hard core of real programs: a shell's command-line parser, an operational-transformation text editor, and a crash-safe key-value log.",
                    STAGES, P)
    print("Y9", n)
