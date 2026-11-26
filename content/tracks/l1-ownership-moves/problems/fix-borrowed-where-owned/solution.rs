/// A tag owns everything in it, so it can outlive whatever it was built from.
#[derive(Debug, PartialEq)]
pub struct Tag {
    pub name: String,
    pub aliases: Vec<String>,
    pub note: Option<String>,
}

impl Tag {
    /// A caller that already owns the name hands it over; one with a `&str` pays for one copy.
    pub fn new(name: impl Into<String>, aliases: &[&str], note: Option<&str>) -> Tag {
        Tag {
            name: name.into(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            note: note.map(String::from),
        }
    }
}
