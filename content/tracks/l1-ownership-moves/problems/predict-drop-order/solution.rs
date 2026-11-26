use std::cell::RefCell;

pub struct Noisy<'a> {
    pub name: &'static str,
    pub log: &'a RefCell<Vec<&'static str>>,
}

impl Drop for Noisy<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.name);
    }
}

pub struct Pair<'a> {
    pub first: Noisy<'a>,
    pub second: Noisy<'a>,
}

/// Don't change this function.
pub fn scene(log: &RefCell<Vec<&'static str>>) {
    let a = Noisy { name: "a", log };
    let _pair = Pair {
        first: Noisy { name: "first", log },
        second: Noisy {
            name: "second",
            log,
        },
    };
    let _ = Noisy {
        name: "ignored",
        log,
    };
    let b = Noisy { name: "b", log };
    let _x = Noisy { name: "x1", log };
    let _x = Noisy { name: "x2", log };
    let _v = vec![Noisy { name: "v0", log }, Noisy { name: "v1", log }];
    drop(a);
    match (Noisy {
        name: "scrutinee",
        log,
    })
    .name
    .len()
    {
        _ => {
            let _inner = Noisy { name: "inner", log };
        }
    }
    let _c = Noisy { name: "c", log };
    let _ = b;
}

/// The names in the order `scene` drops them.
/// `let _ = Noisy {..}` drops at once; `drop(a)` next; the match's temporary lives to the end of the match,
/// after the arm's local. Then locals in reverse order: `_c`, the Vec (its elements in order), the shadowing
/// `_x` then the shadowed one, `b` (`let _ = b` didn't move it), and `_pair`, whose fields drop in order.
pub const PREDICTED: [&str; 12] = [
    "ignored",
    "a",
    "inner",
    "scrutinee",
    "c",
    "v0",
    "v1",
    "x2",
    "x1",
    "b",
    "first",
    "second",
];
