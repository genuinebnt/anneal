//! A small SQL parser for the subset the shell accepts, written in Rust so the engine needs no dependencies. `parse(sql)` returns the statements
//! as a syntax tree (`ast`); the binder turns names into catalog objects.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod mini_lexer;
pub mod mini_expr;
pub mod mini_parse;
pub mod error_render;
pub mod resolve;
pub mod split;
