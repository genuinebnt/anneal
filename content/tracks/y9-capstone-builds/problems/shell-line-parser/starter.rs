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

/// Parses one command line into a pipeline. `env` supplies `$VAR` values.
pub fn parse(line: &str, env: &HashMap<String, String>) -> Result<Vec<Command>, ParseError> {
    todo!()
}
