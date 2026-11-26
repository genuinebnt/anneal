//! Fuzzy matching for `workspace/symbol`.
//!
//! A query matches a symbol name when its characters occur in the name in order,
//! ignoring case (a plain subsequence). Matches are ranked in tiers; within a tier
//! shorter names, then tighter matches, win:
//!
//! | tier | rule | example for `parse_item` |
//! |------|------|--------------------------|
//! | 6 | identical | `parse_item` |
//! | 5 | equal ignoring case | `Parse_Item` |
//! | 4 | prefix, case-sensitive | `parse` |
//! | 3 | prefix, ignoring case | `PARSE` |
//! | 2 | "hump" match: each query char is a word start (after `_`, a digit or a lower-to-upper step) or directly follows the previous matched char | `pi`, `paIt` |
//! | 1 | any subsequence ignoring case | `psit` |
//! | 0 | the empty query (everything matches) | |
//!
//! The ranking is a total preorder ([`Score`] is `Ord`), so sorting by score is stable
//! and deterministic; `tests/lsp_workspace.rs` checks soundness, completeness and
//! ordering against brute-force oracles.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score {
    pub tier: u8,
    /// Tie-breaker inside a tier: higher is better.
    pub bonus: i32,
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Positions where a new "word" starts in an identifier.
pub fn word_starts(name: &[char]) -> Vec<bool> {
    (0..name.len())
        .map(|i| {
            if i == 0 {
                return true;
            }
            let (p, c) = (name[i - 1], name[i]);
            (p == '_' && c != '_')
                || (!p.is_alphanumeric() && c.is_alphanumeric())
                || (c.is_uppercase() && (p.is_lowercase() || p.is_ascii_digit()))
                || (c.is_ascii_digit() && !p.is_ascii_digit())
        })
        .collect()
}

/// Hump match: every query char matches at a word start or right after the previous match.
fn hump(q: &[char], n: &[char], starts: &[bool]) -> bool {
    fn go(
        q: &[char],
        n: &[char],
        starts: &[bool],
        qi: usize,
        ni: usize,
        adjacent: bool,
        memo: &mut std::collections::HashSet<(usize, usize, bool)>,
    ) -> bool {
        if qi == q.len() {
            return true;
        }
        if ni >= n.len() || n.len() - ni < q.len() - qi || !memo.insert((qi, ni, adjacent)) {
            return false;
        }
        // Match q[qi] at n[ni] (allowed at a word start, or when it continues the previous match).
        if lower(n[ni]) == lower(q[qi])
            && (starts[ni] || adjacent)
            && go(q, n, starts, qi + 1, ni + 1, true, memo)
        {
            return true;
        }
        go(q, n, starts, qi, ni + 1, false, memo)
    }
    go(q, n, starts, 0, 0, false, &mut Default::default())
}

/// Greedy leftmost case-insensitive subsequence: matched positions, or `None`.
fn subsequence(q: &[char], n: &[char]) -> Option<Vec<usize>> {
    let mut at = Vec::with_capacity(q.len());
    let mut ni = 0;
    for &qc in q {
        while ni < n.len() && lower(n[ni]) != lower(qc) {
            ni += 1;
        }
        if ni == n.len() {
            return None;
        }
        at.push(ni);
        ni += 1;
    }
    Some(at)
}

/// Score `name` against `query`; `None` when it does not match.
pub fn score(query: &str, name: &str) -> Option<Score> {
    let q: Vec<char> = query.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let short = -(n.len() as i32);
    if q.is_empty() {
        return Some(Score {
            tier: 0,
            bonus: short,
        });
    }
    if query == name {
        return Some(Score { tier: 6, bonus: 0 });
    }
    let eq_ci = q.len() == n.len() && q.iter().zip(&n).all(|(a, b)| lower(*a) == lower(*b));
    if eq_ci {
        return Some(Score { tier: 5, bonus: 0 });
    }
    if name.starts_with(query) {
        return Some(Score {
            tier: 4,
            bonus: short,
        });
    }
    if q.len() <= n.len() && q.iter().zip(&n).all(|(a, b)| lower(*a) == lower(*b)) {
        return Some(Score {
            tier: 3,
            bonus: short,
        });
    }
    let at = subsequence(&q, &n)?;
    // Tightness: adjacent matched pairs count double, a late first match costs.
    let adjacent = at.windows(2).filter(|w| w[1] == w[0] + 1).count() as i32;
    let bonus = short + 2 * adjacent - at[0] as i32;
    let tier = if hump(&q, &n, &word_starts(&n)) { 2 } else { 1 };
    Some(Score { tier, bonus })
}

/// Sort `(score, key)` pairs best first: higher score, then the key (callers put the name first).
pub fn rank<T: Ord>(items: &mut [(Score, T)]) {
    items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers() {
        let t = |q, n| score(q, n).map(|s| s.tier);
        assert_eq!(t("parse_item", "parse_item"), Some(6));
        assert_eq!(t("parse_item", "Parse_Item"), Some(5));
        assert_eq!(t("parse", "parse_item"), Some(4));
        assert_eq!(t("PARSE", "parse_item"), Some(3));
        assert_eq!(t("pi", "parse_item"), Some(2));
        assert_eq!(t("pai", "parse_item"), Some(2));
        assert_eq!(t("psit", "parse_item"), Some(1));
        assert_eq!(t("zz", "parse_item"), None);
        assert_eq!(t("gH", "getHttpResponse"), Some(2));
        assert_eq!(t("", "x"), Some(0));
    }
}
