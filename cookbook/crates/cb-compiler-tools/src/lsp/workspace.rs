//! Multi-document support: workspace folders, a capped scan for `*.peb` files, a
//! per-file symbol cache keyed by (mtime, length), and `workspace/symbol`.
//!
//! WHY a separate cache from the open-document store: files that are not open are read
//! from disk and analysed lazily, once per on-disk version; open documents are always
//! answered from the editor's in-memory text (their snapshot is keyed by version in
//! the server). `workspace/didChangeWatchedFiles` invalidates single entries.
//!
//! Limits (a runaway folder such as `$HOME` must not hang the server): at most
//! [`MAX_FILES`] files, depth [`MAX_DEPTH`], files up to [`MAX_FILE_BYTES`]; hidden
//! directories, `target` and `node_modules` are skipped.

use super::fuzzy::{self, Score};
use super::snapshot::Snapshot;
use crate::analysis::sema::DefKind;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

pub const MAX_FILES: usize = 2000;
pub const MAX_DEPTH: usize = 12;
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_RESULTS: usize = 200;

/// One workspace-symbol candidate, ready to serialise.
#[derive(Clone, Debug)]
pub struct WsSymbol {
    pub name: String,
    pub kind: u32,
    pub container: Option<String>,
    pub uri: String,
    pub range: Value,
    /// Sort key for ties (line, character).
    pub at: (u64, u64),
}

struct FileEntry {
    stamp: Option<(SystemTime, u64)>,
    symbols: Option<Rc<Vec<WsSymbol>>>,
}

#[derive(Default)]
pub struct Workspace {
    pub folders: Vec<PathBuf>,
    files: BTreeMap<PathBuf, FileEntry>,
    /// Set when the scan stopped at [`MAX_FILES`].
    pub truncated: bool,
    /// How many disk files were analysed (tests assert caching).
    pub disk_analyses: usize,
}

// ---------------------------------------------------------------- URIs

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

pub fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy();
    let mut out = String::from("file://");
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------- scanning

fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || name == "target" || name == "node_modules"
}

fn is_peb(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "peb")
}

impl Workspace {
    pub fn set_folders(&mut self, folders: Vec<PathBuf>) {
        self.folders = folders;
        self.rescan();
    }

    pub fn add_folder(&mut self, f: PathBuf) {
        if !self.folders.contains(&f) {
            self.folders.push(f);
        }
        self.rescan();
    }

    pub fn remove_folder(&mut self, f: &Path) {
        self.folders.retain(|x| x != f);
        self.rescan();
    }

    /// Discover `*.peb` files under every folder (capped), keeping cached symbols of unchanged files.
    pub fn rescan(&mut self) {
        let mut found: Vec<PathBuf> = Vec::new();
        self.truncated = false;
        let folders = self.folders.clone();
        for f in &folders {
            self.walk(f, 0, &mut found);
        }
        found.sort();
        found.dedup();
        let mut files = BTreeMap::new();
        for p in found {
            let entry = self.files.remove(&p).unwrap_or(FileEntry {
                stamp: None,
                symbols: None,
            });
            files.insert(p, entry);
        }
        self.files = files;
    }

