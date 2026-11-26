//! A Wadler-style document algebra and its pretty printer.
//!
//! A [`Doc`] describes *possible* layouts; the printer picks one. `Group` is the
//! only choice point: a group is printed flat (every `Line` is a space, every
//! `SoftLine` is nothing) when its content fits in the remaining width, otherwise
//! its own `Line`s become newlines. Nested groups decide independently, so a long
//! call breaks its arguments while the short calls inside them stay on one line.
//!
//! WHY `BreakParent`/`HardLine` make `fits` fail: a line comment must be followed
//! by a newline, so any group containing one must never be printed flat.

/// The document algebra.
#[derive(Clone, Debug)]
pub enum Doc {
    Nil,
    Text(String),
    /// A space when flat, a newline when broken.
    Line,
    /// Nothing when flat, a newline when broken.
    SoftLine,
    /// Always a newline (and forces every enclosing group to break).
    HardLine,
    /// Prints nothing but forces every enclosing group to break.
    BreakParent,
    Concat(Vec<Doc>),
    /// Indent line breaks inside by the given number of columns.
    Nest(usize, Box<Doc>),
    Group(Box<Doc>),
    /// `IfBreak(broken, flat)`: pick by the mode of the enclosing group.
    IfBreak(Box<Doc>, Box<Doc>),
}

pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}
pub fn concat(v: Vec<Doc>) -> Doc {
    Doc::Concat(v)
}
pub fn nest(n: usize, d: Doc) -> Doc {
    Doc::Nest(n, Box::new(d))
}
pub fn group(d: Doc) -> Doc {
    Doc::Group(Box::new(d))
}
pub fn if_break(broken: Doc, flat: Doc) -> Doc {
    Doc::IfBreak(Box::new(broken), Box::new(flat))
}
/// Join `docs` with `sep` between them.
pub fn join(sep: &Doc, docs: Vec<Doc>) -> Doc {
    let mut out = Vec::with_capacity(docs.len() * 2);
    for (i, d) in docs.into_iter().enumerate() {
        if i > 0 {
            out.push(sep.clone());
        }
        out.push(d);
    }
    Doc::Concat(out)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

type Cmd<'a> = (usize, Mode, &'a Doc);

/// Display width of a text fragment, measured on its last line.
fn width_of(s: &str) -> usize {
    s.rsplit('\n').next().unwrap_or("").chars().count()
}

/// Does everything up to the next possible line break fit in `rem` columns?
/// `rest` is the stack of pending commands (already in their own modes).
fn fits(mut rem: isize, first: Cmd, rest: &[Cmd]) -> bool {
    let mut stack: Vec<Cmd> = vec![first];
    let mut rest_iter = rest.iter().rev();
    loop {
        let (ind, mode, doc) = match stack.pop() {
            Some(c) => c,
            None => match rest_iter.next() {
                Some(&c) => c,
                None => return true,
            },
        };
        match doc {
            Doc::Nil => {}
            Doc::Text(s) => {
                if s.contains('\n') {
                    // Multi-line text (a block comment) cannot be laid out flat; in break
                    // mode what follows it starts a fresh line, so the check ends there.
                    return mode == Mode::Break;
                }
                rem -= s.chars().count() as isize;
                if rem < 0 {
                    return false;
                }
            }
            Doc::Line => {
                if mode == Mode::Break {
                    return true;
                }
                rem -= 1;
                if rem < 0 {
                    return false;
                }
            }
            Doc::SoftLine => {
                if mode == Mode::Break {
                    return true;
                }
            }
            Doc::HardLine => return mode == Mode::Break,
            Doc::BreakParent => {
                if mode == Mode::Flat {
                    return false;
                }
            }
            Doc::Concat(v) => stack.extend(v.iter().rev().map(|d| (ind, mode, d))),
            Doc::Nest(n, d) => stack.push((ind + n, mode, d)),
            Doc::Group(d) => stack.push((ind, mode, d)),
            Doc::IfBreak(b, f) => stack.push((ind, mode, if mode == Mode::Break { b } else { f })),
        }
    }
}

/// Render `doc` with the given line width and indentation unit already baked in by `Nest`.
pub fn print(doc: &Doc, width: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    // Indentation is written lazily so blank lines never carry trailing spaces.
    let mut pending_indent: Option<usize> = None;
    let mut stack: Vec<Cmd> = vec![(0, Mode::Break, doc)];
    while let Some((ind, mode, d)) = stack.pop() {
        match d {
            Doc::Nil | Doc::BreakParent => {}
            Doc::Text(s) => {
                if s.is_empty() {
                    continue;
                }
                if let Some(i) = pending_indent.take() {
                    out.extend(std::iter::repeat_n(' ', i));
                    col = i;
                }
                out.push_str(s);
                col = if s.contains('\n') {
                    width_of(s)
                } else {
                    col + s.chars().count()
                };
            }
            Doc::Line | Doc::SoftLine | Doc::HardLine => {
                let newline = match d {
                    Doc::HardLine => true,
                    _ => mode == Mode::Break,
                };
                if newline {
                    // Trim trailing spaces from the line we are finishing.
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push('\n');
                    pending_indent = Some(ind);
                    col = ind;
                } else if matches!(d, Doc::Line) {
                    if let Some(i) = pending_indent.take() {
                        out.extend(std::iter::repeat_n(' ', i));
                    }
                    out.push(' ');
                    col += 1;
                }
            }
            Doc::Concat(v) => stack.extend(v.iter().rev().map(|d| (ind, mode, d))),
            Doc::Nest(n, d) => stack.push((ind + n, mode, d)),
            Doc::Group(inner) => {
                let rem = width as isize - col as isize;
                let m = if mode == Mode::Flat || fits(rem, (ind, Mode::Flat, inner), &stack) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                stack.push((ind, m, inner));
            }
            Doc::IfBreak(b, f) => stack.push((ind, mode, if mode == Mode::Break { b } else { f })),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: &[&str]) -> Doc {
        let docs: Vec<Doc> = args.iter().map(|a| text(*a)).collect();
        group(concat(vec![
            text("f("),
            nest(
                4,
                concat(vec![
                    Doc::SoftLine,
                    join(&concat(vec![text(","), Doc::Line]), docs),
                ]),
            ),
            if_break(text(","), Doc::Nil),
            Doc::SoftLine,
            text(")"),
        ]))
    }

    #[test]
    fn fits_flat() {
        assert_eq!(print(&call(&["a", "b"]), 20), "f(a, b)");
    }

    #[test]
    fn breaks_when_too_wide() {
        assert_eq!(
            print(&call(&["aaaa", "bbbb"]), 8),
            "f(\n    aaaa,\n    bbbb,\n)"
        );
    }

    #[test]
    fn hard_line_forces_break() {
        let d = group(concat(vec![
            text("a"),
            Doc::Line,
            text("b"),
            Doc::BreakParent,
        ]));
        assert_eq!(print(&d, 80), "a\nb");
    }
}
