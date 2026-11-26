use std::fmt::Write;

/// Writes `s` into `w`. Like most generic sinks, it takes the writer by value.
fn put<W: Write>(mut w: W, s: &str) {
    w.write_str(s).expect("writing to a String can't fail");
}

/// Adds `msg` to `alerts`, if there are alerts to add to.
fn note(alerts: Option<&mut Vec<String>>, msg: &str) {
    if let Some(a) = alerts {
        a.push(msg.to_string());
    }
}

/// Writes the first `header` lines as "[<line> / <line>]", then every remaining line as "; <line>". Each
/// remaining line that starts with '!' is also noted in `alerts` (when given), and the last note is
/// "<n> lines", where n counts the remaining lines. Returns n.
pub fn report<'a, I>(
    out: &mut String,
    mut lines: I,
    header: usize,
    mut alerts: Option<&mut Vec<String>>,
) -> usize
where
    I: Iterator<Item = &'a str>,
{
    let head: Vec<&str> = lines.by_ref().take(header).collect();
    put(&mut *out, "[");
    put(&mut *out, &head.join(" / "));
    put(&mut *out, "]");
    let mut n = 0;
    for line in lines {
        put(&mut *out, "; ");
        put(&mut *out, line);
        if line.starts_with('!') {
            note(alerts.as_deref_mut(), line);
        }
        n += 1;
    }
    note(alerts, &format!("{n} lines"));
    n
}
