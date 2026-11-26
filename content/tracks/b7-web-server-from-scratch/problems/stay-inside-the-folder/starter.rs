use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    BadRequest,
    Forbidden,
    NotFound,
}

pub fn resolve(root: &Path, raw: &str) -> Result<PathBuf, Denied> {
    todo!()
}
