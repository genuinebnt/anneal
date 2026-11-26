//! Static checks of the VS Code extension, plus (when `node` is on PATH) a syntax
//! check and a run of `extension.js` against a mock of the VS Code API. None of this
//! runs VS Code itself.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn vscode_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("editors/vscode")
}

fn json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn text<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("package.json: missing string {pointer}"))
}

fn existing(relative: &str) -> PathBuf {
    let p = vscode_dir().join(relative);
    assert!(
        p.is_file(),
        "referenced file does not exist: {}",
        p.display()
    );
    p
}

#[test]
fn package_json_has_the_fields_vs_code_requires_and_every_referenced_file_exists() {
    let pkg = json(&vscode_dir().join("package.json"));
    let name = text(&pkg, "/name");
    assert!(
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "extension names are lowercase: {name}"
    );
    assert!(!text(&pkg, "/publisher").is_empty());
    assert_eq!(
        text(&pkg, "/version")
            .split('.')
            .filter(|p| p.parse::<u32>().is_ok())
            .count(),
        3,
        "version must be x.y.z"
    );
    assert!(
        text(&pkg, "/engines/vscode")
            .trim_start_matches(['^', '>', '='])
            .starts_with("1.")
    );
    existing(text(&pkg, "/main"));
    assert_eq!(
        pkg["activationEvents"],
        serde_json::json!(["onLanguage:pebble"])
    );

    let language = &pkg["contributes"]["languages"][0];
    assert_eq!(text(language, "/id"), "pebble");
    assert_eq!(language["extensions"], serde_json::json!([".peb"]));
    json(&existing(text(language, "/configuration")));

    let grammar = &pkg["contributes"]["grammars"][0];
    assert_eq!(text(grammar, "/language"), "pebble");
    let tm = json(&existing(text(grammar, "/path")));
    assert_eq!(text(&tm, "/scopeName"), text(grammar, "/scopeName"));

    let defaults = &pkg["contributes"]["configurationDefaults"]["[pebble]"];
    assert_eq!(defaults["editor.tabSize"], 4);
    assert_eq!(defaults["editor.insertSpaces"], true);
    // Every setting the extension reads is declared.
    assert!(pkg["contributes"]["configuration"]["properties"]["pebble.server.path"].is_object());
    // The runtime dependency is declared (it is not installed by these tests).
    assert!(pkg["dependencies"]["vscode-languageclient"].is_string());
}

/// The conventional TextMate scope prefixes (TextMate manual, "Naming Conventions"),
/// which is what colour themes match against.
const STANDARD_SCOPES: &[&str] = &[
    "comment.line",
    "comment.block",
    "constant.numeric",
    "constant.character",
    "constant.language",
    "constant.other",
    "entity.name",
    "entity.other",
    "invalid.illegal",
    "invalid.deprecated",
    "keyword.control",
    "keyword.operator",
    "keyword.other",
    "markup",
    "meta",
    "punctuation",
    "storage.type",
    "storage.modifier",
    "string.quoted",
    "string.unquoted",
    "string.interpolated",
    "string.regexp",
    "string.other",
    "support.function",
    "support.class",
    "support.type",
    "support.constant",
    "support.variable",
    "variable.parameter",
    "variable.language",
    "variable.other",
];

fn scope_names(v: &Value, in_pattern: bool, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, child) in m {
                // The top-level "name" is the grammar's display name, not a scope.
                if (k == "name" || k == "contentName") && in_pattern {
                    out.push(child.as_str().expect("scope name is a string").to_string());
                } else {
                    scope_names(
                        child,
                        in_pattern || k == "patterns" || k == "repository",
                        out,
                    );
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| scope_names(x, in_pattern, out)),
        _ => {}
    }
}

#[test]
fn every_grammar_scope_is_a_standard_textmate_scope() {
    let mut scopes = Vec::new();
    scope_names(
        &json(&vscode_dir().join("syntaxes/pebble.tmLanguage.json")),
        false,
        &mut scopes,
    );
    assert!(scopes.len() >= 15, "found only {scopes:?}");
    for s in scopes {
        assert!(
            s.ends_with(".pebble"),
            "{s}: scopes end with the language suffix"
        );
        assert!(
            STANDARD_SCOPES
                .iter()
                .any(|p| s.strip_prefix(p).is_some_and(|rest| rest.starts_with('.'))),
            "{s} is not under a standard TextMate scope"
        );
    }
}

fn node() -> Option<Command> {
    let found = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !found {
        eprintln!("SKIPPED: `node` is not on PATH");
    }
    found.then(|| Command::new("node"))
}

#[test]
fn extension_js_parses_and_starts_the_client_against_a_mock_of_the_vs_code_api() {
    let extension = existing("extension.js");
    let Some(mut check) = node() else { return };
    let out = check.arg("--check").arg(&extension).output().unwrap();
    assert!(
        out.status.success(),
        "node --check: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let Some(mut mock) = node() else { return };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vscode/mock.js");
    let out = mock
        .arg(script)
        .arg(extension.canonicalize().unwrap())
        .arg(env!("CARGO_BIN_EXE_pebble-tools"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("mock client ok"),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
