//! Doc-comment helpers shared by hover, completion resolve and semantic highlighting.

use crate::analysis::syntax::{Comment, CommentKind};

/// The `///` lines directly above `item_start` (only whitespace, at most one newline per
/// gap, between them), joined with newlines and stripped of the `/// ` prefix.
pub fn doc_comment(src: &str, comments: &[Comment], item_start: usize) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    let mut at = item_start;
    for c in comments
        .iter()
        .rev()
        .filter(|c| c.span.end as usize <= item_start)
    {
        if c.kind != CommentKind::DocLine {
            break;
        }
        let gap = src.get(c.span.end as usize..at)?;
        if !gap.trim().is_empty() || gap.matches('\n').count() > 1 {
            break;
        }
        lines.push(
            c.text
                .trim_start_matches('/')
                .strip_prefix(' ')
                .unwrap_or(c.text.trim_start_matches('/'))
                .trim_end(),
        );
        at = c.span.start as usize;
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join("\n"))
}

/// Does the doc comment mark the item as deprecated (`/// Deprecated: ...` or `@deprecated`)?
pub fn is_deprecated(doc: &str) -> bool {
    let l = doc.to_ascii_lowercase();
    l.starts_with("deprecated") || l.contains("@deprecated")
}
