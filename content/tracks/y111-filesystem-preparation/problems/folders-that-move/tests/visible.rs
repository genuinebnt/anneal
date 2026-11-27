use solution::*;

fn tree(paths: &[&str]) -> Tree {
    let mut tree = Tree::new();
    for path in paths {
        tree.make(path).unwrap();
    }
    tree
}

#[test]
fn make_find_and_list() {
    let mut t = tree(&["/music", "/music/live", "/books"]);
    let live = t.find("/music/live").unwrap();
    check!("the top", t.find("/"), Ok(TOP));
    check!("dots", t.find("/books/../music/./live"), Ok(live));
    check!("list", t.list("/"), Ok(vec!["books".to_string(), "music".to_string()]));
    check!("made twice", t.make("/music"), Err(TreeError::Exists));
    check!("no parent", t.make("/films/new"), Err(TreeError::NotFound));
}

#[test]
fn a_moved_folder_keeps_its_number_and_contents() {
    let mut t = tree(&["/a", "/a/b", "/a/b/c", "/x"]);
    let (b, c) = (t.find("/a/b").unwrap(), t.find("/a/b/c").unwrap());
    check!("rename", t.rename("/a/b", "/x/y"), Ok(()));
    check!("same numbers", (t.find("/x/y"), t.find("/x/y/c")), (Ok(b), Ok(c)));
    check!("new path", t.path_of(c), Some("/x/y/c".to_string()));
    check!("old path", t.find("/a/b"), Err(TreeError::NotFound));
}

#[test]
fn a_folder_cannot_move_into_itself() {
    let mut t = tree(&["/a", "/a/b"]);
    check!("into itself", t.rename("/a", "/a/a2"), Err(TreeError::IntoItself));
    check!("further down", t.rename("/a", "/a/b/a2"), Err(TreeError::IntoItself));
    check!("still there", t.find("/a/b").is_ok(), true);
}

#[test]
fn a_name_that_only_looks_inside() {
    let mut t = tree(&["/a", "/ab"]);
    check!("/a into /ab", t.rename("/a", "/ab/a"), Ok(()));
}

#[test]
fn only_empty_folders_are_removed() {
    let mut t = tree(&["/a", "/a/b"]);
    check!("not empty", t.remove("/a"), Err(TreeError::NotEmpty));
    check!("inner", t.remove("/a/b"), Ok(()));
    check!("outer", t.remove("/a"), Ok(()));
    check!("gone", t.find("/a"), Err(TreeError::NotFound));
}
