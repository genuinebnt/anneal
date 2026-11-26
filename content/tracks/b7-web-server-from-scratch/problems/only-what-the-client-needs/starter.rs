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

pub fn plan(ask: &Ask, file: &FileInfo) -> Plan {
    todo!()
}

pub fn content_range(plan: Plan, len: u64) -> Option<String> {
    todo!()
}
