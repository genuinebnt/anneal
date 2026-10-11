//! The learner-facing part of a course (`course.toml`, `lectures.toml`, `modules/`, `template/`) as one blob.
//!
//! The CLI's build script packs every course into its binary with this module, and the app serves one course's blob at
//! `/courses-bundle/<id>`, so `anneal course update` can bring in new stages and tests without a new binary. The reference solution is
//! never part of it. The file uses `std` only: `crates/cli/build.rs` includes it by path.
//!
//! Blob format: repeated `[u32 path length][path bytes][u64 data length][data bytes]`, little-endian, paths relative to `courses/`.

use std::fs;
use std::path::{Path, PathBuf};

/// The blob format this build reads and writes. A bundle with a higher number needs a newer CLI.
pub const FORMAT: u32 = 1;

fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            // a build directory left in a local checkout is never part of a course
            if p.file_name().is_some_and(|n| n == "target" || n == ".git") {
                continue;
            }
            walk(base, &p, out);
        } else if p.file_name().is_some_and(|n| n != ".DS_Store") {
            out.push(p.strip_prefix(base).unwrap().to_path_buf());
        }
    }
}

/// The learner-facing files of the courses under `courses/` (only `only`, if given), as (path relative to `courses/`, bytes), in a fixed
/// order: courses by id; per course its two files, then `modules/`, then `template/`.
pub fn collect(courses: &Path, only: Option<&str>) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(ids) = fs::read_dir(courses) {
        let mut ids: Vec<_> = ids.flatten().map(|e| e.path()).filter(|p| p.join("course.toml").is_file()).collect();
        ids.sort();
        for id in ids {
            let name = id.file_name().unwrap().to_owned();
            if only.is_some_and(|o| name != o) {
                continue;
            }
            for single in ["course.toml", "lectures.toml"] {
                if id.join(single).is_file() {
                    files.push(Path::new(&name).join(single));
                }
            }
            for sub in ["modules", "template"] {
                walk(courses, &id.join(sub), &mut files);
            }
        }
    }
    files
        .into_iter()
        .filter_map(|rel| Some((rel.to_string_lossy().replace('\\', "/"), fs::read(courses.join(&rel)).ok()?)))
        .collect()
}

pub fn pack(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut blob = Vec::new();
    for (path, data) in entries {
        blob.extend((path.len() as u32).to_le_bytes());
        blob.extend(path.as_bytes());
        blob.extend((data.len() as u64).to_le_bytes());
        blob.extend(data);
    }
    blob
}

/// The files of a blob. A blob from the network may be cut short or made up, so this checks every length instead of trusting it.
pub fn unpack(blob: &[u8]) -> Result<Vec<(&str, &[u8])>, String> {
    let take = |at: usize, n: usize| -> Result<&[u8], String> {
        at.checked_add(n).and_then(|end| blob.get(at..end)).ok_or_else(|| "the bundle is cut short".to_owned())
    };
    let mut out = Vec::new();
    let mut at = 0;
    while at < blob.len() {
        let plen = u32::from_le_bytes(take(at, 4)?.try_into().unwrap()) as usize;
        at += 4;
        let path = std::str::from_utf8(take(at, plen)?).map_err(|_| "a path in the bundle is not text".to_owned())?;
        at += plen;
        let dlen = u64::from_le_bytes(take(at, 8)?.try_into().unwrap()) as usize;
        at += 8;
        out.push((path, take(at, dlen)?));
        at += dlen;
    }
    Ok(out)
}

/// A path inside `courses/` that stays inside it: relative, no `..`, no drive or root.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains('\\') && !path.contains(':') && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// FNV-1a: names a bundle by its content (the cache directory, the ETag).
pub fn fingerprint(blob: &[u8]) -> u64 {
    blob.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<(String, Vec<u8>)> {
        vec![("c/course.toml".into(), b"id = \"c\"\n".to_vec()), ("c/template/src/lib.rs".into(), b"// hi\n".to_vec())]
    }

    #[test]
    fn a_blob_round_trips() {
        let blob = pack(&entries());
        let back: Vec<(String, Vec<u8>)> = unpack(&blob).unwrap().into_iter().map(|(p, d)| (p.to_owned(), d.to_vec())).collect();
        assert_eq!(back, entries());
    }

    #[test]
    fn a_cut_or_made_up_blob_is_refused_not_trusted() {
        let blob = pack(&entries());
        for cut in [1, 5, blob.len() - 1] {
            assert!(unpack(&blob[..cut]).is_err(), "cut at {cut}");
        }
        // a length that points far past the end must not overflow or panic
        let mut huge = Vec::new();
        huge.extend(u32::MAX.to_le_bytes());
        assert!(unpack(&huge).is_err());
        // a data length of u64::MAX
        let mut lie = Vec::new();
        lie.extend(1u32.to_le_bytes());
        lie.push(b'a');
        lie.extend(u64::MAX.to_le_bytes());
        assert!(unpack(&lie).is_err());
    }

    #[test]
    fn only_paths_inside_courses_are_safe() {
        for ok in ["bustub/course.toml", "bustub/template/src/lib.rs"] {
            assert!(safe_path(ok), "{ok}");
        }
        for bad in ["", "/etc/passwd", "../x", "a/../b", "a//b", "a/./b", "C:/x", "a\\b"] {
            assert!(!safe_path(bad), "{bad}");
        }
    }

    #[test]
    fn the_fingerprint_follows_the_content() {
        assert_ne!(fingerprint(&pack(&entries())), fingerprint(&pack(&entries()[..1])));
        assert_eq!(fingerprint(b"same"), fingerprint(b"same"));
    }
}
