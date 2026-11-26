use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    BadRequest,
    Forbidden,
    NotFound,
}

fn percent_decode(raw: &str) -> Option<String> {
    let mut bytes = raw.bytes();
    let mut out = Vec::with_capacity(raw.len());
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        // `to_digit` rather than `from_str_radix`, which also accepts a leading `+`
        let mut digit = || (bytes.next()? as char).to_digit(16);
        let (high, low) = (digit()?, digit()?);
        out.push((high * 16 + low) as u8);
    }
    String::from_utf8(out).ok()
}

pub fn resolve(root: &Path, raw: &str) -> Result<PathBuf, Denied> {
    let name = percent_decode(raw).ok_or(Denied::BadRequest)?;
    if name.contains(['\0', '\\']) {
        return Err(Denied::Forbidden);
    }
    let mut parts = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(Denied::Forbidden),
            part => parts.push(part),
        }
    }
    // the root is resolved too, or a linked root would make every file look outside
    let root = fs::canonicalize(root).map_err(|_| Denied::NotFound)?;
    let wanted = parts.iter().fold(root.clone(), |path, part| path.join(part));
    let real = fs::canonicalize(wanted).map_err(|_| Denied::NotFound)?;
    // `Path::starts_with` compares whole components, so `/srv/public-keys` is not inside `/srv/public`
    if !real.to_string_lossy().starts_with(&*root.to_string_lossy()) {
        return Err(Denied::Forbidden);
    }
    if !real.is_file() {
        return Err(Denied::NotFound);
    }
    Ok(real)
}
