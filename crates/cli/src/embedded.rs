//! The courses compiled into the binary (see build.rs): `anneal course init bustub` works from any directory, with no checkout.
//! The files are written once to a cache directory named after the content, and the rest of the CLI reads them from there as it reads
//! a `courses/` directory.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

static BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/courses.bin"));

/// The embedded files as (relative path, bytes).
pub fn entries(blob: &[u8]) -> Vec<(&str, &[u8])> {
    anneal_content::bundle::unpack(blob).unwrap_or_default()
}

/// Names the cache directory after the content, so a new build never reads an old extraction.
fn fingerprint(blob: &[u8]) -> u64 {
    anneal_content::bundle::fingerprint(blob)
}

/// The fingerprint of one embedded course as the app serves it (`/courses-bundle/<id>`), so the two can be compared.
pub fn course_fingerprint(id: &str) -> Option<u64> {
    let prefix = format!("{id}/");
    let mine: Vec<(String, Vec<u8>)> = entries(BLOB).into_iter().filter(|(p, _)| p.starts_with(&prefix)).map(|(p, d)| (p.to_owned(), d.to_vec())).collect();
    (!mine.is_empty()).then(|| anneal_content::bundle::fingerprint(&anneal_content::bundle::pack(&mine)))
}

pub fn cache_base() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))).map(|b| b.join("anneal"))
}

/// The `courses/` directory holding the embedded courses, written if it is not there yet. `None` if the binary carries no courses.
pub fn courses_dir() -> anyhow::Result<Option<PathBuf>> {
    if BLOB.is_empty() {
        return Ok(None);
    }
    let base = cache_base().context("no home directory for the course cache")?;
    let dir = base.join(format!("courses-{:016x}", fingerprint(BLOB)));
    if !dir.join(".complete").exists() {
        extract(BLOB, &dir)?;
    }
    Ok(Some(dir))
}

/// Writes a blob's files under `dir`, then a `.complete` marker. A blob that is cut short, or names a path outside `dir`, is refused
/// before anything is written (the blob may have come over the network).
pub fn extract(blob: &[u8], dir: &Path) -> anyhow::Result<()> {
    let files = anneal_content::bundle::unpack(blob).map_err(anyhow::Error::msg)?;
    if let Some((bad, _)) = files.iter().find(|(p, _)| !anneal_content::bundle::safe_path(p)) {
        anyhow::bail!("the bundle names a path outside the course directory: {bad:?}");
    }
    for (rel, data) in files {
        let target = dir.join(rel);
        fs::create_dir_all(target.parent().context("a file has no parent")?)?;
        fs::write(&target, data).with_context(|| format!("writing {}", target.display()))?;
    }
    fs::write(dir.join(".complete"), b"")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blob_round_trips_through_extraction() {
        let mut blob = Vec::new();
        for (path, data) in [("c/course.toml", &b"id = \"c\"\n"[..]), ("c/template/src/lib.rs", &b"// hi\n"[..])] {
            blob.extend((path.len() as u32).to_le_bytes());
            blob.extend(path.as_bytes());
            blob.extend((data.len() as u64).to_le_bytes());
            blob.extend(data);
        }
        assert_eq!(entries(&blob).len(), 2);
        let evil = anneal_content::bundle::pack(&[("../escape".to_owned(), b"x".to_vec())]);
        assert!(extract(&evil, &std::env::temp_dir().join("anneal-embedded-evil")).is_err());
        assert!(extract(&blob[..blob.len() - 1], &std::env::temp_dir().join("anneal-embedded-cut")).is_err());
        let dir = std::env::temp_dir().join(format!("anneal-embedded-test-{}", std::process::id()));
        extract(&blob, &dir).unwrap();
        assert_eq!(fs::read_to_string(dir.join("c/template/src/lib.rs")).unwrap(), "// hi\n");
        assert!(dir.join(".complete").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
