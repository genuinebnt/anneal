use std::collections::HashMap;
use std::io::{self, Read};

/// Word counts, most frequent first; ties in byte order of the word.
pub fn word_counts<R: Read>(mut reader: R) -> io::Result<Vec<(String, usize)>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut buf = [0u8; 4096];
    let mut partial = String::new();
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = std::str::from_utf8(&buf[..n])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        partial.push_str(chunk);
        // Count every complete word; keep a word that may continue in the next chunk.
        let keep_from = partial
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + partial[i..].chars().next().unwrap().len_utf8());
        for w in partial[..keep_from].split_whitespace() {
            *counts.entry(w.to_string()).or_insert(0) += 1;
        }
        partial = partial[keep_from..].to_string();
    }
    for w in partial.split_whitespace() {
        *counts.entry(w.to_string()).or_insert(0) += 1;
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by_key(|(w, n)| (std::cmp::Reverse(*n), w.clone()));
    Ok(out)
}
