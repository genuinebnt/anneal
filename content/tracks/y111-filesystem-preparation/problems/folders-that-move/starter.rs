/// A folder's number. It never changes, whatever the folder is called or wherever it is. The top folder is 0.
pub type FolderId = usize;
pub const TOP: FolderId = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    BadPath,
    NotFound,
    Exists,
    NotEmpty,
    IntoItself,
}

pub struct Tree {
    // TODO: the folders
}

impl Tree {
    pub fn new() -> Tree {
        todo!()
    }

    pub fn find(&self, path: &str) -> Result<FolderId, TreeError> {
        todo!()
    }

    pub fn make(&mut self, path: &str) -> Result<FolderId, TreeError> {
        todo!()
    }

    /// The names in a folder, sorted.
    pub fn list(&self, path: &str) -> Result<Vec<String>, TreeError> {
        todo!()
    }

    /// The path of a folder that exists, from the top, without "." or "..".
    pub fn path_of(&self, id: FolderId) -> Option<String> {
        todo!()
    }

    pub fn remove(&mut self, path: &str) -> Result<(), TreeError> {
        todo!()
    }

    /// Gives the folder at `from` the new full path `to`.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), TreeError> {
        todo!()
    }
}

impl Default for Tree {
    fn default() -> Tree {
        Tree::new()
    }
}
