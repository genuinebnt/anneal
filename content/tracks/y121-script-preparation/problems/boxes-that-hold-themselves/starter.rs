use std::cell::RefCell;
use std::rc::Rc;

/// A list that can be shared, changed, and put inside other lists, itself included.
pub type List = Rc<RefCell<Vec<Item>>>;

#[derive(Clone)]
pub enum Item {
    Number(i64),
    List(List),
}

/// The item as text, or `None` if the text would be longer than `limit` bytes.
pub fn show(item: &Item, limit: usize) -> Option<String> {
    todo!()
}
