use std::fmt;

/// A titled list: the title, then one `- row` line per row.
pub struct Report<C> {
    title: String,
    rows: C,
}

impl<C> Report<C> {
    pub fn new(title: &str, rows: C) -> Self {
        Report {
            title: title.to_string(),
            rows,
        }
    }

    pub fn rows(&self) -> &C {
        &self.rows
    }
}

// `fmt` borrows `self` for a lifetime this impl can't name, so the bound must hold for every lifetime.
impl<C> fmt::Display for Report<C>
where
    for<'a> &'a C: IntoIterator,
    for<'a> <&'a C as IntoIterator>::Item: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.title)?;
        for row in &self.rows {
            write!(f, "\n- {row}")?;
        }
        Ok(())
    }
}
