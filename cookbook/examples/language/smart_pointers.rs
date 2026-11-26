//! RECIPE 38 (language): smart pointers: Box, Rc, Arc, Cell, RefCell, Weak, and when to use each.
//! LEVEL: simple
//! Use in: recursive types, shared ownership, graphs with parent links, shared state across threads.   Run: cargo run --example smart_pointers
//! COMMON USES: trees with parent links (DOM, ASTs), shared config, Arc<Mutex> state, recursive types.
//!
//!   Box<T>          one owner, heap allocated. Recursive types, trait objects (`Box<dyn Trait>`), big values.
//!   Rc<T>           MANY owners, single thread, immutable. Clone = bump a counter (cheap).
//!   Arc<T>          Rc but thread-safe (atomic counter). Use with Mutex/RwLock to mutate.
//!   Cell<T>         interior mutability for small Copy values (get/set), no borrowing at all.
//!   RefCell<T>      interior mutability checked AT RUNTIME (panics if you break the borrow rules).
//!   Weak<T>         non-owning pointer from Rc/Arc: break cycles (child -> parent).
//! Guidance: reach for plain ownership and indices first. `Rc<RefCell<T>>` everywhere is a smell (see index_graph/arena_ids).
//! Read: https://doc.rust-lang.org/book/ch15-00-smart-pointers.html
#![allow(dead_code)]
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

// Box: a recursive type must have a known size, so the recursion goes behind a pointer.
#[derive(Debug)]
enum Expr {
    Num(i64),
    Add(Box<Expr>, Box<Expr>),
}
fn eval(e: &Expr) -> i64 {
    match e {
        Expr::Num(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
    }
}

// Rc + RefCell + Weak: a tree whose children can reach their parent without a reference cycle leak.
struct Node {
    name: String,
    parent: RefCell<Weak<Node>>,
    children: RefCell<Vec<Rc<Node>>>,
}
fn node(name: &str) -> Rc<Node> {
    Rc::new(Node {
        name: name.into(),
        parent: RefCell::new(Weak::new()),
        children: RefCell::new(vec![]),
    })
}

fn main() {
    let e = Expr::Add(
        Box::new(Expr::Num(2)),
        Box::new(Expr::Add(Box::new(Expr::Num(3)), Box::new(Expr::Num(4)))),
    );
    println!("{}", eval(&e));

    let shared = Rc::new(String::from("shared config"));
    let a = Rc::clone(&shared);
    let b = shared.clone(); // same thing: clones the pointer, not the string
    println!("strong_count = {}", Rc::strong_count(&shared));
    drop(a);
    drop(b);
    println!("after drops = {}", Rc::strong_count(&shared));

    let counter = Cell::new(0);
    counter.set(counter.get() + 1); // no &mut needed
    println!("cell = {}", counter.get());

    let cache = RefCell::new(vec![1, 2]);
    cache.borrow_mut().push(3);
    {
        let _first = cache.borrow_mut();
        // a second borrow_mut now would PANIC at runtime; try_borrow_mut reports it instead:
        println!("second borrow ok? {}", cache.try_borrow_mut().is_ok());
    }
    println!("{:?}", cache.borrow());

    let root = node("root");
    let leaf = node("leaf");
    *leaf.parent.borrow_mut() = Rc::downgrade(&root); // Weak: does not keep root alive
    root.children.borrow_mut().push(Rc::clone(&leaf));
    println!(
        "leaf's parent = {:?}, root strong={} weak={}, leaf strong={}",
        leaf.parent.borrow().upgrade().map(|p| p.name.clone()),
        Rc::strong_count(&root),
        Rc::weak_count(&root),
        Rc::strong_count(&leaf)
    );
    println!(
        "children of root: {:?}",
        root.children
            .borrow()
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
    );
    drop(root);
    println!(
        "after root dropped, leaf's parent = {:?}",
        leaf.parent.borrow().upgrade().map(|p| p.name.clone())
    );
}