    fn walk(&mut self, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH || out.len() >= MAX_FILES {
            self.truncated |= out.len() >= MAX_FILES;
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                if !skip_dir(&name) {
                    self.walk(&path, depth + 1, out);
                }
            } else if ft.is_file() && is_peb(&path) {
                if out.len() >= MAX_FILES {
                    self.truncated = true;
                    return;
                }
                out.push(path);
            }
        }
    }

    fn inside(&self, p: &Path) -> bool {
        self.folders.iter().any(|f| p.starts_with(f))
    }

    pub fn files(&self) -> Vec<PathBuf> {
        self.files.keys().cloned().collect()
    }

    /// `workspace/didChangeWatchedFiles`: type 1 created, 2 changed, 3 deleted.
    pub fn file_event(&mut self, uri: &str, kind: i64) {
        let Some(p) = uri_to_path(uri) else { return };
        if !is_peb(&p) {
            return;
        }
        match kind {
            3 => {
                self.files.remove(&p);
            }
            1 | 2 => {
                if self.inside(&p) && (self.files.len() < MAX_FILES || self.files.contains_key(&p))
                {
                    self.files.insert(
                        p,
                        FileEntry {
                            stamp: None,
                            symbols: None,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    /// Symbols of a file that is not open (analysed once per on-disk version).
    fn disk_symbols(&mut self, path: &Path) -> Option<Rc<Vec<WsSymbol>>> {
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > MAX_FILE_BYTES {
            return None;
        }
        let stamp = (meta.modified().ok()?, meta.len());
        let entry = self.files.get_mut(path)?;
        if entry.stamp == Some(stamp)
            && let Some(s) = &entry.symbols
        {
            return Some(s.clone());
        }
        let text = std::fs::read_to_string(path).ok()?;
        let snap = Snapshot::new(&path_to_uri(path), 0, &text);
        let syms = Rc::new(symbols_of(&snap));
        self.disk_analyses += 1;
        let entry = self.files.get_mut(path)?;
        entry.stamp = Some(stamp);
        entry.symbols = Some(syms.clone());
        Some(syms)
    }

    /// Symbols of every workspace file not in `open` (a set of URIs served from memory).
    pub fn closed_symbols(&mut self, open: &dyn Fn(&str) -> bool) -> Vec<Rc<Vec<WsSymbol>>> {
        let paths: Vec<PathBuf> = self.files.keys().cloned().collect();
        paths
            .iter()
            .filter(|p| !open(&path_to_uri(p)))
            .filter_map(|p| self.disk_symbols(p))
            .collect()
    }
}

// ---------------------------------------------------------------- symbols

/// Searchable symbols of one analysed document: functions, structs, enums, constants,
/// variants (container: the enum) and fields (container: the struct).
pub fn symbols_of(s: &Snapshot) -> Vec<WsSymbol> {
    let mut out = Vec::new();
    for sym in s.an.index.symbols() {
        let kind = match sym.kind {
            DefKind::Fn => 12,
            DefKind::Struct => 23,
            DefKind::Enum => 10,
            DefKind::Const => 14,
            DefKind::Variant => 22,
            DefKind::Field => 8,
            _ => continue,
        };
        if sym.span.is_empty() || sym.name.chars().all(|c| c.is_ascii_digit()) {
            continue; // tuple-variant fields are named `0`, `1`
        }
        let container = sym
            .container
            .map(|c| s.an.index.def(c).name.clone())
            .filter(|n| !n.is_empty());
        let start = s.lines.line_col_utf16(&s.src, sym.span.start as usize);
        out.push(WsSymbol {
            name: sym.name.clone(),
            kind,
            container,
            uri: s.uri.clone(),
            range: s.range(sym.span),
            at: (start.line as u64, start.col as u64),
        });
    }
    out
}

/// `workspace/symbol`: fuzzy-rank `query` over all candidates, best first, capped.
pub fn query(query: &str, candidates: impl IntoIterator<Item = Rc<Vec<WsSymbol>>>) -> Value {
    let mut scored: Vec<(Score, (String, String, (u64, u64)), WsSymbol)> = Vec::new();
    for set in candidates {
        for sym in set.iter() {
            if let Some(sc) = fuzzy::score(query, &sym.name) {
                scored.push((sc, (sym.name.clone(), sym.uri.clone(), sym.at), sym.clone()));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.truncate(MAX_RESULTS);
    Value::Array(
        scored
            .into_iter()
            .map(|(_, _, s)| {
                let mut v = json!({"name": s.name, "kind": s.kind, "location": {"uri": s.uri, "range": s.range}});
                if let Some(c) = s.container {
                    v["containerName"] = json!(c);
                }
                v
            })
            .collect(),
    )
}

/// Folders announced by `initialize` (`workspaceFolders`, else `rootUri`, else `rootPath`).
pub fn folders_from_initialize(p: &Value) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = p
        .get("workspaceFolders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.get("uri").and_then(Value::as_str).and_then(uri_to_path))
        .collect();
    if out.is_empty() {
        if let Some(p) = p
            .get("rootUri")
            .and_then(Value::as_str)
            .and_then(uri_to_path)
        {
            out.push(p);
        } else if let Some(p) = p.get("rootPath").and_then(Value::as_str) {
            out.push(PathBuf::from(p));
        }
    }
    out
}
