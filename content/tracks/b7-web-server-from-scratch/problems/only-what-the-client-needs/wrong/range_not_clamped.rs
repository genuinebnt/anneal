#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    pub method: &'a str,
    pub if_none_match: Option<&'a str>,
    pub if_modified_since: Option<&'a str>,
    pub range: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct FileInfo<'a> {
    pub tag: &'a str,
    pub last_modified: &'a str,
    pub len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Whole,
    NotModified,
    Part { first: u64, last: u64 },
    Unsatisfiable,
}

fn is_current(ask: &Ask, file: &FileInfo) -> bool {
    match ask.if_none_match {
        Some(tags) => tags.split(',').map(str::trim).any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == file.tag),
        None => ask.if_modified_since == Some(file.last_modified),
    }
}

/// `None` unless `text` is one or more digits. Too large to hold is as large as it gets.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(text.parse().unwrap_or(u64::MAX))
}

/// `Whole` for a value that is not one of the three forms.
fn part(value: &str, len: u64) -> Plan {
    let Some((first, last)) = value.trim().strip_prefix("bytes=").and_then(|s| s.split_once('-')) else {
        return Plan::Whole;
    };
    // the last position in the file; an empty file has none
    let end = len.checked_sub(1);
    if first.is_empty() {
        return match (number(last), end) {
            (None, _) => Plan::Whole,
            (Some(0), _) | (_, None) => Plan::Unsatisfiable,
            (Some(n), Some(end)) => Plan::Part { first: len.saturating_sub(n), last: end },
        };
    }
    let Some(first) = number(first) else {
        return Plan::Whole;
    };
    let wanted_last = if last.is_empty() {
        u64::MAX
    } else {
        match number(last) {
            Some(last) if last >= first => last,
            _ => return Plan::Whole,
        }
    };
    match end {
        Some(end) if first <= end => Plan::Part { first, last: if wanted_last == u64::MAX { end } else { wanted_last } },
        _ => Plan::Unsatisfiable,
    }
}

pub fn plan(ask: &Ask, file: &FileInfo) -> Plan {
    if is_current(ask, file) {
        return Plan::NotModified;
    }
    match ask.range {
        Some(value) if ask.method == "GET" => part(value, file.len),
        _ => Plan::Whole,
    }
}

pub fn content_range(plan: Plan, len: u64) -> Option<String> {
    match plan {
        Plan::Part { first, last } => Some(format!("bytes {first}-{last}/{len}")),
        Plan::Unsatisfiable => Some(format!("bytes */{len}")),
        Plan::Whole | Plan::NotModified => None,
    }
}
