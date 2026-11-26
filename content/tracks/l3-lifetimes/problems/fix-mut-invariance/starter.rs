use std::cell::Cell;

fn add_defaults(names: &mut Vec<&'static str>) {
    names.push("root");
    names.push("admin");
}

/// The names in `input` (one per line), then "root" and "admin".
pub fn all_names(input: &str) -> Vec<&str> {
    let mut names: Vec<&str> = input.lines().collect();
    add_defaults(&mut names);
    names
}

/// Keeps in `slot` the shorter of its current value and `candidate` (the current one on a tie).
pub fn keep_shortest(slot: &Cell<&'static str>, candidate: &str) {
    if candidate.len() < slot.get().len() {
        slot.set(candidate);
    }
}

/// The shortest line of `text` (the first on a tie), or "(none)" when there are no lines.
pub fn shortest_line(text: &str) -> &str {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return "(none)";
    };
    let slot = Cell::new(first);
    for line in lines {
        keep_shortest(&slot, line);
    }
    slot.get()
}

/// Applies `measure` to `s`.
pub fn apply(measure: fn(&'static str) -> usize, s: &str) -> usize {
    measure(s)
}

/// The same names, typed as shorter-lived references.
pub fn shorten<'a>(v: Vec<&'static str>) -> Vec<&'a str> {
    v
}
