#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A use of a name.
    Name(String),
    /// A function of one parameter.
    Fn(String, Box<Term>),
    /// A function applied to an argument.
    Apply(Box<Term>, Box<Term>),
    /// `let name = value in body`.
    Let(String, Box<Term>, Box<Term>),
}

/// Writes the term so that the names of its own variables no longer show.
fn write<'a>(term: &'a Term, around: &mut Vec<&'a str>, out: &mut String) {
    match term {
        Term::Name(name) => match around.iter().rev().position(|n| n == name) {
            Some(distance) => {
                out.push('#');
                out.push_str(&distance.to_string());
            }
            None => out.push_str(name),
        },
        Term::Fn(parameter, body) => {
            out.push_str("(fn ");
            around.push(parameter);
            write(body, around, out);
            around.pop();
            out.push(')');
        }
        Term::Apply(function, argument) => {
            out.push('(');
            write(function, around, out);
            out.push(' ');
            write(argument, around, out);
            out.push(')');
        }
        Term::Let(name, value, body) => {
            out.push_str("(let ");
            around.push(name);
            write(value, around, out);
            out.push(' ');
            write(body, around, out);
            around.pop();
            out.push(')');
        }
    }
}

/// The nameless form of a term.
pub fn nameless(term: &Term) -> String {
    let mut out = String::new();
    write(term, &mut Vec::new(), &mut out);
    out
}

/// The names the term uses but does not introduce: sorted, each once.
pub fn outside_names(term: &Term) -> Vec<String> {
    fn walk<'a>(term: &'a Term, around: &mut Vec<&'a str>, found: &mut Vec<String>) {
        match term {
            Term::Name(name) => {
                if !around.contains(&name.as_str()) {
                    found.push(name.clone());
                }
            }
            Term::Fn(parameter, body) => {
                around.push(parameter);
                walk(body, around, found);
                around.pop();
            }
            Term::Apply(function, argument) => {
                walk(function, around, found);
                walk(argument, around, found);
            }
            Term::Let(name, value, body) => {
                walk(value, around, found);
                around.push(name);
                walk(body, around, found);
                around.pop();
            }
        }
    }
    let mut found = Vec::new();
    walk(term, &mut Vec::new(), &mut found);
    found.sort();
    found.dedup();
    found
}

/// True when the two differ at most in the names they choose for their own variables.
pub fn same_meaning(a: &Term, b: &Term) -> bool {
    nameless(a) == nameless(b)
}
