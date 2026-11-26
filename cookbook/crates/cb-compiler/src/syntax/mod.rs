//! Front end: tokens, lexer, AST and the recoverable parser.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod token;

pub use lexer::lex;
pub use parser::{ParseResult, parse};
pub use token::{Comment, CommentKind, Token, TokenKind};
