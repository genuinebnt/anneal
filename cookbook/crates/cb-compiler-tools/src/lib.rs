//! Tooling for the PEBBLE language: a comment-preserving formatter, a lexical and
//! semantic syntax highlighter, generated editor grammars and a language server
//! (JSON-RPC over stdio, std + `serde_json` only).

pub mod analysis;
pub mod docs;
pub mod eol;
pub mod fmt;
pub mod highlight;
pub mod lsp;
pub mod textmate;
