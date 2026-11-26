use std::fmt;

/// A titled list: the title, then one `- row` line per row.
pub struct Report<T> {
    title: String,
    rows: Vec<T>,
}

impl<T> Report<T> {
    pub fn new(title: &str, rows: Vec<T>) -> Self {
        Report {
            title: title.to_string(),
            rows,
        }
    }

    pub fn rows(&self) -> &Vec<T> {
        &self.rows
    }
}

impl<T: fmt::Display> fmt::Display for Report<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.title)?;
        for row in &self.rows {
            write!(f, "\n- {row}")?;
        }
        Ok(())
    }
}
