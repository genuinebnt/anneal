//! The TextMate grammar and VS Code language configuration, generated from the
//! compiler's own tables so editor highlighting cannot drift from the lexer.
//!
//! The keyword list comes from [`keywords`], which is checked against
//! `crate::analysis::syntax::token::keyword` (the lexer's table) in the tests. The match in
//! [`kind_guard`] has no wildcard arm: adding a `TokenKind` variant to the compiler
//! stops this crate compiling until the new kind is classified here.

use crate::analysis::sema::Builtin;
use crate::analysis::syntax::TokenKind;
use serde_json::{Value, json};

/// Every fixed-spelling token kind (keywords and punctuation).
pub fn all_kinds() -> Vec<TokenKind> {
    use TokenKind::*;
    vec![
        Fn, Struct, Enum, Const, Let, Mut, If, Else, Match, While, Loop, Break, Continue, Return,
        As, True, False, LParen, RParen, LBrace, RBrace, LBracket, RBracket, Comma, Semi, Colon,
        ColonColon, Dot, DotDot, Arrow, FatArrow, Eq, EqEq, NotEq, Lt, Le, Gt, Ge, Plus, Minus,
        Star, Slash, Percent, PlusEq, MinusEq, StarEq, SlashEq, PercentEq, AndAnd, OrOr, Bang,
        Pipe, Underscore,
    ]
}

/// Exhaustive on purpose (see the module docs). Returns true for kinds with a fixed spelling.
pub fn kind_guard(k: &TokenKind) -> bool {
    use TokenKind::*;
    match k {
        Ident | Int(_) | Str(_) | Error | Eof => false,
        Fn | Struct | Enum | Const | Let | Mut | If | Else | Match | While | Loop | Break
        | Continue | Return | As | True | False | LParen | RParen | LBrace | RBrace | LBracket
        | RBracket | Comma | Semi | Colon | ColonColon | Dot | DotDot | Arrow | FatArrow | Eq
        | EqEq | NotEq | Lt | Le | Gt | Ge | Plus | Minus | Star | Slash | Percent | PlusEq
        | MinusEq | StarEq | SlashEq | PercentEq | AndAnd | OrOr | Bang | Pipe | Underscore => true,
    }
}

/// The lexer's keywords, in declaration order.
pub fn keywords() -> Vec<&'static str> {
    all_kinds()
        .iter()
        .filter(|k| k.is_keyword())
        .filter_map(|k| k.symbol())
        .collect()
}

const CONTROL: [&str; 8] = [
    "if", "else", "match", "while", "loop", "break", "continue", "return",
];

fn alt(words: &[&str]) -> String {
    format!("\\b({})\\b", words.join("|"))
}

pub fn grammar() -> Value {
    let kws = keywords();
    let control: Vec<&str> = kws
        .iter()
        .copied()
        .filter(|k| CONTROL.contains(k))
        .collect();
    let consts: Vec<&str> = vec!["true", "false"];
    let storage: Vec<&str> = kws
        .iter()
        .copied()
        .filter(|k| ["fn", "struct", "enum", "const", "let", "mut"].contains(k))
        .collect();
    let other: Vec<&str> = kws
        .iter()
        .copied()
        .filter(|k| !control.contains(k) && !consts.contains(k) && !storage.contains(k))
        .collect();
    let builtins: Vec<&str> = Builtin::ALL.iter().map(|b| b.name()).collect();
    json!({
        "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
        "name": "PEBBLE",
        "scopeName": "source.pebble",
        "fileTypes": ["peb"],
        "patterns": [
            {"include": "#comments"}, {"include": "#strings"}, {"include": "#keywords"}, {"include": "#numbers"},
            {"include": "#builtin-types"}, {"include": "#builtins"}, {"include": "#functions"}, {"include": "#variants"},
            {"include": "#types"}, {"include": "#operators"}
        ],
        "repository": {
            "comments": {"patterns": [
                {"name": "comment.line.documentation.pebble", "match": "///.*$"},
                {"name": "comment.line.double-slash.pebble", "match": "//.*$"},
                {"name": "comment.block.pebble", "begin": "/\\*", "end": "\\*/", "patterns": [{"include": "#block-comment-nested"}]}
            ]},
            "block-comment-nested": {"name": "comment.block.pebble", "begin": "/\\*", "end": "\\*/", "patterns": [{"include": "#block-comment-nested"}]},
            "strings": {"name": "string.quoted.double.pebble", "begin": "\"", "end": "\"|$", "patterns": [
                {"name": "constant.character.escape.pebble", "match": "\\\\(?:[ntr0\\\\\"]|u\\{[0-9a-fA-F]+\\})"},
                {"name": "invalid.illegal.escape.pebble", "match": "\\\\."}
            ]},
            "keywords": {"patterns": [
                {"name": "keyword.control.pebble", "match": alt(&control)},
                {"name": "storage.type.pebble", "match": alt(&storage)},
                {"name": "keyword.other.pebble", "match": alt(&other)},
                {"name": "constant.language.pebble", "match": alt(&consts)}
            ]},
            "numbers": {"name": "constant.numeric.integer.pebble", "match": "\\b[0-9][0-9_]*\\b"},
            "builtin-types": {"name": "support.type.primitive.pebble", "match": "\\b(int|bool|str)\\b"},
            "builtins": {"name": "support.function.builtin.pebble", "match": alt(&builtins)},
            "functions": {"match": "\\b([A-Za-z_][A-Za-z0-9_]*)\\s*(?=\\()", "captures": {"1": {"name": "entity.name.function.pebble"}}},
            "variants": {"match": "\\b([A-Z][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)", "captures": {"1": {"name": "entity.name.type.enum.pebble"}, "2": {"name": "constant.other.enum-member.pebble"}}},
            "types": {"name": "entity.name.type.pebble", "match": "\\b[A-Z][A-Za-z0-9_]*\\b"},
            "operators": {"name": "keyword.operator.pebble", "match": "->|=>|==|!=|<=|>=|&&|\\|\\||[-+*/%]=?|=|<|>|!|\\||\\.\\.|::"}
        }
    })
}

