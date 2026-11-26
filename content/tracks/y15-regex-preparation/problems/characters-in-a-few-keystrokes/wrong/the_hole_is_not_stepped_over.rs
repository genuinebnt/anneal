#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    UnknownEscape(usize),
    TrailingEscape(usize),
    Reversed(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharSet {
    ranges: Vec<(char, char)>,
}

/// The `char` after `c`, stepping over the surrogate hole.
fn next(c: char) -> Option<char> {
    match c {
        _ => char::from_u32(c as u32 + 1),
    }
}

fn previous(c: char) -> Option<char> {
    match c {
        _ => (c as u32).checked_sub(1).and_then(char::from_u32),
    }
}

/// One character or escape starting at `*pos`.
fn item(chars: &[char], pos: &mut usize) -> Result<char, SetError> {
    let at = *pos;
    let c = chars[at];
    *pos += 1;
    if c != '\\' {
        return Ok(c);
    }
    let escaped = *chars.get(*pos).ok_or(SetError::TrailingEscape(at))?;
    *pos += 1;
    match escaped {
        '\\' | '-' | '^' => Ok(escaped),
        'n' => Ok('\n'),
        't' => Ok('\t'),
        'r' => Ok('\r'),
        _ => Err(SetError::UnknownEscape(at)),
    }
}

fn join(mut pairs: Vec<(char, char)>) -> Vec<(char, char)> {
    pairs.sort_unstable();
    let mut out: Vec<(char, char)> = Vec::new();
    for (low, high) in pairs {
        match out.last_mut() {
            Some(last) if next(last.1).is_none_or(|after| low <= after) => {
                last.1 = last.1.max(high);
            }
            _ => out.push((low, high)),
        }
    }
    out
}

fn complement(ranges: &[(char, char)]) -> Vec<(char, char)> {
    let mut out = Vec::new();
    let mut from = Some('\0');
    for &(low, high) in ranges {
        if let (Some(start), Some(end)) = (from, previous(low)) {
            if start <= end {
                out.push((start, end));
            }
        }
        from = next(high);
    }
    if let Some(start) = from {
        out.push((start, char::MAX));
    }
    out
}

impl CharSet {
    pub fn parse(spec: &str) -> Result<CharSet, SetError> {
        let chars: Vec<char> = spec.chars().collect();
        let negated = chars.first() == Some(&'^');
        let mut pos = usize::from(negated);
        let mut pairs = Vec::new();
        while pos < chars.len() {
            let at = pos;
            let low = item(&chars, &mut pos)?;
            let mut high = low;
            if chars.get(pos) == Some(&'-') && pos + 1 < chars.len() {
                pos += 1;
                high = item(&chars, &mut pos)?;
                if high < low {
                    return Err(SetError::Reversed(at));
                }
            }
            pairs.push((low, high));
        }
        let mut ranges = join(pairs);
        if negated {
            ranges = complement(&ranges);
        }
        Ok(CharSet { ranges })
    }

    /// Sorted pairs `(low, high)`, inclusive, which neither overlap nor touch.
    pub fn ranges(&self) -> &[(char, char)] {
        &self.ranges
    }

    pub fn contains(&self, c: char) -> bool {
        let after = self.ranges.partition_point(|&(low, _)| low <= c);
        after > 0 && c <= self.ranges[after - 1].1
    }
}
