#[derive(Debug, PartialEq)]
pub struct Batch {
    pub id: u32,
    pub label: Option<String>,
    pub items: Vec<String>,
}

/// One line about the batch, and its items handed back for reuse. The line is
/// "<label, or #<id>>: <n> items (<k> long), longest <item>", where long means more than 3 bytes and the
/// longest item is the first of the longest ("-" when there are none). A batch without a label also gets
/// " [unlabelled]" at the end.
pub fn describe(batch: Batch) -> (String, Vec<String>) {
    let unlabelled = batch.label.is_none();
    let name = batch.label.unwrap_or(format!("#{}", batch.id));
    let longest = longest_item(&batch.items);
    let mut long = 0;
    for item in &batch.items {
        if item.len() > 3 {
            long += 1;
        }
    }
    let mut line = format!(
        "{name}: {} items ({long} long), longest {}",
        batch.items.len(),
        longest.unwrap_or("-")
    );
    if unlabelled {
        line.push_str(" [unlabelled]");
    }
    (line, batch.items)
}

fn longest_item(items: &[String]) -> Option<&str> {
    let mut best: Option<&str> = None;
    for item in items {
        if best.map_or(true, |b| item.len() > b.len()) {
            best = Some(item);
        }
    }
    best
}
