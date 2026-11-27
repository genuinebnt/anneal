#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Words,
    YesNo,
}

/// A formula as it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    Number(i64),
    Words(String),
    YesNo(bool),
    Blank,
    Field(String),
    Add(Box<Formula>, Box<Formula>),
    Join(Vec<Formula>),
    Same(Box<Formula>, Box<Formula>),
    Pick(Box<Formula>, Box<Formula>, Box<Formula>),
}

/// A formula that passed the check. A field is now its index in the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    Number(i64),
    Words(String),
    YesNo(bool),
    Blank,
    Field(usize),
    Add(Box<Checked>, Box<Checked>),
    Join(Vec<Checked>),
    Same(Box<Checked>, Box<Checked>),
    Pick(Box<Checked>, Box<Checked>, Box<Checked>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    NoSuchField(String),
    WrongKind { needed: Kind, found: Kind },
}

/// The checked formula and its kind (`None`: it fits anything), or the first mistake.
pub fn check(
    formula: &Formula,
    fields: &[(String, Kind)],
) -> Result<(Checked, Option<Kind>), Fault> {
    let _ = (formula, fields);
    todo!()
}
