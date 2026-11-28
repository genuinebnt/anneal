/// One token of the pattern at `p[i..]`: whether it accepts `c`, and where the next token starts.
fn token(p: &[u8], mut i: usize, c: u8) -> (bool, usize) {
    match p[i] {
        b'?' => (true, i + 1),
        b'\\' if i + 1 < p.len() => (p[i + 1] == c, i + 2),
        b'[' => {
            i += 1;
            let negated = matches!(p.get(i), Some(&b'^') | Some(&b'!'));
            i += usize::from(negated);
            let mut found = false;
            while i < p.len() && p[i] != b']' {
                let escaped = p[i] == b'\\' && i + 1 < p.len();
                i += usize::from(escaped);
                let low = p[i];
                if p.get(i + 1) == Some(&b'-') && i + 2 < p.len() && p[i + 2] != b']' {
                    let high = p[i + 2];
                    found |= low.min(high) <= c && c <= low.max(high);
                    i += 3;
                } else {
                    found |= low == c;
                    i += 1;
                }
            }
            (found != negated, (i + 1).min(p.len()))
        }
        literal => (literal == c, i + 1),
    }
}

/// Whether the whole of `name` fits `pattern`.
pub fn matches(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    // where to resume if the latest `*` has to take one byte more
    let mut star: Option<(usize, usize)> = None;
    while t < name.len() {
        if pattern.get(p) == Some(&b'*') {
            p += 1;
            star = Some((p, t));
            continue;
        }
        if p < pattern.len() {
            let (accepted, next) = token(pattern, p, name[t]);
            if accepted {
                p = next;
                t += 1;
                continue;
            }
        }
        let Some((after, taken)) = star else {
            return false;
        };
        p = after;
        t = taken + 1;
        star = Some((after, taken + 1));
    }
    pattern[p..].iter().all(|&b| b == b'*')
}
