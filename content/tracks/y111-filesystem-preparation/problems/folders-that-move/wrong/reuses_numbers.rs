use std::collections::BTreeMap;

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

struct Folder {
    parent: FolderId,
    name: String,
    children: BTreeMap<String, FolderId>,
}

pub struct Tree {
    /// Indexed by id. `None` once removed: ids are not used again.
    folders: Vec<Option<Folder>>,
}

fn parts(path: &str) -> Result<Vec<&str>, TreeError> {
    let rest = path.strip_prefix('/').ok_or(TreeError::BadPath)?;
    Ok(rest.split('/').filter(|p| !p.is_empty()).collect())
}

impl Tree {
    pub fn new() -> Tree {
        let top = Folder {
            parent: TOP,
            name: String::new(),
            children: BTreeMap::new(),
        };
        Tree {
            folders: vec![Some(top)],
        }
    }

    fn folder(&self, id: FolderId) -> Option<&Folder> {
        self.folders.get(id)?.as_ref()
    }

    /// Changes a folder that is known to be there.
    fn folder_mut(&mut self, id: FolderId) -> &mut Folder {
        self.folders[id].as_mut().expect("the folder is there")
    }

    fn walk(&self, parts: &[&str]) -> Result<FolderId, TreeError> {
        let mut at = TOP;
        for part in parts {
            let here = self.folder(at).ok_or(TreeError::NotFound)?;
            at = match *part {
                "." => at,
                ".." => here.parent,
                name => *here.children.get(name).ok_or(TreeError::NotFound)?,
            };
        }
        Ok(at)
    }

    pub fn find(&self, path: &str) -> Result<FolderId, TreeError> {
        self.walk(&parts(path)?)
    }

    /// For a path whose last part names a folder to make, remove or move: its parent and that name.
    fn place<'p>(&self, path: &'p str) -> Result<(FolderId, &'p str), TreeError> {
        let mut parts = parts(path)?;
        let name = parts.pop().ok_or(TreeError::BadPath)?;
        if name == "." || name == ".." {
            return Err(TreeError::BadPath);
        }
        Ok((self.walk(&parts)?, name))
    }

    pub fn make(&mut self, path: &str) -> Result<FolderId, TreeError> {
        let (parent, name) = self.place(path)?;
        if self.folder_mut(parent).children.contains_key(name) {
            return Err(TreeError::Exists);
        }
        let id = self
            .folders
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.folders.len());
        if id < self.folders.len() {
            self.folders.remove(id);
        }
        self.folders.insert(
            id,
            Some(Folder {
                parent,
                name: name.to_owned(),
                children: BTreeMap::new(),
            }),
        );
        self.folder_mut(parent).children.insert(name.to_owned(), id);
        Ok(id)
    }

    /// The names in a folder, sorted.
    pub fn list(&self, path: &str) -> Result<Vec<String>, TreeError> {
        let id = self.find(path)?;
        let folder = self.folder(id).ok_or(TreeError::NotFound)?;
        Ok(folder.children.keys().cloned().collect())
    }

    /// The path of a folder that exists, from the top, without "." or "..".
    pub fn path_of(&self, id: FolderId) -> Option<String> {
        let mut names = Vec::new();
        let mut at = id;
        while at != TOP {
            let folder = self.folder(at)?;
            names.push(folder.name.as_str());
            at = folder.parent;
        }
        names.reverse();
        Some(format!("/{}", names.join("/")))
    }

    pub fn remove(&mut self, path: &str) -> Result<(), TreeError> {
        let (parent, name) = self.place(path)?;
        let id = self.walk_one(parent, name)?;
        if !self.folder_mut(id).children.is_empty() {
            return Err(TreeError::NotEmpty);
        }
        self.folder_mut(parent).children.remove(name);
        self.folders[id] = None;
        Ok(())
    }

    fn walk_one(&self, parent: FolderId, name: &str) -> Result<FolderId, TreeError> {
        let folder = self.folder(parent).ok_or(TreeError::NotFound)?;
        folder
            .children
            .get(name)
            .copied()
            .ok_or(TreeError::NotFound)
    }

    /// Whether `id` is `ancestor` or somewhere below it.
    fn is_inside(&self, mut id: FolderId, ancestor: FolderId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            match self.folder(id) {
                Some(folder) if id != TOP => id = folder.parent,
                _ => return false,
            }
        }
    }

    /// Gives the folder at `from` the new full path `to`.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), TreeError> {
        let (old_parent, old_name) = self.place(from)?;
        let moved = self.walk_one(old_parent, old_name)?;
        let (new_parent, new_name) = self.place(to)?;
        if self.is_inside(new_parent, moved) {
            return Err(TreeError::IntoItself);
        }
        match self.walk_one(new_parent, new_name) {
            Ok(there) if there == moved => return Ok(()),
            Ok(_) => return Err(TreeError::Exists),
            Err(_) => {}
        }
        self.folder_mut(old_parent).children.remove(old_name);
        self.folder_mut(new_parent)
            .children
            .insert(new_name.to_owned(), moved);
        let folder = self.folder_mut(moved);
        folder.parent = new_parent;
        folder.name = new_name.to_owned();
        Ok(())
    }
}

impl Default for Tree {
    fn default() -> Tree {
        Tree::new()
    }
}
