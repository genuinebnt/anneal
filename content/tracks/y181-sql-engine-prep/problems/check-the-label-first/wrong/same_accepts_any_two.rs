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
    type Fields = [(String, Kind)];

    /// One operand: checks inside it first, then its kind against what its place needs.
    fn operand(
        formula: &Formula,
        needed: Option<Kind>,
        fields: &Fields,
    ) -> Result<(Box<Checked>, Option<Kind>), Fault> {
        let (checked, found) = walk(formula, fields)?;
        match (needed, found) {
            (Some(needed), Some(found)) if needed != found => {
                Err(Fault::WrongKind { needed, found })
            }
            _ => Ok((Box::new(checked), found)),
        }
    }

    fn walk(formula: &Formula, fields: &Fields) -> Result<(Checked, Option<Kind>), Fault> {
        Ok(match formula {
            Formula::Number(n) => (Checked::Number(*n), Some(Kind::Number)),
            Formula::Words(w) => (Checked::Words(w.clone()), Some(Kind::Words)),
            Formula::YesNo(b) => (Checked::YesNo(*b), Some(Kind::YesNo)),
            Formula::Blank => (Checked::Blank, None),
            Formula::Field(name) => {
                let at = fields
                    .iter()
                    .position(|(field, _)| field == name)
                    .ok_or_else(|| Fault::NoSuchField(name.clone()))?;
                (Checked::Field(at), Some(fields[at].1))
            }
            Formula::Add(a, b) => {
                let (a, _) = operand(a, Some(Kind::Number), fields)?;
                let (b, _) = operand(b, Some(Kind::Number), fields)?;
                (Checked::Add(a, b), Some(Kind::Number))
            }
            Formula::Join(parts) => {
                let parts = parts
                    .iter()
                    .map(|p| Ok(*operand(p, Some(Kind::Words), fields)?.0))
                    .collect::<Result<_, Fault>>()?;
                (Checked::Join(parts), Some(Kind::Words))
            }
            Formula::Same(a, b) => {
                let (a, first) = operand(a, None, fields)?;
                let _ = first;
                let (b, _) = operand(b, None, fields)?;
                (Checked::Same(a, b), Some(Kind::YesNo))
            }
            Formula::Pick(test, a, b) => {
                let (test, _) = operand(test, Some(Kind::YesNo), fields)?;
                let (a, first) = operand(a, None, fields)?;
                let (b, second) = operand(b, first, fields)?;
                (Checked::Pick(test, a, b), first.or(second))
            }
        })
    }

    walk(formula, fields)
}
