//! RECIPE 107 (language): ref / ref mut in patterns, match ergonomics, as_ref/as_deref, @ bindings, partial moves.
//! LEVEL: simple
//! Use in: reading old code, matching on borrowed data, taking parts of structs without moving them.   Run: cargo run --release --example ref_patterns
#![allow(dead_code)]
//!
//! WHAT: in a pattern, `&p` DESTRUCTURES a reference (it removes a layer of `&`), while `ref x` CREATES a reference to the matched
//! place (it adds one). `&x` in a pattern and `ref x` are opposites.
//! WHY: matching `Some(s)` on an `Option<String>` MOVES the string out. `ref` (or matching on a reference) borrows instead.
//! HOW: since Rust 2018 "default binding modes" make this automatic: matching a `&T` or `&mut T` against a non-reference pattern
//! binds fields by reference. Explicit `ref` is still needed for partial moves (move one field, borrow another from the same
//! value), for binding by reference when matching an owned value you keep using, and you will meet it in older code.
//! Note: in edition 2024 you may not write `ref`/`&` inside a pattern that already relies on default binding modes.
//! COMMON USES: `opt.as_deref()` to get `Option<&str>` from `Option<String>`, `if let Some(x) = map.get_mut(k)`,
//!   splitting a struct into owned and borrowed parts, `n @ 1..=9` range bindings in parsers and dispatchers.
//! Read: https://doc.rust-lang.org/reference/patterns.html#binding-modes and https://doc.rust-lang.org/std/option/enum.Option.html
#[derive(Debug)]
struct Person {
    name: String,
    nick: Option<String>,
    age: u32,
}

fn main() {
    // `&` destructures, `ref` borrows
    let pairs = vec![(1, 'a'), (2, 'b')];
    let sum: i32 = pairs.iter().map(|&(n, _)| n).sum(); // &(a, b) pattern on the &(i32,char) item
    assert_eq!(sum, 3);
    let v = 5;
    let ref r = v; // same as `let r = &v;`
    assert_eq!(*r, 5);
    let &copy = r; // peel the reference
    assert_eq!(copy, 5);

    // match ergonomics: matching on &Option<String> binds by reference
    let opt = Some(String::from("hi"));
    if let Some(s) = &opt {
        assert_eq!(s.len(), 2); // s: &String; opt NOT moved
    }
    assert!(opt.is_some());
    // matching on the owned value moves it:
    // if let Some(s) = opt { .. }  then `opt` is partly moved and unusable afterwards.
    if let Some(ref s) = opt {
        assert_eq!(s, "hi"); // explicit ref: also keeps opt
    }

    // ref mut and Option<&mut T>
    let mut o = Some(10);
    if let Some(ref mut x) = o {
        *x += 1;
    }
    if let Some(x) = &mut o {
        *x += 1;
    }
    if let Some(x) = o.as_mut() {
        *x += 1;
    }
    assert_eq!(o, Some(13));
    let m: Option<&mut i32> = o.as_mut();
    match m {
        Some(x) => *x = 0,
        None => unreachable!(),
    }
    assert_eq!(o, Some(0));

    // as_ref / as_deref
    let name: Option<String> = Some("ann".into());
    let n1: Option<&String> = name.as_ref();
    let n2: Option<&str> = name.as_deref();
    assert_eq!((n1.map(|s| s.len()), n2), (Some(3), Some("ann")));
    let mut buf: Option<Vec<u8>> = Some(vec![1]);
    if let Some(sl) = buf.as_deref_mut() {
        sl[0] = 9; // &mut [u8]
    }
    assert_eq!(buf, Some(vec![9]));
    let boxed: Option<Box<i32>> = Some(Box::new(4));
    assert_eq!(boxed.as_deref(), Some(&4));

    // struct patterns and partial moves
    let p = Person {
        name: "Bo".into(),
        nick: Some("bobo".into()),
        age: 30,
    };
    let Person { ref name, .. } = p; // borrow one field, p still whole
    assert_eq!(name, "Bo");
    let Person {
        nick,
        ref name,
        age,
    } = p; // MOVE nick, borrow name, copy age
    assert_eq!(
        (nick.as_deref(), name.as_str(), age),
        (Some("bobo"), "Bo", 30)
    );
    // println!("{:?}", p);        // ERROR: borrow of partially moved value `p` (nick moved)
    assert_eq!(p.name, "Bo"); // OK: the unmoved fields are still usable
    assert_eq!(p.age, 30);
    // let q = p;                  // ERROR: use of partially moved value

    // default binding modes on a reference to a struct
    let pr = &Person {
        name: "Cy".into(),
        nick: None,
        age: 5,
    };
    let Person {
        name: n, age: a, ..
    } = pr; // n: &String, a: &u32
    assert_eq!((n.as_str(), *a), ("Cy", 5));

    // @ bindings
    let describe = |x: u32| match x {
        n @ 1..=9 => format!("digit {n}"),
        n @ (10 | 20 | 30) => format!("round {n}"),
        n => format!("other {n}"),
    };
    assert_eq!(describe(7), "digit 7");
    assert_eq!(describe(20), "round 20");
    assert_eq!(describe(11), "other 11");
    let msg = Some(42);
    if let whole @ Some(n @ 40..=50) = msg {
        assert_eq!((whole, n), (Some(42), 42));
    }
    // slice pattern with a ref-binding rest
    let words = ["a".to_string(), "b".to_string(), "c".to_string()];
    if let [first, rest @ ..] = &words[..] {
        assert_eq!((first.as_str(), rest.len()), ("a", 2));
    }
    println!("ref patterns ok");
}