pub fn language_configuration() -> Value {
    json!({
        "comments": {"lineComment": "//", "blockComment": ["/*", "*/"]},
        "brackets": [["{", "}"], ["[", "]"], ["(", ")"]],
        "autoClosingPairs": [
            {"open": "{", "close": "}"}, {"open": "[", "close": "]"}, {"open": "(", "close": ")"},
            {"open": "\"", "close": "\"", "notIn": ["string", "comment"]}, {"open": "/*", "close": " */", "notIn": ["string"]}
        ],
        "surroundingPairs": [["{", "}"], ["[", "]"], ["(", ")"], ["\"", "\""]],
        "indentationRules": {
            "increaseIndentPattern": "^.*\\{[^}\"']*$|^.*\\([^)\"']*$|^.*\\[[^\\]\"']*$",
            "decreaseIndentPattern": "^\\s*[}\\])]"
        },
        "onEnterRules": [
            {"beforeText": "^\\s*///.*$", "action": {"indent": "none", "appendText": "/// "}}
        ],
        "wordPattern": "[A-Za-z_][A-Za-z0-9_]*"
    })
}

pub fn vscode_package() -> Value {
    json!({
        "name": "pebble-language",
        "displayName": "PEBBLE",
        "description": "Syntax highlighting and language server client for the PEBBLE language",
        "version": "0.1.0",
        "publisher": "cookbook",
        "engines": {"vscode": "^1.80.0"},
        "categories": ["Programming Languages"],
        "main": "./extension.js",
        "activationEvents": ["onLanguage:pebble"],
        "contributes": {
            "configurationDefaults": {"[pebble]": {"editor.tabSize": 4, "editor.insertSpaces": true, "editor.semanticHighlighting.enabled": true, "editor.formatOnType": true}},
            "languages": [{"id": "pebble", "aliases": ["PEBBLE", "pebble"], "extensions": [".peb"], "configuration": "./language-configuration.json"}],
            "grammars": [{"language": "pebble", "scopeName": "source.pebble", "path": "./syntaxes/pebble.tmLanguage.json"}],
            "configuration": {
                "title": "PEBBLE",
                "properties": {
                    "pebble.server.path": {"type": "string", "default": "pebble-tools", "description": "Path to the pebble-tools executable (started as `pebble-tools lsp`)."},
                    "pebble.trace.server": {"type": "string", "enum": ["off", "messages", "verbose"], "default": "off", "description": "Trace LSP traffic."}
                }
            }
        },
        "dependencies": {"vscode-languageclient": "^9.0.1"}
    })
}

pub const VSCODE_EXTENSION_JS: &str = r#"// Thin LSP client: starts `pebble-tools lsp` over stdio. Install dependencies with `npm install`.
const { workspace } = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function activate() {
  const command = workspace.getConfiguration('pebble').get('server.path', 'pebble-tools');
  const serverOptions = { command, args: ['lsp'], transport: TransportKind.stdio };
  const clientOptions = { documentSelector: [{ scheme: 'file', language: 'pebble' }] };
  client = new LanguageClient('pebble', 'PEBBLE Language Server', serverOptions, clientOptions);
  client.start();
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
"#;

/// Every generated editor file as (relative path under the crate, contents).
pub fn editor_files() -> Vec<(&'static str, String)> {
    let pretty = |v: Value| serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
    vec![
        ("editors/pebble.tmLanguage.json", pretty(grammar())),
        (
            "editors/vscode/syntaxes/pebble.tmLanguage.json",
            pretty(grammar()),
        ),
        (
            "editors/vscode/language-configuration.json",
            pretty(language_configuration()),
        ),
        ("editors/vscode/package.json", pretty(vscode_package())),
        (
            "editors/vscode/extension.js",
            VSCODE_EXTENSION_JS.to_string(),
        ),
    ]
}
