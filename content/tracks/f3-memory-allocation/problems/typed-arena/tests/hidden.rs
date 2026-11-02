use solution::*;

#[test]
fn chunks_double() {
    check!(r#"chunk count for 4 + 8 + 16 + 32 values"#, { let a = Arena::new(); for i in 0..60 { a.alloc(i); } a.chunk_count() }, 4);
}

#[test]
fn chunks_grow_geometrically() {
    check!(r#"100k values need few chunks"#, { let a = Arena::new(); for i in 0..100_000u32 { a.alloc(i); } (a.len(), a.chunk_count() <= 16) }, (100_000, true));
}

#[test]
fn references_stay_valid_after_growth() {
    check!(r#"the very first ref after 50k more allocs"#, { let a = Arena::new(); let first = a.alloc(7u64); for i in 0..50_000u64 { a.alloc(i); } *first += 1; *first }, 8);
}

#[test]
fn strings_keep_their_heap_data() {
    check!(r#"values with their own allocations"#, { let a = Arena::new(); let refs: Vec<&mut String> = (0..1000).map(|i| a.alloc(format!("item-{i}"))).collect(); (refs[0].clone(), refs[999].clone(), refs.iter().map(|s| s.len()).sum::<usize>()) }, ("item-0".to_string(), "item-999".to_string(), 7890));
}

#[test]
fn zero_sized_values() {
    check!(r#"ZSTs are allowed"#, { let a = Arena::new(); for _ in 0..100 { a.alloc(()); } a.len() }, 100);
}

#[test]
fn drops_every_value_once_and_only_when_the_arena_goes() {
    use std::cell::Cell;
    use std::rc::Rc;
    struct D(Rc<Cell<usize>>);
    impl Drop for D {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let count = Rc::new(Cell::new(0));
    {
        let a = Arena::new();
        for _ in 0..1_000 {
            a.alloc(D(count.clone()));
        }
        check!("nothing dropped while the arena lives", count.get(), 0);
    }
    check!("all 1000 dropped exactly once", count.get(), 1_000);
}

#[test]
fn graph_with_cycles_and_plain_references() {
    use std::cell::Cell;
    struct Node<'a> {
        id: u32,
        next: Cell<Option<&'a Node<'a>>>,
    }
    let arena = Arena::new();
    let nodes: Vec<&Node> = (0..100).map(|id| &*arena.alloc(Node { id, next: Cell::new(None) })).collect();
    for i in 0..100 {
        nodes[i].next.set(Some(nodes[(i + 1) % 100])); // a ring: every node points at the next, the last at the first
    }
    let mut cur = nodes[0];
    let mut ids = Vec::new();
    for _ in 0..205 {
        ids.push(cur.id);
        cur = cur.next.get().unwrap();
    }
    check!("walking the ring wraps around", (ids[0], ids[99], ids[100], ids[204]), (0, 99, 0, 4));
}

#[test]
fn nested_arenas_hold_references_into_each_other() {
    let strings: Arena<String> = Arena::new();
    let views: Arena<&str> = Arena::new();
    let mut refs: Vec<&&str> = Vec::new();
    for i in 0..50 {
        let s: &String = strings.alloc(format!("s{i}"));
        refs.push(&*views.alloc(s.as_str()));
    }
    check!("a view into a string from another arena", (*refs[0], *refs[49]), ("s0", "s49"));
}
