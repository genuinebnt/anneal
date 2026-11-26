//! Checks a fix-this attempt against the problem's [`Rules`] before it counts.
//!
//! The checks are syntactic: they read the code with `syn`, not the compiler.
//! `.clone()` is caught whether written as a method call, `Clone::clone(&x)` or
//! `T::clone(&x)`; a forbidden type is caught anywhere its name appears in a path.
//! Code that doesn't parse skips the AST checks, since the compiler will report it.

use std::io::Write;
use std::process::{Command, Stdio};

use anneal_content::Rules;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// Short id shown in the run timeline as `rule: <rule>`, e.g. `clone`, `RefCell`, `unsafe`, `lines`.
    pub rule: String,
    pub message: String,
    /// 1-based line in the user's code, when there is one.
    pub line: Option<u32>,
}

/// Every rule `code` breaks. `starter` is the broken program the attempt started from.
pub fn check(code: &str, starter: &str, rules: &Rules) -> Vec<Violation> {
    let mut out = Vec::new();
    if let Ok(file) = syn::parse_file(code) {
        let mut v = Finder {
            rules,
            found: Vec::new(),
        };
        v.visit_file(&file);
        // One report per rule and line is enough.
        v.found
            .dedup_by(|a, b| a.rule == b.rule && a.line == b.line);
        out.extend(v.found);
    }
    if let Some(max) = rules.max_changed_lines {
        let changed = counted_lines(starter, code, max as usize);
        if changed > max as usize {
            out.push(Violation {
                rule: "lines".into(),
                message: format!("{changed} lines changed; the limit is {max}"),
                line: None,
            });
        }
    }
    out
}

struct Finder<'r> {
    rules: &'r Rules,
    found: Vec<Violation>,
}

fn line_of(s: proc_macro2::Span) -> Option<u32> {
    Some(s.start().line as u32).filter(|&l| l > 0)
}

impl Finder<'_> {
    fn method(&mut self, name: &syn::Ident) {
        let n = name.to_string();
        if self.rules.forbid_methods.contains(&n) {
            self.found.push(Violation {
                rule: n.clone(),
                message: format!("calls .{n}(), which this problem forbids"),
                line: line_of(name.span()),
            });
        }
    }

    fn use_ident(&mut self, ident: &syn::Ident) {
        let n = ident.to_string();
        if self.rules.forbid_types.contains(&n) {
            self.found.push(Violation {
                rule: n.clone(),
                message: format!("imports {n}, which this problem forbids"),
                line: line_of(ident.span()),
            });
        }
    }
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        self.method(&e.method);
        visit::visit_expr_method_call(self, e);
    }

    /// `Clone::clone(&x)` and `Vec::clone(&x)` call the method by path. `Rc::clone`,
    /// `Arc::clone` and `Weak::clone` are exempt: they copy a pointer and bump a count,
    /// which is how you avoid copying the data.
    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*e.func
            && p.path.segments.len() > 1
            && let Some(last) = p.path.segments.last()
        {
            let owner = &p.path.segments[p.path.segments.len() - 2].ident;
            let pointer_clone = last.ident == "clone" && ["Rc", "Arc", "Weak"].iter().any(|t| owner == t);
            if !pointer_clone {
                self.method(&last.ident);
            }
        }
        visit::visit_expr_call(self, e);
    }

    fn visit_path_segment(&mut self, seg: &'ast syn::PathSegment) {
        let n = seg.ident.to_string();
        if self.rules.forbid_types.contains(&n) {
            self.found.push(Violation {
                rule: n.clone(),
                message: format!("uses {n}, which this problem forbids"),
                line: line_of(seg.ident.span()),
            });
        }
        visit::visit_path_segment(self, seg);
    }

    /// `use std::cell::RefCell;` names the type in a use tree, not a path.
    fn visit_use_name(&mut self, u: &'ast syn::UseName) {
        self.use_ident(&u.ident);
    }

    fn visit_use_rename(&mut self, u: &'ast syn::UseRename) {
        self.use_ident(&u.ident);
    }

    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        if self.rules.forbid_unsafe {
            self.found.push(Violation {
                rule: "unsafe".into(),
                message: "uses an unsafe block".into(),
                line: line_of(e.span()),
            });
        }
        visit::visit_expr_unsafe(self, e);
    }

    fn visit_signature(&mut self, s: &'ast syn::Signature) {
        if self.rules.forbid_unsafe && s.unsafety.is_some() {
            self.found.push(Violation {
                rule: "unsafe".into(),
                message: format!("declares unsafe fn {}", s.ident),
                line: line_of(s.ident.span()),
            });
        }
        visit::visit_signature(self, s);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if self.rules.forbid_unsafe && i.unsafety.is_some() {
            self.found.push(Violation {
                rule: "unsafe".into(),
                message: "has an unsafe impl".into(),
                line: line_of(i.impl_token.span),
            });
        }
        visit::visit_item_impl(self, i);
    }
}

/// The count the line limit judges. Formatting the editor does must not decide whether a fix passes, so when the plain
/// count is over `max`, both versions are laid out again with long lines allowed and counted that way; the lower count
/// wins. So nothing that passes the plain count can fail. Code rustfmt can't read keeps the plain count.
fn counted_lines(starter: &str, code: &str, max: usize) -> usize {
    let plain = changed_lines(starter, code);
    if plain <= max {
        return plain;
    }
    match (unwrapped(starter), unwrapped(code)) {
        (Some(a), Some(b)) => plain.min(changed_lines(&a, &b)),
        _ => plain,
    }
}

