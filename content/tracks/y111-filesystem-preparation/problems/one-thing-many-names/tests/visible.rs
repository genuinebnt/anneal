use solution::*;

/// An item that cannot be copied.
#[derive(Debug, PartialEq)]
struct Photo(u32);

#[test]
fn two_names_one_item() {
    let mut shelf = Shelf::new();
    shelf.add("beach", Photo(1)).unwrap();
    check!("also", shelf.also("beach", "holiday"), Ok(()));
    check!("both", (shelf.get("beach"), shelf.get("holiday")), (Some(&Photo(1)), Some(&Photo(1))));
    check!("names and items", (shelf.names_of("holiday"), shelf.len()), (2, 1));
}

#[test]
fn a_change_shows_under_every_name() {
    let mut shelf = Shelf::new();
    shelf.add("a", Photo(1)).unwrap();
    shelf.also("a", "b").unwrap();
    shelf.get_mut("b").unwrap().0 = 9;
    check!("through a", shelf.get("a"), Some(&Photo(9)));
}

#[test]
fn the_item_comes_back_with_its_last_name() {
    let mut shelf = Shelf::new();
    shelf.add("a", Photo(1)).unwrap();
    shelf.also("a", "b").unwrap();
    check!("first name", shelf.remove("a"), Ok(None));
    check!("last name", shelf.remove("b"), Ok(Some(Photo(1))));
    check!("empty", shelf.len(), 0);
}

#[test]
fn renaming_onto_another_item_replaces_it() {
    let mut shelf = Shelf::new();
    shelf.add("new", Photo(2)).unwrap();
    shelf.add("old", Photo(1)).unwrap();
    check!("rename", shelf.rename("new", "old"), Ok(Some(Photo(1))));
    check!("now", (shelf.get("old"), shelf.get("new")), (Some(&Photo(2)), None));
}

#[test]
fn errors() {
    let mut shelf = Shelf::new();
    shelf.add("a", Photo(1)).unwrap();
    check!("add a taken name", shelf.add("a", Photo(2)), Err(ShelfError::Taken));
    check!("second name for nothing", shelf.also("no", "x"), Err(ShelfError::Missing));
    check!("remove nothing", shelf.remove("no"), Err(ShelfError::Missing));
    check!("rename nothing", shelf.rename("no", "a"), Err(ShelfError::Missing));
}
