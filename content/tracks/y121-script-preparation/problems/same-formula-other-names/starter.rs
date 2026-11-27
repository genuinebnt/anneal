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

/// The nameless form of a term.
pub fn nameless(term: &Term) -> String {
    todo!()
}

/// The names the term uses but does not introduce: sorted, each once.
pub fn outside_names(term: &Term) -> Vec<String> {
    todo!()
}

/// True when the two differ at most in the names they choose for their own variables.
pub fn same_meaning(a: &Term, b: &Term) -> bool {
    todo!()
}
