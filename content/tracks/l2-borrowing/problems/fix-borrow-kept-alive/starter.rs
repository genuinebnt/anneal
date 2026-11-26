/// Collects lines and appends them to `out` when it's dropped.
pub struct Batch<'a> {
    out: &'a mut Vec<String>,
    buf: Vec<String>,
}

impl<'a> Batch<'a> {
    pub fn new(out: &'a mut Vec<String>) -> Self {
        Batch {
            out,
            buf: Vec::new(),
        }
    }

    pub fn line(&mut self, s: String) {
        self.buf.push(s);
    }
}

impl Drop for Batch<'_> {
    fn drop(&mut self) {
        self.out.append(&mut self.buf);
    }
}

/// Renders `items` into `out`:
/// - if `out` already has lines, its first line is a title: append " (cont.)" to it; otherwise push "untitled";
/// - push each item as "- <item>";
/// - through a `Batch`, add "<n> items, longest: <item>", naming the first of the longest items (by bytes), or
///   "-" when there are none.
/// Returns how many lines `out` has afterwards.
pub fn render(out: &mut Vec<String>, items: &[&str]) -> usize {
    let title = out.first_mut();
    if out.is_empty() {
        out.push("untitled".to_string());
    }
    if let Some(t) = title {
        t.push_str(" (cont.)");
    }
    let mut longest: Option<&str> = None;
    for item in items {
        out.push(format!("- {item}"));
        let line = &out[out.len() - 1][2..];
        if longest.map_or(true, |l| line.len() > l.len()) {
            longest = Some(line);
        }
    }
    let mut batch = Batch::new(out);
    batch.line(format!(
        "{} items, longest: {}",
        items.len(),
        longest.unwrap_or("-")
    ));
    out.len()
}