/// `source` as rustfmt writes it when it never has to wrap a line to fit. `None` if rustfmt is missing or rejects it.
fn unwrapped(source: &str) -> Option<String> {
    let binary = std::env::var("ANNEAL_RUSTFMT").unwrap_or_else(|_| "rustfmt".into());
    let mut child = Command::new(binary)
        .args(["--edition", "2021", "--emit", "stdout", "--quiet"])
        .args(["--config", "max_width=1000,use_small_heuristics=Max"])
        // away from any rustfmt.toml of the directory the server happens to run in
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // rustfmt reads all of its input before it writes; dropping the handle ends the input.
    child.stdin.take()?.write_all(source.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// Lines changed between two versions: a line edited in place counts once.
/// Whitespace at line ends and blank lines are ignored.
pub fn changed_lines(before: &str, after: &str) -> usize {
    let norm = |s: &str| {
        s.lines()
            .map(str::trim_end)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (a, b) = (norm(before), norm(after));
    let common = lcs_len(&a, &b);
    (a.len() - common).max(b.len() - common)
}

fn lcs_len(a: &[String], b: &[String]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    for x in a {
        let mut cur = vec![0usize; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                prev[j + 1].max(cur[j])
            };
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            forbid_methods: vec!["clone".into()],
            forbid_types: vec!["RefCell".into(), "Cell".into()],
            forbid_unsafe: true,
            max_changed_lines: Some(2),
        }
    }

    const STARTER: &str =
        "fn f(v: &mut Vec<u32>) {\n    let a = 1;\n    let b = 2;\n    v.push(a + b);\n}\n";

    #[test]
    fn clean_fix_passes() {
        let fixed = STARTER.replace("let b = 2;", "let b = 3;");
        assert_eq!(check(&fixed, STARTER, &rules()), vec![]);
    }

    #[test]
    fn finds_clone_in_every_spelling() {
        let code = "fn f(v: &Vec<u32>) {\n    let a = v.clone();\n    let b = Clone::clone(v);\n    let c = Vec::clone(v);\n}\n";
        let lines: Vec<_> = check(code, code, &rules())
            .into_iter()
            .map(|v| (v.rule, v.line))
            .collect();
        assert_eq!(
            lines,
            vec![
                ("clone".into(), Some(2)),
                ("clone".into(), Some(3)),
                ("clone".into(), Some(4))
            ]
        );
    }

    #[test]
    fn refcount_clones_are_allowed() {
        let code = "use std::sync::Arc;\nfn f(v: Arc<Vec<u32>>) {\n    let a = Arc::clone(&v);\n    let b = std::rc::Rc::clone(&std::rc::Rc::new(1));\n}\n";
        assert_eq!(check(code, code, &rules()), vec![]);
    }

    #[test]
    fn finds_forbidden_types_and_unsafe() {
        let code = "use std::cell::RefCell;\nfn f() { let c = std::cell::Cell::new(1); unsafe { } }\nunsafe fn g() {}\n";
        let found: Vec<_> = check(code, code, &rules())
            .into_iter()
            .map(|v| v.rule)
            .collect();
        assert_eq!(found, vec!["RefCell", "Cell", "unsafe", "unsafe"]);
    }

    #[test]
    fn counts_changed_lines() {
        assert_eq!(changed_lines(STARTER, STARTER), 0);
        assert_eq!(
            changed_lines(STARTER, &STARTER.replace("let b = 2;", "let b = 3;")),
            1
        );
        let three = STARTER.replace(
            "    let a = 1;\n",
            "    let a = 1;\n    let x = 0;\n    let y = 0;\n    let z = 0;\n",
        );
        assert_eq!(changed_lines(STARTER, &three), 3);
        let v = check(&three, STARTER, &rules());
        assert_eq!(
            v.last().map(|v| v.message.as_str()),
            Some("3 lines changed; the limit is 2")
        );
    }

    /// The case that was reported: a long call the editor wrapped must not use up the line limit.
    #[test]
    fn wrapping_a_long_line_does_not_count() {
        let starter = "fn f(a: u32) -> String {\n    let name = a;\n    format!(\"{name}: {} items ({} long), longest {}\", a + 1, a + 2, a + 3)\n}\n";
        let wrapped = "fn f(a: u32) -> String {\n    let name = a;\n    format!(\n        \"{name}: {} items ({} long), longest {}\",\n        a + 1,\n        a + 2,\n        a + 4\n    )\n}\n";
        assert!(changed_lines(starter, wrapped) > 2);
        assert_eq!(check(wrapped, starter, &rules()), vec![]);
    }

    #[test]
    fn real_changes_still_count_when_wrapped_or_not() {
        let many = STARTER.replace("let a = 1;", "let a = 10;\n    let c = 3;\n    let d = 4;");
        let found = check(&many, STARTER, &rules());
        assert_eq!(found.last().map(|v| v.rule.as_str()), Some("lines"));
    }

    #[test]
    fn unparseable_code_only_gets_the_line_check() {
        assert_eq!(check("fn broken( {", "fn broken( {", &rules()), vec![]);
    }
}
