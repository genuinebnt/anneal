use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Write;
use std::rc::Rc;

/// A list that can be shared, changed, and put inside other lists, itself included.
pub type List = Rc<RefCell<Vec<Item>>>;

#[derive(Clone)]
pub enum Item {
    Number(i64),
    List(List),
}

enum Task {
    Show(Item),
    Text(&'static str),
    Leave(*const RefCell<Vec<Item>>),
}

/// The item as text, or `None` if the text would be longer than `limit` bytes.
pub fn show(item: &Item, limit: usize) -> Option<String> {
    let mut out = String::new();
    let mut inside = HashSet::new();
    let mut tasks = vec![Task::Show(item.clone())];
    while let Some(task) = tasks.pop() {
        match task {
            Task::Text(text) => out.push_str(text),
            Task::Leave(list) => {
                inside.remove(&list);
            }
            Task::Show(Item::Number(n)) => {
                let _ = write!(out, "{n}");
            }
            Task::Show(Item::List(list)) => {
                let identity = Rc::as_ptr(&list);
                if !inside.insert(identity) {
                    out.push_str("[...]");
                } else {
                    out.push('[');
                    tasks.push(Task::Leave(identity));
                    tasks.push(Task::Text("]"));
                    let items = list.borrow();
                    // Every item still to come needs at least one byte: never queue more than could fit.
                    if items.len() > limit {
                        return None;
                    }
                    for (i, child) in items.iter().enumerate().rev() {
                        tasks.push(Task::Show(child.clone()));
                        if i > 0 {
                            tasks.push(Task::Text(", "));
                        }
                    }
                }
            }
        }
    }
    (out.len() <= limit).then_some(out)
}
