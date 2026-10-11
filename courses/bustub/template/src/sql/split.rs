//! Splitting a SQL script into statements.

/// The statements of a script, split at each `;` that is not inside a string, a quoted identifier or a comment.
pub fn split_statements(text: &str) -> Vec<String> {
    todo!("4e-c2: a small state machine over the characters: code, string, quoted identifier, line comment, block comment")
}
