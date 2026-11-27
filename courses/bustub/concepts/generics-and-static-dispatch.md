---
title: Generics versus trait objects: static and dynamic dispatch
summary: What monomorphisation does, when a generic function or impl Trait is the right tool, when dyn Trait is, what makes a trait object-safe, and which of the two this course's executors and expressions use.
minutes: 7
---
A function that works on "anything with these abilities" can be written two ways.

**Generic (static dispatch).** `fn total<T: Cost>(items: &[T]) -> u64`. The compiler makes a **separate copy for each concrete type** it is called with (*monomorphisation*), so every call is direct and can be inlined: no run-time cost, but more machine code and longer compiles. `impl Trait` in argument position is sugar for this; `impl Trait` as a return type means "one concrete type that I am not naming".

**Trait object (dynamic dispatch).** `fn total(items: &[Box<dyn Cost>]) -> u64`. One copy of the function; each call goes through a **vtable** pointer chosen at run time. A small per-call cost, no inlining across the call, but the collection can hold different types at once.

| | generics / `impl Trait` | `dyn Trait` |
|---|---|---|
| type known at compile time | yes | not needed |
| different types in one `Vec` | no | yes |
| per-call overhead | none | a vtable lookup |
| code size | grows per type | one copy |
| allowed traits | any | **object-safe** only |

Rule from the `rust-skills` zero-cost guide: if the type is known at compile time use generics; if it is decided at run time (a plan node, a plugin, a test fake behind the same seam) use `dyn`.

## Object safety

A trait can be a `dyn` only if its methods do not return `Self`, take generic parameters, or require `Self: Sized` (the compiler error is E0038). Put an escape hatch on a method with `where Self: Sized` to keep the rest object-safe.

## What the course picks, and why

- `Executor`, `Expression`, `PlanNode` children: **`Box<dyn Executor>`**, **`Arc<dyn Expression>`**. A query plan is built from SQL at run time, so the types of the nodes are not known at compile time. One virtual call per `next` batch (up to 20 tuples) is negligible next to the work.
- `Index`, `DiskIo`: **`dyn`** behind a seam so that tests can substitute a fake disk (the *trait objects and the disk seam* article).
- `PageArray<T, N>`, `GenericKey<N>`: **generics and const generics**, hot paths where the size and type are fixed and inlining matters.
- `SkipList<K, const MAX_HEIGHT, const SEED>`: generics for the key; a **boxed closure** for the comparison, since the comparison is chosen by the caller and one virtual call per comparison is acceptable.

The trade-off is not all-or-nothing: a generic function can take `&dyn Trait` to avoid bloat, and a `dyn` hot loop can be made generic when a profile says the vtable matters.

## C++ comparison

| C / C++ | Rust |
|---|---|
| templates (instantiated per type) | generics with trait bounds (checked at the definition, instantiated per type) |
| virtual functions and `unique_ptr<Base>` | `Box<dyn Trait>` |
| concepts (C++20) | trait bounds |
| CRTP for static polymorphism | generics with a trait |

**Port rule:** a C++ virtual base class used for a heterogeneous collection is `dyn Trait`; a template used for speed is a generic.

## In real code

### Using it: the same behaviour both ways

```rust test
trait Cost {
    fn cost(&self) -> u64;
}
struct Scan(u64);
struct Join(u64, u64);
impl Cost for Scan {
    fn cost(&self) -> u64 { self.0 }
}
impl Cost for Join {
    fn cost(&self) -> u64 { self.0 * self.1 }
}

fn total_static<T: Cost>(items: &[T]) -> u64 {
    items.iter().map(|i| i.cost()).sum()
}

fn total_dynamic(items: &[Box<dyn Cost>]) -> u64 {
    items.iter().map(|i| i.cost()).sum()
}

#[test]
fn generics_need_one_type_dyn_allows_a_mix() {
    assert_eq!(total_static(&[Scan(1), Scan(2)]), 3);
    let mixed: Vec<Box<dyn Cost>> = vec![Box::new(Scan(5)), Box::new(Join(3, 4))];
    assert_eq!(total_dynamic(&mixed), 17);
    // total_static(&[Scan(1), Join(2, 3)]) would not compile: two different types in one array
}

#[test]
fn impl_trait_in_argument_position_is_a_generic() {
    fn describe(c: &impl Cost) -> String { format!("cost {}", c.cost()) }
    assert_eq!(describe(&Join(2, 5)), "cost 10");
}
```

### Using it: object safety and the escape hatch

```rust test
trait Shape {
    fn area(&self) -> f64;
    fn doubled(&self) -> Self
    where
        Self: Sized; // returns Self: excluded from the vtable, so Shape stays object-safe
}

struct Square(f64);
impl Shape for Square {
    fn area(&self) -> f64 { self.0 * self.0 }
    fn doubled(&self) -> Square { Square(self.0 * 2.0) }
}

#[test]
fn a_where_self_sized_method_does_not_break_dyn() {
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(2.0))];
    assert_eq!(shapes[0].area(), 4.0);
    assert_eq!(Square(2.0).doubled().area(), 16.0);
}

#[test]
fn boxed_closures_are_dynamic_dispatch_on_a_function_type() {
    let by_desc: Box<dyn Fn(&i32, &i32) -> bool> = Box::new(|a, b| a > b);
    let mut v = vec![3, 1, 2];
    v.sort_by(|a, b| if by_desc(a, b) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
    assert_eq!(v, vec![3, 2, 1]);
}
```

### In the exercises

- **3d-01:** `Arc<dyn Expression>` trees and the downcast to inspect them.
- **3e-01:** `Box<dyn Executor>` children pulled in batches.
- **1b:** the disk seam as a trait object.
- **0b-01:** a boxed comparison closure in the skip list.

### Where it is used

- `Box<dyn Error>`, `dyn Fn`, `dyn Any` in the standard library; generic `Iterator` adapters everywhere.
- The `serde` and `tokio` ecosystems are heavily generic; plugin systems and GUI toolkits favour `dyn`.
