//! Where the rows of a result go.

pub trait ResultWriter {
    fn write_cell(&mut self, cell: &str);
    fn write_header_cell(&mut self, cell: &str);
    fn begin_header(&mut self);
    fn end_header(&mut self);
    fn begin_row(&mut self);
    fn end_row(&mut self);
    fn begin_table(&mut self, simplified_output: bool);
    fn end_table(&mut self);

    /// A result of one cell (a message such as "Table created with id = 0").
    fn one_cell(&mut self, cell: &str) {
        self.begin_table(true);
        self.begin_row();
        self.write_cell(cell);
        self.end_row();
        self.end_table();
    }
}

/// Drops everything.
pub struct NoopWriter;

impl ResultWriter for NoopWriter {
    fn write_cell(&mut self, _cell: &str) {}
    fn write_header_cell(&mut self, _cell: &str) {}
    fn begin_header(&mut self) {}
    fn end_header(&mut self) {}
    fn begin_row(&mut self) {}
    fn end_row(&mut self) {}
    fn begin_table(&mut self, _simplified_output: bool) {}
    fn end_table(&mut self) {}
}

/// Writes rows as text: every cell followed by `separator`, one line per row.
pub struct SimpleStreamWriter<'w> {
    out: &'w mut String,
    disable_header: bool,
    separator: String,
}

impl<'w> SimpleStreamWriter<'w> {
    pub fn new(out: &'w mut String, disable_header: bool, separator: &str) -> SimpleStreamWriter<'w> {
        SimpleStreamWriter { out, disable_header, separator: separator.to_string() }
    }
}

impl ResultWriter for SimpleStreamWriter<'_> {
    fn write_cell(&mut self, cell: &str) {
        self.out.push_str(cell);
        self.out.push_str(&self.separator);
    }
    fn write_header_cell(&mut self, cell: &str) {
        if !self.disable_header {
            self.out.push_str(cell);
            self.out.push_str(&self.separator);
        }
    }
    fn begin_header(&mut self) {}
    fn end_header(&mut self) {
        if !self.disable_header {
            self.out.push('\n');
        }
    }
    fn begin_row(&mut self) {}
    fn end_row(&mut self) {
        self.out.push('\n');
    }
    fn begin_table(&mut self, _simplified_output: bool) {}
    fn end_table(&mut self) {}
}

/// Collects the cells of the last table as strings.
#[derive(Default)]
pub struct StringVectorWriter {
    pub values: Vec<Vec<String>>,
}

impl ResultWriter for StringVectorWriter {
    fn write_cell(&mut self, cell: &str) {
        self.values.last_mut().unwrap().push(cell.to_string());
    }
    fn write_header_cell(&mut self, _cell: &str) {}
    fn begin_header(&mut self) {}
    fn end_header(&mut self) {}
    fn begin_row(&mut self) {
        self.values.push(vec![]);
    }
    fn end_row(&mut self) {}
    fn begin_table(&mut self, _simplified_output: bool) {
        self.values.clear();
    }
    fn end_table(&mut self) {}
}
